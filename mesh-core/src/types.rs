use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical schema version tag for control plane tracking
pub const MESH_MONITOR_SCHEMA_VERSION: &str = "1.0.0";

/// Sidecar → MFA telemetry envelope (HTTP POST /telemetry).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MeshPulsePayload {
    pub status: String,
    #[serde(alias = "reporter")]
    pub agent: u16,
    pub active_mesh_neighbors: Vec<u16>,
    #[serde(alias = "target")]
    pub report_target: u16,
    pub attempt: u8,
    /// Unix seconds (UTC). Required for MFA ingest; bound into the signed canonical message.
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_hex: Option<String>,
    /// Fiber node secp256k1 pubkey (for hub funding) — distinct from telemetry signing key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fnn_pubkey_hex: Option<String>,
    /// Optional Fiber P2P multiaddr for hub `connect_peer` when pubkey is not in gossip yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_connect_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_shannons: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_shannons: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEnvelope {
    pub schema_version: String,
    pub timestamp: u64,
    pub event_id: Uuid,
    #[serde(flatten)]
    pub data: MonitorEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum MonitorEventData {
    #[serde(rename = "REQUITY_INJECTION")]
    LiquidityInjection {
        node: u16,
        amount_shannons: u64,
        vault: String,
    },
    #[serde(rename = "TOPOLOGY_SYNC")]
    TopologySync {
        version: u64,
        updated_channels_count: usize,
    },
    #[serde(rename = "COPILOT_PREDICTION_ALERT")]
    CopilotAlert {
        node: u16,
        channel_id: String,
        drain_rate_shannons_sec: f64,
        seconds_remaining: f64,
    },
    #[serde(rename = "INTENT_SWAP_SUCCESS")]
    IntentSwapSuccess {
        swap_id: Uuid,
        amount: u64,
    },
}

impl MonitorEnvelope {
    pub fn wrap(data: MonitorEventData) -> Self {
        Self {
            schema_version: MESH_MONITOR_SCHEMA_VERSION.to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            event_id: Uuid::new_v4(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_envelope_serializes_with_event_tag() {
        let envelope = MonitorEnvelope::wrap(MonitorEventData::TopologySync {
            version: 42,
            updated_channels_count: 3,
        });
        let json = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(json["schema_version"], MESH_MONITOR_SCHEMA_VERSION);
        assert_eq!(json["event"], "TOPOLOGY_SYNC");
        assert_eq!(json["payload"]["version"], 42);
    }
}
