use mesh_core::RING_SIZE;
use std::env;

use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;

pub const PAYMENT_EXEC_TIMEOUT_SECS: u64 = 45;
pub const TELEMETRY_QUEUE: usize = 8192;
pub const BROADCAST_CAP: usize = 2048;
pub const PEER_TX_CAP: usize = 32;
pub const HEARTBEAT_UI_MIN_INTERVAL_MS: u64 = 250;
/// Max HTTP POST body size for telemetry and route intake (64 KiB).
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const DEDUPE_CAP: usize = 2048;
pub const DEFAULT_HUB_FUNDING_SHANNONS: u64 = 50_000_000_000;
pub const DEFAULT_AGENT_WS_TOKEN: &str = "tpxdevs-local-ws";
pub const DEFAULT_HUB_FUNDING_LOCK_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_LIQUIDITY_COPILOT_LOW_WATERMARK_SHANNONS: u64 = 5_000_000_000;
pub const DEFAULT_LIQUIDITY_DEPLETION_HORIZON_SECS: u64 = 120;
pub const DEFAULT_LIQUIDITY_COPILOT_COOLDOWN_SECS: u64 = 300;

pub fn hub_funding_lock_timeout_secs() -> u64 {
    env::var("HUB_FUNDING_LOCK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&secs| secs > 0)
        .unwrap_or(DEFAULT_HUB_FUNDING_LOCK_TIMEOUT_SECS)
}

pub fn mesh_liquidity_copilot_enabled() -> bool {
    env::var("MESH_LIQUIDITY_COPILOT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn liquidity_copilot_low_watermark_shannons() -> u64 {
    env::var("MESH_LIQUIDITY_LOW_WATERMARK_SHANNONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_LOW_WATERMARK_SHANNONS)
}

pub fn liquidity_copilot_depletion_horizon_secs() -> f64 {
    env::var("MESH_LIQUIDITY_DEPLETION_HORIZON_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_DEPLETION_HORIZON_SECS) as f64
}

pub fn liquidity_copilot_cooldown_secs() -> u64 {
    env::var("MESH_LIQUIDITY_COPILOT_COOLDOWN_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_COOLDOWN_SECS)
}

pub fn parse_simulation_edge_nodes() -> u16 {
    env::var("MESH_SIMULATION_EDGE_NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| (1..=RING_SIZE).contains(&n))
        .unwrap_or(RING_SIZE)
}

pub fn simulation_grid_dim(edge_nodes: u16) -> u16 {
    (edge_nodes as f64).sqrt().ceil() as u16
}

pub fn simulation_fleet_hint(edge_nodes: u16) -> String {
    if edge_nodes >= RING_SIZE {
        "fnn-testnet/spawn-mesh-fleet.ps1".to_string()
    } else {
        format!("fnn-testnet/spawn-mesh-fleet.ps1 -To {edge_nodes}")
    }
}

pub fn mesh_sim_payments_enabled() -> bool {
    env::var("MESH_ALLOW_SIM_PAYMENTS")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Extra WebSocket Origin values (exact match). Comma-separated via `MFA_WS_ALLOWED_ORIGINS`.
pub fn load_ws_allowed_origins() -> Vec<String> {
    env::var("MFA_WS_ALLOWED_ORIGINS")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|_| {
            vec![
                "http://127.0.0.1:8088".to_string(),
                "http://localhost:8088".to_string(),
                "http://[::1]:8088".to_string(),
            ]
        })
}

pub fn configure_intake_limits<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
}
