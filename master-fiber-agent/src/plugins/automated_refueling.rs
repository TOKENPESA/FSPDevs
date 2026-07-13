//! Treasury copilot — predictive refueling intents from heartbeat capacity telemetry.

use async_trait::async_trait;
use mesh_core::types::L2Asset;
use serde_json::Value;

use crate::traits::{ClearanceVerdict, MfaPolicyPlugin, PolicyError, RoutingIntent};

pub struct AutomatedRefuelingBrain {
    critical_capacity_floor: u64,
}

impl AutomatedRefuelingBrain {
    pub fn new(critical_capacity_floor: u64) -> Self {
        Self {
            critical_capacity_floor,
        }
    }
}

#[async_trait]
impl MfaPolicyPlugin for AutomatedRefuelingBrain {
    fn plugin_name(&self) -> &'static str {
        "automated_refueling"
    }

    async fn on_heartbeat(&self, agent_id: &str, payload: &Value) -> Result<(), PolicyError> {
        let local_ckb = payload
            .get("local_capacity_shannons")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        if local_ckb > 0 && local_ckb < self.critical_capacity_floor {
            log::warn!(
                "⛽ [TREASURY COPILOT] Predictive refueling intent for {agent_id}: \
                 CKB outbound {local_ckb} < floor {}",
                self.critical_capacity_floor
            );
        }

        if let Some(capacities) = payload.get("asset_capacities").and_then(Value::as_array) {
            for entry in capacities {
                let amount = entry
                    .get("amount_atomic")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let is_low_udt = entry
                    .get("asset")
                    .and_then(Value::as_object)
                    .is_some_and(|asset| asset.contains_key("UDT") || asset.contains_key("xUDT"));
                let is_low_rgb = entry
                    .get("asset")
                    .and_then(Value::as_object)
                    .is_some_and(|asset| asset.contains_key("RGB++"));

                if amount > 0
                    && amount < self.critical_capacity_floor
                    && (is_low_udt || is_low_rgb)
                {
                    log::warn!(
                        "⛽ [TREASURY COPILOT] Multi-asset refueling watch on {agent_id}: \
                         capacity {amount} below floor {}",
                        self.critical_capacity_floor
                    );
                }
            }
        }

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
        _intent: &RoutingIntent,
    ) -> Result<ClearanceVerdict, PolicyError> {
        Ok(ClearanceVerdict::Approved)
    }
}
