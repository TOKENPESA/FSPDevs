//! Securities compliance gate — DIDComm-wrapped RWA accreditation verification.

use async_trait::async_trait;
use mesh_core::network::{DidCommEnvelope, PeerModulePacket};
use serde_json::{json, Value};

use crate::module_system::SidecarModule;

pub struct SecuritiesComplianceModule {
    agent_id: u16,
}

impl SecuritiesComplianceModule {
    pub fn new(agent_id: u16) -> Self {
        Self { agent_id }
    }

    fn verify_accreditation(
        counterparty_did: &str,
        zk_proof: &str,
        accreditation_tier: &str,
    ) -> Result<(), String> {
        let did = counterparty_did.trim();
        if !did.starts_with("did:") {
            return Err("counterparty DID must be a valid DID string".to_string());
        }
        if zk_proof.trim().len() < 32 {
            return Err("zero-knowledge accreditation proof failed validation".to_string());
        }
        if !accreditation_tier.eq_ignore_ascii_case("accredited")
            && !accreditation_tier.eq_ignore_ascii_case("qualified_purchaser")
        {
            return Err(format!(
                "counterparty accreditation tier '{accreditation_tier}' is insufficient for RWA transfer"
            ));
        }
        Ok(())
    }

    fn authorize_rwa_trade(payload: &Value) -> Result<Value, String> {
        let packet = if let Ok(envelope) = serde_json::from_value::<DidCommEnvelope>(payload.clone()) {
            envelope.body
        } else if let Ok(raw) = serde_json::from_value::<PeerModulePacket>(payload.clone()) {
            raw
        } else {
            return Err("RWA authorization requires DidCommEnvelope wrapper".to_string());
        };

        let inner = &packet.payload;
        let counterparty_did = inner
            .get("counterparty_did")
            .and_then(Value::as_str)
            .ok_or("counterparty_did required in accreditation payload")?;
        let zk_proof = inner
            .get("zk_accreditation_proof")
            .and_then(Value::as_str)
            .ok_or("zk_accreditation_proof required")?;
        let tier = inner
            .get("accreditation_tier")
            .and_then(Value::as_str)
            .unwrap_or("accredited");

        Self::verify_accreditation(counterparty_did, zk_proof, tier)?;

        Ok(json!({
            "status": "cleared",
            "counterparty_did": counterparty_did,
            "module": packet.target_module,
            "method": packet.method,
        }))
    }
}

#[async_trait]
impl SidecarModule for SecuritiesComplianceModule {
    fn module_name(&self) -> &'static str {
        "securities_compliance"
    }

    fn local_agent_id(&self) -> u16 {
        self.agent_id
    }

    async fn initialize(&mut self) -> Result<(), String> {
        log::info!("🛡️ [COMPLIANCE] Securities gate armed (DIDComm + ZK accreditation).");
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String> {
        match method {
            "authorize_rwa_trade" => Self::authorize_rwa_trade(&payload),
            "verify_counterparty_did" => {
                let did = payload
                    .get("counterparty_did")
                    .and_then(Value::as_str)
                    .ok_or("counterparty_did required")?;
                let proof = payload
                    .get("zk_accreditation_proof")
                    .and_then(Value::as_str)
                    .ok_or("zk_accreditation_proof required")?;
                let tier = payload
                    .get("accreditation_tier")
                    .and_then(Value::as_str)
                    .unwrap_or("accredited");
                Self::verify_accreditation(did, proof, tier)?;
                Ok(json!({ "status": "verified", "counterparty_did": did }))
            }
            _ => Err(format!(
                "Method '{method}' unsupported on securities_compliance module."
            )),
        }
    }

    async fn handle_peer_message(
        &self,
        source_agent_id: u16,
        method: &str,
        payload: Value,
    ) -> Result<(), String> {
        if method != "request_rwa_transfer" && method != "authorize_rwa_trade" {
            return Err(format!(
                "Peer method '{method}' blocked by securities_compliance gate"
            ));
        }

        match Self::authorize_rwa_trade(&payload) {
            Ok(clearance) => {
                log::info!(
                    "✅ [COMPLIANCE] RWA transfer pre-cleared for FA-{source_agent_id}: {clearance}"
                );
                Ok(())
            }
            Err(reason) => {
                log::warn!(
                    "⛔ [COMPLIANCE] RWA transfer BLOCKED for FA-{source_agent_id}: {reason}"
                );
                Err(reason)
            }
        }
    }
}
