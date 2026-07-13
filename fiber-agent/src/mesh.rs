//! Sidecar-specific mesh helpers; shared logic lives in `mesh-core`.

use std::env;

use uuid::Uuid;

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

/// Deterministic DiCoBa member UUID for a mesh agent (guarantor/borrower identity).
pub fn resolve_dicoba_member_id(agent_id: u16) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("fspdevs-dicoba-member-fa-{agent_id}").as_bytes(),
    )
}

/// Local runtime member id — honors `DICOBA_MEMBER_ID` when set for this process.
pub fn resolve_local_dicoba_member_id(agent_id: u16) -> Uuid {
    env::var("DICOBA_MEMBER_ID")
        .ok()
        .and_then(|raw| Uuid::parse_str(&raw).ok())
        .unwrap_or_else(|| resolve_dicoba_member_id(agent_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dicoba_member_id_is_stable_per_agent() {
        let fa44 = resolve_dicoba_member_id(44);
        assert_eq!(fa44.to_string(), "d3562ff0-edf3-5afe-a26c-08708bdd0480");
        assert_ne!(fa44, resolve_dicoba_member_id(45));
    }
}
