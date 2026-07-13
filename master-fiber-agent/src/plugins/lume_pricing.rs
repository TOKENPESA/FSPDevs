//! LUME FX & spread pricing — injects liquidity premiums into routing costs.

use async_trait::async_trait;
use mesh_core::types::L2Asset;
use serde_json::Value;

use crate::traits::{ClearanceVerdict, MfaPolicyPlugin, PolicyError, RoutingIntent};

const RGBPP_SPREAD_BPS: u32 = 25;
const XUDT_SPREAD_BPS: u32 = 15;

pub struct LumePricingEngine;

impl LumePricingEngine {
    pub fn new() -> Self {
        Self
    }

    fn spread_delta(base_weight: u32, spread_bps: u32) -> u32 {
        let premium = (u64::from(base_weight) * u64::from(spread_bps)) / 10_000;
        base_weight.saturating_add(u32::try_from(premium).unwrap_or(u32::MAX))
    }
}

impl Default for LumePricingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MfaPolicyPlugin for LumePricingEngine {
    fn plugin_name(&self) -> &'static str {
        "lume_pricing"
    }

    async fn on_heartbeat(&self, _agent_id: &str, _payload: &Value) -> Result<(), PolicyError> {
        Ok(())
    }

    fn adjust_edge_weight(
        &self,
        _source: &str,
        _target: &str,
        asset: &L2Asset,
        base_weight: u32,
    ) -> u32 {
        match asset {
            L2Asset::RgbPlusPlus(_) => Self::spread_delta(base_weight, RGBPP_SPREAD_BPS),
            L2Asset::UDT(_) => Self::spread_delta(base_weight, XUDT_SPREAD_BPS),
            _ => base_weight,
        }
    }

    async fn pre_route_clearance(
        &self,
        _intent: &RoutingIntent,
    ) -> Result<ClearanceVerdict, PolicyError> {
        Ok(ClearanceVerdict::Approved)
    }
}
