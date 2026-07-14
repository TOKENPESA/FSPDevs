use crate::config::{
    simulation_fleet_hint, simulation_grid_dim, telco_clearing_api_url, telco_clearing_mock_when_unset,
};
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
    let peers = state.peers.read().await;
    let mut connected_agent_ids: Vec<u16> = peers.keys().copied().collect();
    connected_agent_ids.sort_unstable();
    let connected_agents = connected_agent_ids.len();
    drop(peers);

    let api_url = telco_clearing_api_url();
    let mock_active = telco_clearing_mock_when_unset();
    let regional_clearing_ready = !api_url.is_empty() || mock_active;
    let corporate_vault = env::var("MFA_CORPORATE_TREASURY_VAULT_ID")
        .unwrap_or_else(|_| "corporate-clearing-vault".to_string());

    let assets = state.asset_registry.assets.read().await;
    let mut corridors: Vec<String> = assets.keys().cloned().collect();
    corridors.sort_unstable();
    drop(assets);

    let running_plugins = state.plugin_registry.plugin_names().await;

    Json(serde_json::json!({
        "service": "master_fiber_agent",
        "mode": "mesh",
        "nodes": RING_SIZE,
        "simulation_edge_nodes": edge_nodes,
        "connected_agents": connected_agents,
        "connected_agent_ids": connected_agent_ids,
        "simulation_grid_dim": simulation_grid_dim(edge_nodes),
        "telemetry": "/telemetry",
        "route": "/route",
        "clearing": {
            "regional_float_crisis": "/clearing/float-crisis",
            "enterprise_balance_depletion": "TelemetryPacket → EnterpriseClearinghouse FNN refuel",
            "corporate_treasury_vault": corporate_vault,
            "regional_env_ready": regional_clearing_ready,
            "regional_mock_active": mock_active && api_url.is_empty(),
            "telco_api_env": "MFA_TELCO_CLEARING_API_URL",
            "telco_mock_env": "MFA_TELCO_CLEARING_MOCK",
            "topology_journal": "mesh_topology_journal.wal",
            "float_crisis": "/clearing/float-crisis",
            "b2b_remittance": "/clearing/b2b-remittance",
        },
        "asset_registry": {
            "hub": "AssetRegistryHub",
            "corridors": corridors,
        },
        "running_plugins": running_plugins,
        "auth": {
            "compliance_tickets": "EphemeralTicketRegistry",
            "ticket_ttl_secs": 30,
            "ws_origin_policy": "loopback + MFA_WS_ALLOWED_ORIGINS",
        },
        "compliance_stream": "/api/v1/compliance/stream",
        "compliance_ticket": "/compliance/ticket",
        "compliance_ticket_v1": "/api/v1/compliance/ticket",
        "simulation": "/simulation",
        "websocket": "/ws/:agent_id",
        "register": "POST /api/register (public, rate-limited; issues FA-N + agent_secret)",
        "monitor": "/ws/monitor",
        "dashboard": "http://localhost:8088/",
        "agent_ws_auth": "HMAC headers X-MFA-Agent-Auth + X-Agent-ID + X-MFA-Timestamp, or legacy ?token= (per-agent secret from /api/register when registered)",
        "api_auth": "Authorization: Bearer, X-MFA-API-Token, or ?token= (env MFA_API_TOKEN)",
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
