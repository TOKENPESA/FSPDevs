use crate::config::{simulation_fleet_hint, simulation_grid_dim};
use crate::state::AppState;
use axum::{extract::State, Json};
use mesh_core::RING_SIZE;
use std::env;
use std::sync::Arc;

pub async fn health_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let funding = state.hub_config.funding_allocation_shannons;
    let edge_nodes = state
        .simulation_edge_nodes
        .load(std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({
        "service": "master_fiber_agent",
        "mode": "mesh",
        "nodes": RING_SIZE,
        "simulation_edge_nodes": edge_nodes,
        "simulation_grid_dim": simulation_grid_dim(edge_nodes),
        "telemetry": "/telemetry",
        "route": "/route",
        "simulation": "/simulation",
        "websocket": "/ws/:agent_id",
        "monitor": "/ws/monitor",
        "dashboard": "http://localhost:8088/",
        "agent_ws_auth": "query param ?token= (env MFA_AGENT_WS_TOKEN)",
        "hub": {
            "rpc_url": state.hub_config.rpc_url,
            "funding_allocation_shannons": funding,
            "sidecar_balance_alerts": "off unless FIBER_AGENT_HUB_CHANNEL_FUNDING=true on each sidecar",
            "multi_node_hint": "Set HUB_PEER_ADDR_<AGENT_ID> on sidecar + remote pubkeys in mesh-pubkeys.json",
            "mesh_pubkey_registry": env::var("MESH_PUBKEY_REGISTRY_PATH").unwrap_or_else(|_| "unset".to_string()),
            "live_dashboard": "fnn-testnet/run-live-dashboard.ps1",
            "mesh_fleet": simulation_fleet_hint(edge_nodes),
            "mesh_fleet_daemon": "cargo run --bin mesh-fleet-daemon -p fiber_agent_sidecar"
        }
    }))
}
