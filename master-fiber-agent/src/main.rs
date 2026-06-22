mod auth;
mod ui_events;
mod config;
mod graph;
mod handlers;
mod hub;
mod payment;
mod state;
mod telemetry;
mod types;
mod workers;

use auth::is_allowed_ws_origin;
use config::{
    configure_intake_limits, hub_funding_lock_timeout_secs, load_ws_allowed_origins,
    parse_simulation_edge_nodes, BROADCAST_CAP, DEFAULT_AGENT_WS_TOKEN, DEFAULT_HUB_FUNDING_SHANNONS,
    TELEMETRY_QUEUE,
};
use graph::CompleteMeshGraph;
use handlers::{
    calculate_transaction_route_handler, get_simulation_handler, health_handler,
    ingest_gossip_telemetry_handler, ingest_telemetry_handler, set_simulation_handler,
    ui_monitor_ws_handler, ws_handler,
};
use mesh_core::RING_SIZE;
use payment::PaymentEngineState;
use hub::MultiHubRegistry;
use state::{load_mesh_pubkey_registry, AppState, HubConfig, PeerRegistry, SharedGraph};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use workers::background::{
    background_processor_worker, start_liquidity_copilot_worker, FundingLockManager,
    LiquidityCopilot,
};

use axum::http::{header, Method};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};

const DEFAULT_LIQUIDITY_COPILOT_INTERVAL_MS: u64 = 2000;

fn liquidity_copilot_interval_ms() -> u64 {
    env::var("MESH_LIQUIDITY_COPILOT_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&ms| ms >= 500)
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_INTERVAL_MS)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel(TELEMETRY_QUEUE);
    let (ui_broadcast, _) = broadcast::channel(BROADCAST_CAP);
    let peers: PeerRegistry = Arc::new(RwLock::new(HashMap::new()));
    let graph: SharedGraph = Arc::new(RwLock::new(CompleteMeshGraph::with_lattice(RING_SIZE)));

    let shared_state = Arc::new(AppState {
        tx_queue: tx,
        peers: peers.clone(),
        graph: graph.clone(),
        ui_broadcast: ui_broadcast.clone(),
        alert_dedupe: RwLock::new(HashSet::new()),
        alert_order: RwLock::new(VecDeque::new()),
        active_funding_locks: RwLock::new(FundingLockManager::new(
            hub_funding_lock_timeout_secs(),
        )),
        hub_config: HubConfig {
            rpc_url: env::var("HUB_RPC_URL")
                .or_else(|_| env::var("FNN_RPC_URL"))
                .unwrap_or_else(|_| "http://127.0.0.1:8227".to_string()),
            funding_allocation_shannons: env::var("HUB_FUNDING_SHANNONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_HUB_FUNDING_SHANNONS),
        },
        agent_ws_token: env::var("MFA_AGENT_WS_TOKEN")
            .unwrap_or_else(|_| DEFAULT_AGENT_WS_TOKEN.to_string()),
        agent_fnn_pubkeys: RwLock::new(HashMap::new()),
        mesh_pubkey_registry: load_mesh_pubkey_registry(),
        payment_waiters: Arc::new(RwLock::new(HashMap::new())),
        payment_engine: PaymentEngineState::new(),
        simulation_edge_nodes: AtomicU16::new(parse_simulation_edge_nodes()),
        ws_allowed_origins: load_ws_allowed_origins(),
        agent_liquidity_snap: RwLock::new(HashMap::new()),
        liquidity_copilot: RwLock::new(LiquidityCopilot::new()),
        multi_hub_registry: RwLock::new(MultiHubRegistry::new()),
    });

    tokio::spawn(background_processor_worker(
        rx,
        peers.clone(),
        graph.clone(),
        ui_broadcast.clone(),
        shared_state.clone(),
    ));

    let copilot_state = shared_state.clone();
    let copilot_interval_ms = liquidity_copilot_interval_ms();
    tokio::spawn(async move {
        start_liquidity_copilot_worker(copilot_state, copilot_interval_ms).await;
    });

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE])
        .allow_origin(AllowOrigin::predicate(
            |origin: &axum::http::HeaderValue, _parts: &axum::http::request::Parts| {
                origin
                    .to_str()
                    .map(is_allowed_ws_origin)
                    .unwrap_or(false)
            },
        ));

    let app = configure_intake_limits(
        Router::new()
            .route("/", get(health_handler))
            .route(
                "/simulation",
                get(get_simulation_handler).post(set_simulation_handler),
            )
            .route("/telemetry", post(ingest_telemetry_handler))
            .route("/gossip/channel", post(ingest_gossip_telemetry_handler))
            .route("/route", post(calculate_transaction_route_handler))
            .route("/ws/monitor", get(ui_monitor_ws_handler))
            .route("/ws/:agent_id", get(ws_handler))
            .layer(cors),
    )
    .with_state(shared_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 1025));
    println!(
        "🚀 [MFA-1025] Mesh Network Engine operational at http://{}",
        addr
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
