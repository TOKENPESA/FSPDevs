use std::sync::Arc;

use async_trait::async_trait;
use mesh_core::dicoba_logic::DicobaEngine;
use mesh_core::jungukuu_types::{DicobaMember, JunguKuuVault};
use mesh_core::network::PeerModulePacket;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::dicoba_bridge::DicobaEdgeClient;
use crate::fnn_client::FiberNodeRpc;
use crate::module_system::SidecarModule;
use crate::resolve_dicoba_vault_id;
use crate::storage::AgentDb;

pub struct DicobaModule {
    agent_id: u16,
    db: Arc<AgentDb>,
    edge_client: DicobaEdgeClient,
    local_member_id: Uuid,
    outbound_tx: Option<mpsc::Sender<PeerModulePacket>>,
}

impl DicobaModule {
    pub fn new(
        agent_id: u16,
        db: Arc<AgentDb>,
        fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
        member_id: Uuid,
    ) -> Self {
        let edge_client = DicobaEdgeClient::new(db.clone(), fnn_client, member_id);
        Self {
            agent_id,
            db,
            edge_client,
            local_member_id: member_id,
            outbound_tx: None,
        }
    }
}

#[async_trait]
impl SidecarModule for DicobaModule {
    fn module_name(&self) -> &'static str {
        "dicoba"
    }

    fn local_agent_id(&self) -> u16 {
        self.agent_id
    }

    async fn initialize(&mut self) -> Result<(), String> {
        log::info!("DICOBA Module Ready: Initialized local JunguKuu ledger.");
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String> {
        match method {
            "stream_micro_contribution" => {
                let vault: JunguKuuVault = serde_json::from_value(
                    payload
                        .get("vault_config")
                        .cloned()
                        .ok_or_else(|| "missing vault_config".to_string())?,
                )
                .map_err(|e| format!("Invalid vault config: {e}"))?;
                let amount_fiat = payload["amount_fiat"].as_f64().unwrap_or(0.0);
                let rate = payload["shannons_conversion_rate"].as_f64().unwrap_or(38.0);
                let atomic_shannons = (amount_fiat * rate).round().max(0.0) as u64;
                let receipt = self
                    .edge_client
                    .stream_contribution(&vault, atomic_shannons)
                    .await?;
                serde_json::to_value(receipt)
                    .map_err(|err| format!("receipt serialization failed: {err}"))
            }
            "get_vault_context" => {
                let group_name = payload
                    .get("group_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if group_name.is_empty() {
                    return Err("group_name is required".to_string());
                }
                let vault_id = resolve_dicoba_vault_id(&group_name);
                Ok(serde_json::json!({
                    "group_name": group_name,
                    "vault_id": vault_id,
                    "local_member_id": self.local_member_id,
                }))
            }
            "list_vault_contributors" => {
                let group_name = payload
                    .get("group_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if group_name.is_empty() {
                    return Err("group_name is required".to_string());
                }
                let vault_id = payload
                    .get("vault_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| resolve_dicoba_vault_id(&group_name).to_string());
                let contributors = self
                    .db
                    .list_vault_contributors(&vault_id, &group_name)?;
                Ok(serde_json::json!({
                    "group_name": group_name,
                    "vault_id": vault_id,
                    "contributors": contributors,
                }))
            }
            "list_member_vaults" => {
                let member_id = payload
                    .get("member_id")
                    .and_then(Value::as_str)
                    .and_then(|raw| Uuid::parse_str(raw).ok())
                    .unwrap_or(self.local_member_id);
                let vaults = self.db.list_member_vaults(&member_id.to_string())?;
                Ok(serde_json::json!({
                    "member_id": member_id,
                    "vaults": vaults,
                }))
            }
            "stream_weekly_contribution" => {
                let mut vault: JunguKuuVault =
                    serde_json::from_value(payload["vault_config"].clone())
                        .map_err(|e| format!("Invalid vault config: {e}"))?;
                let shares = payload["shares_to_buy"].as_u64().unwrap_or(0) as u32;
                let pay_social = payload["pay_social_fund"].as_bool().unwrap_or(false);
                let fines = payload["fines_shannons"].as_u64().unwrap_or(0);

                let total_shannons = DicobaEngine::process_weekly_contribution(
                    &mut vault,
                    self.local_member_id,
                    shares,
                    pay_social,
                    fines,
                )?;

                let receipt = self
                    .edge_client
                    .stream_contribution(&vault, total_shannons)
                    .await?;

                Ok(serde_json::json!({
                    "status": "success",
                    "shannons_routed": total_shannons,
                    "receipt": receipt,
                }))
            }
            "get_credit_profile" => {
                let vault: JunguKuuVault = serde_json::from_value(
                    payload
                        .get("vault_config")
                        .cloned()
                        .ok_or_else(|| "missing vault_config".to_string())?,
                )
                .map_err(|e| format!("Invalid vault config: {e}"))?;
                let member_id = payload
                    .get("member_id")
                    .and_then(Value::as_str)
                    .and_then(|raw| Uuid::parse_str(raw).ok())
                    .unwrap_or(self.local_member_id);
                let conversion_rate = payload["shannons_conversion_rate"].as_f64().unwrap_or(38.0);

                let member = vault
                    .members
                    .iter()
                    .find(|member| member.member_id == member_id)
                    .cloned()
                    .unwrap_or_else(|| DicobaMember::new(member_id, ""));

                let max_borrow_shannons =
                    member.maximum_borrowing_capacity(vault.share_price_shannons);
                let rate_bps = vault.current_utilization_interest_rate();

                Ok(serde_json::json!({
                    "max_borrowing_capacity_shannons": max_borrow_shannons,
                    "max_borrowing_capacity_fiat": max_borrow_shannons as f64 / conversion_rate,
                    "current_interest_rate_bps": rate_bps,
                    "current_interest_rate_monthly_pct": rate_bps as f64 / 100.0,
                    "digital_reputation_score": member.digital_reputation_score,
                }))
            }
            "request_loan" => {
                let requested_shannons = payload["principal_shannons"]
                    .as_u64()
                    .ok_or("Missing or invalid principal_shannons")?;

                let guarantor_id_str = payload["guarantor_id"]
                    .as_str()
                    .ok_or("Missing guarantor_id")?;

                let guarantor_id = Uuid::parse_str(guarantor_id_str)
                    .map_err(|_| "Invalid UUID format for guarantor".to_string())?;

                let group_name = payload
                    .get("group_name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string);
                let vault_id = group_name
                    .as_ref()
                    .map(|name| resolve_dicoba_vault_id(name).to_string());

                log::info!(
                    "🏦 [DICOBA] Staging loan for {} Shannons. Guarantor stake requested from: {}{}",
                    requested_shannons,
                    guarantor_id,
                    group_name
                        .as_ref()
                        .map(|name| format!(" · vault: {name}"))
                        .unwrap_or_default()
                );

                Ok(serde_json::json!({
                    "status": "pending_guarantor_signatures",
                    "staged_principal": requested_shannons,
                    "guarantor_id": guarantor_id,
                    "borrower_member_id": self.local_member_id,
                    "group_name": group_name,
                    "vault_id": vault_id,
                    "message": "Loan contract generated. Waiting for guarantor to counter-sign and lock shares."
                }))
            }
            _ => Err(format!("Method '{method}' not supported by the DICOBA module")),
        }
    }

    async fn handle_peer_message(
        &self,
        source_agent_id: u16,
        method: &str,
        payload: Value,
    ) -> Result<(), String> {
        match method {
            "request_guarantor_signature" => {
                let guarantor_id = payload
                    .get("guarantor_member_id")
                    .or_else(|| payload.get("guarantor_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "missing guarantor_member_id".to_string())?;
                let loan_id = payload
                    .get("loan_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unspecified");
                let principal = payload
                    .get("principal_shannons")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);

                log::info!(
                    "🤝 [DICOBA P2P] FA-{source_agent_id} requested guarantor signature for loan {loan_id} · guarantor {guarantor_id} · {principal} shannons"
                );

                if guarantor_id != self.local_member_id.to_string() {
                    return Err(format!(
                        "guarantor_member_id {guarantor_id} does not match local member {}",
                        self.local_member_id
                    ));
                }

                Ok(())
            }
            _ => Err(format!(
                "Peer method '{method}' not supported by the DICOBA module"
            )),
        }
    }

    fn set_outbound_channel(&mut self, tx: mpsc::Sender<PeerModulePacket>) {
        self.outbound_tx = Some(tx);
    }
}
