pub mod daemon;
pub mod fnn_client;
pub mod hot_swap;
pub mod identity;
pub mod mesh;
pub mod mesh_ports;
pub mod payment;
pub mod storage;
pub mod telemetry;

use std::env;

use fnn_client::{FiberNodeRpc, LiveFnnClient, SimulatedFnnClient};
use secp256k1::{PublicKey, Secp256k1};
use serde::{Deserialize, Serialize};

pub use daemon::{run_agent_sidecar, SidecarConfig};
pub use hot_swap::{execute_hot_swap, refresh_pubkey_cache};
pub use identity::{attach_telemetry_signature, resolve_agent_secret_key, resolve_agent_signing_key};
pub use mesh::{
    mesh_unix_timestamp_secs, parse_agent_id, resolve_open_channel_shannons, MeshPubkeyRegistry,
    MeshPulsePayload, RING_SIZE,
};
pub use mesh_ports::{parse_fleet_range, resolve_fnn_rpc_url};
pub use payment::execute_mesh_payment;
pub use storage::AgentDb;
pub use telemetry::{flush_queued_telemetry, post_telemetry, send_or_queue_telemetry};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MeshChannelState {
    pub peer_id: u16,
    pub nonce: u64,
    pub consecutive_failures: u8,
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_pubkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub local_balance_shannons: u64,
    #[serde(default)]
    pub remote_balance_shannons: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConfigUpdatePayload {
    pub command: String,
    #[serde(default)]
    pub target_peer_id: u16,
    #[serde(default)]
    pub alternative_peer_id: u16,
    #[serde(default)]
    pub payment_id: Option<String>,
    #[serde(default)]
    pub destination_agent: Option<u16>,
    #[serde(default)]
    pub target_fnn_pubkey: Option<String>,
    #[serde(default)]
    pub amount_shannons: Option<u64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PaymentResultPayload {
    pub command: String,
    pub payment_id: String,
    pub agent: u16,
    pub destination_agent: u16,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_shannons: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn agent_fnn_pubkey_result(agent_id: u16) -> Result<String, String> {
    let secret = resolve_agent_secret_key(agent_id)?;
    let secp = Secp256k1::signing_only();
    let pubkey = PublicKey::from_secret_key(&secp, &secret);
    Ok(hex::encode(pubkey.serialize()))
}

pub fn agent_fnn_pubkey(agent_id: u16) -> String {
    agent_fnn_pubkey_result(agent_id).unwrap_or_else(|e| {
        eprintln!("⚠️ [FNN] agent_fnn_pubkey(FA-{agent_id}) failed: {e}");
        String::new()
    })
}

pub fn peer_id_from_agent_pubkey(peer_public_key: &str) -> Option<u16> {
    mesh_core::peer_id_from_agent_pubkey(peer_public_key)
}

pub fn aggregate_active_balances(channels: &[MeshChannelState]) -> (u64, u64) {
    channels
        .iter()
        .filter(|ch| ch.is_active)
        .fold((0u64, 0u64), |(outbound, inbound), ch| {
            (
                outbound.saturating_add(ch.local_balance_shannons),
                inbound.saturating_add(ch.remote_balance_shannons),
            )
        })
}

pub async fn resolve_fnn_backend(
    agent_id: u16,
    rpc_url: &str,
) -> Box<dyn FiberNodeRpc + Send + Sync> {
    let mode = env::var("FNN_MODE").unwrap_or_default();
    if mode.eq_ignore_ascii_case("simulate") || mode.eq_ignore_ascii_case("sim") {
        println!("🧪 [FNN] Simulation mode enabled (FNN_MODE={mode})");
        return Box::new(SimulatedFnnClient::new(agent_id));
    }

    let live = LiveFnnClient::new(rpc_url.to_string());
    match live.list_channels().await {
        Ok(_) => {
            println!("🔗 [FNN] Live RPC connected at {rpc_url}");
            Box::new(live)
        }
        Err(e) => {
            eprintln!("⚠️ [FNN] Daemon not reachable at {rpc_url} — using simulated channels");
            eprintln!("   Reason: {e}");
            eprintln!("   Tip: start your FNN node, or set FNN_MODE=simulate for local dev");
            Box::new(SimulatedFnnClient::new(agent_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_fnn_pubkey_is_secp256k1_hex() {
        let pk = agent_fnn_pubkey(44);
        assert!(pk.len() >= 66);
        assert!(pk.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(peer_id_from_agent_pubkey(&pk), Some(44));
    }

    #[test]
    fn peer_id_from_agent_pubkey_supports_legacy_sim_peer() {
        assert_eq!(peer_id_from_agent_pubkey("sim-peer-44"), Some(44));
    }

    #[test]
    fn aggregate_active_balances_sums_active_channels_only() {
        let channels = vec![
            MeshChannelState {
                peer_id: 45,
                nonce: 1,
                consecutive_failures: 0,
                is_active: true,
                peer_pubkey: None,
                channel_id: None,
                local_balance_shannons: 100,
                remote_balance_shannons: 200,
            },
            MeshChannelState {
                peer_id: 46,
                nonce: 1,
                consecutive_failures: 0,
                is_active: false,
                peer_pubkey: None,
                channel_id: None,
                local_balance_shannons: 1_000,
                remote_balance_shannons: 2_000,
            },
        ];

        assert_eq!(aggregate_active_balances(&channels), (100, 200));
    }

    #[test]
    fn sign_telemetry_includes_neighbors_in_canonical() {
        let secret_key =
            resolve_agent_secret_key(44).expect("resolve dev signing key for FA-44");

        let payload = MeshPulsePayload {
            status: "MESH_HEARTBEAT".to_string(),
            agent: 44,
            active_mesh_neighbors: vec![45, 46],
            report_target: 44,
            attempt: 0,
            timestamp: mesh_unix_timestamp_secs(),
            public_key_hex: None,
            signature_hex: None,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            outbound_shannons: None,
            inbound_shannons: None,
        };

        let signed = attach_telemetry_signature(payload, &secret_key);
        assert!(signed.public_key_hex.is_some());
        assert!(signed.signature_hex.is_some());
    }
}
