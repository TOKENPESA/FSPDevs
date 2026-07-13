use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use fsp_fixed_math::TelcoFloatFixedPoint;
use crate::identity::resolve_agent_secret_key;
use crate::module_system::SidecarModule;
use crate::peer_packet::sign_peer_module_packet;
use crate::storage::{AgentDb, TelcoFloatSnapshot};
use mesh_core::network::PeerModulePacket;

fn snapshot_to_fixed_point(snapshot: TelcoFloatSnapshot) -> TelcoFloatFixedPoint {
    TelcoFloatFixedPoint {
        provider: snapshot.provider,
        account_id: snapshot.account_id,
        live_balance_units: snapshot.live_balance_units,
        critical_floor_units: snapshot.critical_floor_units,
    }
}

/// MFA control-plane supervisor agent id (unsigned peer instructions only).
const MFA_SUPERVISOR_AGENT_ID: u16 = 0;

pub struct TelcoB2cFiatSweepModule {
    agent_id: u16,
    db: Arc<AgentDb>,
    telco_api_url: String,
    http_client: reqwest::Client,
    outbound_tx: Option<mpsc::Sender<PeerModulePacket>>,
}

impl TelcoB2cFiatSweepModule {
    pub fn new(agent_id: u16, db: Arc<AgentDb>) -> Self {
        let telco_api_url = std::env::var("MFA_TELCO_CLEARING_API_URL").unwrap_or_else(|_| {
            "https://api.telecom-gateway.internal/v1/b2c".to_string()
        });
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            agent_id,
            db,
            telco_api_url,
            http_client,
            outbound_tx: None,
        }
    }
}

#[async_trait]
impl SidecarModule for TelcoB2cFiatSweepModule {
    fn module_name(&self) -> &'static str {
        "telco_b2c_sweep"
    }

    fn local_agent_id(&self) -> u16 {
        self.agent_id
    }

    fn set_outbound_channel(&mut self, tx: mpsc::Sender<PeerModulePacket>) {
        self.outbound_tx = Some(tx);
    }

    async fn initialize(&mut self) -> Result<(), String> {
        log::info!("🔌 [TELCO MODULE] Initialized plug-and-play fiat sweep framework.");
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String> {
        match method {
            "get_float_status" => {
                let account_id = payload["account_id"].as_str().ok_or("Missing account_id")?;
                let float_record = snapshot_to_fixed_point(
                    self.db.get_telco_float_record(account_id)?,
                );
                Ok(json!({ "status": "success", "data": float_record }))
            }
            "trigger_manual_sweep" => {
                let amount_units = payload["amount_units"]
                    .as_u64()
                    .ok_or("Invalid amount_units")?;
                let provider = payload["provider"].as_str().ok_or("Missing provider")?;

                log::info!(
                    "⚡ [TELCO MODULE] Manual execution of B2C sweep triggered. Amount: {} units.",
                    amount_units
                );

                if let Some(ref tx) = self.outbound_tx {
                    let packet = PeerModulePacket {
                        source_agent_id: self.agent_id,
                        target_agent_id: MFA_SUPERVISOR_AGENT_ID,
                        target_module: self.module_name().to_string(),
                        method: "request_mfa_clearing_allocation".to_string(),
                        payload: json!({ "provider": provider, "amount_units": amount_units }),
                        signature: None,
                    };
                    let secret = resolve_agent_secret_key(self.agent_id)?;
                    let signed = sign_peer_module_packet(packet, &secret)?;
                    tx.send(signed)
                        .await
                        .map_err(|e| format!("Outbound channel failure: {e}"))?;
                }

                Ok(json!({
                    "status": "staged",
                    "message": "Sweep request dispatched to MFA Clearinghouse."
                }))
            }
            _ => Err(format!(
                "Method '{method}' unsupported on telco_b2c_sweep module."
            )),
        }
    }

    async fn handle_peer_message(
        &self,
        source_agent_id: u16,
        method: &str,
        payload: Value,
    ) -> Result<(), String> {
        if source_agent_id != MFA_SUPERVISOR_AGENT_ID {
            return Err(
                "Security Violation: Telco module only ingests instructions signed by MFA Supervisor."
                    .to_string(),
            );
        }

        match method {
            "execute_clearing_injection" => {
                let allocation_id = payload["allocation_id"]
                    .as_str()
                    .ok_or("Missing allocation_id")?;
                let injection_amount = payload["amount_units"]
                    .as_u64()
                    .ok_or("Missing amount_units")?;
                let account_id = payload["account_id"]
                    .as_str()
                    .unwrap_or("primary");

                log::info!(
                    "💰 [TELCO MODULE] MFA Clearinghouse approved allocation [{allocation_id}]. Executing real fiat injection loop..."
                );

                let telco_payload = json!({
                    "agent_id": self.agent_id,
                    "action": "CREDIT_FLOAT",
                    "value_units": injection_amount
                });

                match self
                    .http_client
                    .post(&self.telco_api_url)
                    .json(&telco_payload)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        log::info!(
                            "✅ [TELCO MODULE] Carrier B2C settlement confirmed. Updating local ledger boundaries."
                        );
                        self.db
                            .increment_local_fiat_float(account_id, injection_amount)?;
                        Ok(())
                    }
                    Ok(failed_resp) => Err(format!(
                        "Carrier endpoint rejected settlement: Status {}",
                        failed_resp.status()
                    )),
                    Err(err) => Err(format!(
                        "Network timeout hitting Carrier B2C API bridge: {err}"
                    )),
                }
            }
            _ => Err(format!(
                "Peer method '{method}' unsupported on telco_b2c_sweep module."
            )),
        }
    }
}
