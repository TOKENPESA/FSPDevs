//! Sovereign compliance gate — DIDComm credential verification for RWA routes.

use std::collections::HashSet;

use async_trait::async_trait;
use mesh_core::types::L2Asset;
use serde_json::Value;

use crate::traits::{ClearanceVerdict, MfaPolicyPlugin, PolicyError, RoutingIntent};

pub struct SovereignComplianceFilter {
    approved_dids: HashSet<String>,
}

impl SovereignComplianceFilter {
    pub fn new() -> Self {
        let mut approved_dids = HashSet::new();
        approved_dids.insert("did:fsp:accredited:issuer-alpha".to_string());
        approved_dids.insert("did:fsp:qualified:corp-treasury".to_string());
        Self { approved_dids }
    }

    fn requires_clearance(asset: &L2Asset) -> bool {
        matches!(asset, L2Asset::RgbPlusPlus(_) | L2Asset::UDT(_))
    }
}

impl Default for SovereignComplianceFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MfaPolicyPlugin for SovereignComplianceFilter {
    fn plugin_name(&self) -> &'static str {
        "sovereign_compliance"
    }

    async fn on_heartbeat(&self, _agent_id: &str, _payload: &Value) -> Result<(), PolicyError> {
        Ok(())
    }

    fn adjust_edge_weight(
        &self,
        _source: &str,
        _target: &str,
        _asset: &L2Asset,
        base_weight: u32,
    ) -> u32 {
        base_weight
    }

    async fn pre_route_clearance(
        &self,
        intent: &RoutingIntent,
    ) -> Result<ClearanceVerdict, PolicyError> {
        if !Self::requires_clearance(&intent.target_asset) {
            return Ok(ClearanceVerdict::Approved);
        }

        let counterparty_did = intent
            .metadata
            .get("counterparty_did")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|did| !did.is_empty());

        let sender_did = intent
            .metadata
            .get("sender_did")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|did| !did.is_empty());

        let zk_proof = intent
            .metadata
            .get("zk_accreditation_proof")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|proof| proof.len() >= 32);

        match (counterparty_did, sender_did, zk_proof) {
            (Some(counterparty), Some(sender), Some(_proof))
                if counterparty.starts_with("did:") && sender.starts_with("did:") =>
            {
                if self.approved_dids.contains(counterparty) && self.approved_dids.contains(sender)
                {
                    Ok(ClearanceVerdict::Approved)
                } else {
                    Ok(ClearanceVerdict::Rejected(
                        "counterparty or sender DID not in sovereign compliance registry"
                            .to_string(),
                    ))
                }
            }
            _ => Ok(ClearanceVerdict::Rejected(
                "RWA route requires sender_did, counterparty_did, and zk_accreditation_proof"
                    .to_string(),
            )),
        }
    }
}
