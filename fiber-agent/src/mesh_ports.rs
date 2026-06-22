use std::path::{Path, PathBuf};

use mesh_core::{FNN_P2P_BASE, FNN_RPC_BASE, RING_SIZE};

pub fn fnn_rpc_port(agent_id: u16) -> u16 {
    FNN_RPC_BASE.saturating_add(agent_id)
}

pub fn fnn_p2p_port(agent_id: u16) -> u16 {
    FNN_P2P_BASE.saturating_add(agent_id)
}

pub fn fnn_rpc_url(agent_id: u16) -> String {
    format!("http://127.0.0.1:{}", fnn_rpc_port(agent_id))
}

pub fn fnn_p2p_multiaddr(agent_id: u16) -> String {
    format!("/ip4/127.0.0.1/tcp/{}", fnn_p2p_port(agent_id))
}

pub fn default_fnn_nodes_root() -> PathBuf {
    PathBuf::from("fnn-testnet").join("nodes")
}

pub fn fnn_node_data_dir(agent_id: u16, root: Option<&Path>) -> PathBuf {
    let base = root
        .map(PathBuf::from)
        .unwrap_or_else(default_fnn_nodes_root);
    base.join(format!("fa-{agent_id:04}"))
}

pub fn resolve_fnn_rpc_url(agent_id: u16) -> String {
    if let Ok(url) = std::env::var("FNN_RPC_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }
    if mesh_fnn_auto_ports_enabled() {
        return fnn_rpc_url(agent_id);
    }
    "http://127.0.0.1:8227".to_string()
}

pub fn mesh_fnn_auto_ports_enabled() -> bool {
    std::env::var("MESH_FNN_AUTO_PORTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn parse_fleet_range() -> Result<(u16, u16), String> {
    let from: u16 = std::env::var("MESH_FLEET_FROM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let to: u16 = std::env::var("MESH_FLEET_TO")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(RING_SIZE);
    if !(1..=RING_SIZE).contains(&from) || !(1..=RING_SIZE).contains(&to) || from > to {
        return Err(format!(
            "MESH_FLEET_FROM/TO must be 1..={RING_SIZE} with FROM <= TO (got {from}..={to})"
        ));
    }
    Ok((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_unique_per_agent() {
        assert_eq!(fnn_rpc_port(1), 18_001);
        assert_eq!(fnn_p2p_port(44), 28_044);
        assert_eq!(fnn_rpc_port(1024), 19_024);
    }

    #[test]
    fn data_dir_uses_padded_id() {
        let dir = fnn_node_data_dir(44, None);
        assert!(dir.to_string_lossy().contains("fa-0044"));
    }
}
