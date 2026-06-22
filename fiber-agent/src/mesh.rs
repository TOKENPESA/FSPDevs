//! Sidecar-specific mesh helpers; shared logic lives in `mesh-core`.

use std::env;

pub use mesh_core::{
    agent_fnn_pubkey, chord_peer, is_live_fiber_pubkey, mesh_neighbor_ids,
    mesh_unix_timestamp_secs, neighbors_canonical, ring_peer, shannons_to_hex, skip_peer,
    telemetry_canonical_message, MeshPubkeyRegistry, MeshPulsePayload, RING_SIZE,
    DEFAULT_OPEN_CHANNEL_SHANNONS,
};

pub fn parse_agent_id() -> Result<u16, String> {
    let id: u16 = env::var("AGENT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(44);
    if !(1..=RING_SIZE).contains(&id) {
        return Err(format!("AGENT_ID must be 1..={RING_SIZE}, got {id}"));
    }
    Ok(id)
}

pub fn resolve_open_channel_shannons() -> u64 {
    env::var("FNN_OPEN_CHANNEL_SHANNONS")
        .or_else(|_| env::var("HUB_FUNDING_SHANNONS"))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_OPEN_CHANNEL_SHANNONS)
}
