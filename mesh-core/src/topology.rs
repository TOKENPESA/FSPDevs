use crate::constants::RING_SIZE;
use crate::types::MeshPulsePayload;

/// Opposite (chord) peer on a 1-indexed ring — matches dashboard topology.
pub fn chord_peer(id: u16, total_nodes: u16) -> u16 {
    let n = total_nodes.max(1) as u32;
    let i = (id.saturating_sub(1)) as u32;
    ((i + n / 2) % n + 1) as u16
}

pub fn ring_peer(id: u16, total_nodes: u16) -> u16 {
    if id >= total_nodes { 1 } else { id + 1 }
}

pub fn skip_peer(id: u16, total_nodes: u16) -> u16 {
    if id >= total_nodes.saturating_sub(1) {
        id.saturating_add(2).saturating_sub(total_nodes).max(1)
    } else {
        id + 2
    }
}

pub fn mesh_neighbor_ids(agent_id: u16, total_nodes: u16) -> [u16; 3] {
    [
        ring_peer(agent_id, total_nodes),
        skip_peer(agent_id, total_nodes),
        chord_peer(agent_id, total_nodes),
    ]
}

pub fn neighbors_canonical(neighbors: &[u16]) -> String {
    let mut sorted = neighbors.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn mesh_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn telemetry_canonical_message(payload: &MeshPulsePayload) -> String {
    format!(
        "telemetry:{}:{}:{}:{}",
        payload.agent_id, payload.timestamp, payload.nonce, payload.local_capacity_shannons
    )
}

pub fn valid_agent_id(id: u16) -> bool {
    (1..=RING_SIZE).contains(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_peer_matches_dashboard_formula() {
        assert_eq!(chord_peer(1, 1024), 513);
        assert_eq!(chord_peer(514, 1024), 2);
    }

    #[test]
    fn neighbors_canonical_is_sorted() {
        assert_eq!(neighbors_canonical(&[45, 44, 46]), "44,45,46");
    }

    #[test]
    fn telemetry_canonical_includes_nonce_and_capacity() {
        let payload = MeshPulsePayload {
            agent_id: 1,
            timestamp: 1_700_000_000,
            nonce: 42,
            local_capacity_shannons: 9_000,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![2, 3],
            report_target: 1,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };
        assert_eq!(
            telemetry_canonical_message(&payload),
            "telemetry:1:1700000000:42:9000"
        );
    }
}
