use serde::{Deserialize, Serialize};

pub use mesh_core::MeshPulsePayload;

#[derive(Debug, Clone)]
pub struct PaymentExecResult {
    pub success: bool,
    pub payment_hash: Option<String>,
    pub fee_shannons: Option<u64>,
    pub error: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SidecarPaymentReply {
    pub command: String,
    pub payment_id: String,
    #[serde(default)]
    pub status: String,
    pub payment_hash: Option<String>,
    pub fee_shannons: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SimulationConfigPayload {
    pub edge_nodes: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SimulationConfigResponse {
    pub edge_nodes: u16,
    pub ring_max: u16,
    pub grid_dim: u16,
    pub fleet_hint: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteRequestPayload {
    pub source: u16,
    pub destination: u16,
    pub amount_shannons: u64,
    #[serde(default)]
    pub active_network_limit: Option<u16>,
    /// When true (default), MFA dispatches a keysend payment on the source sidecar after routing.
    #[serde(default)]
    pub execute: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteResponse {
    pub status: String,
    pub path: Vec<u16>,
    pub execution_latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_fee_shannons: Option<u64>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigUpdatePayload {
    pub command: String,
    pub target_peer_id: u16,
    pub alternative_peer_id: u16,
}
