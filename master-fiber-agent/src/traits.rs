//! MFA policy plugin interconnect — strict boundary between physics and rules.

use async_trait::async_trait;
use mesh_core::types::{FloatExhaustionTelemetry, L2Asset};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::clearing::{MatchedSwapLeg, MultiAssetCrossClearingIntent};

/// Routing request evaluated by policy plugins before HTLC manifest generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingIntent {
    pub source: u16,
    pub destination: u16,
    pub amount_atomic: u64,
    pub target_asset: L2Asset,
    pub path: Vec<u16>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearanceVerdict {
    Approved,
    Rejected(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PolicyError {
    #[error("policy plugin '{0}' rejected operation: {1}")]
    Rejected(&'static str, String),
    #[error("policy plugin '{0}' internal error: {1}")]
    Internal(&'static str, String),
    #[error("invalid policy payload: {0}")]
    InvalidPayload(String),
}

/// Clearinghouse / regulatory hook surface — never embed business rules in Dijkstra core.
#[async_trait]
pub trait MfaPolicyPlugin: Send + Sync {
    fn plugin_name(&self) -> &'static str;

    async fn on_heartbeat(&self, agent_id: &str, payload: &Value) -> Result<(), PolicyError>;

    /// Synchronous edge-cost adjustment: W_final = f(W_base) chained across plugins.
    fn adjust_edge_weight(
        &self,
        source: &str,
        target: &str,
        asset: &L2Asset,
        base_weight: u32,
    ) -> u32;

    async fn pre_route_clearance(
        &self,
        intent: &RoutingIntent,
    ) -> Result<ClearanceVerdict, PolicyError>;
}

/// Clearinghouse swap / settlement plugin — business rules isolated from routing physics.
#[async_trait]
pub trait MfaClearingPlugin: Send + Sync {
    fn plugin_name(&self) -> &'static str;

    async fn handle_float_crisis(
        &self,
        state: std::sync::Arc<crate::state::AppState>,
        telemetry: FloatExhaustionTelemetry,
    ) -> Result<(), String>;

    async fn handle_multi_asset_cross_clearing(
        &self,
        state: std::sync::Arc<crate::state::AppState>,
        intent: MultiAssetCrossClearingIntent,
    ) -> Result<Vec<MatchedSwapLeg>, String>;

    async fn run_multi_asset_match_loop(
        &self,
        state: std::sync::Arc<crate::state::AppState>,
        intents: Vec<MultiAssetCrossClearingIntent>,
    ) -> Vec<Result<MatchedSwapLeg, String>> {
        let mut results = Vec::with_capacity(intents.len());
        for intent in intents {
            match self
                .handle_multi_asset_cross_clearing(state.clone(), intent)
                .await
            {
                Ok(legs) => results.extend(legs.into_iter().map(Ok)),
                Err(err) => results.push(Err(err)),
            }
        }
        results
    }
}
