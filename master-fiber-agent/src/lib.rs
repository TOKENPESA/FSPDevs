pub mod api;
pub mod auth;
pub mod compliance_hub;
pub mod clearing;
pub mod clearinghouse;
pub mod config;
pub mod fnn_client;
pub mod graph;
pub mod graph_persistence;
pub mod handlers;
pub mod hub;
pub mod micro_routing;
pub mod middleware;
pub mod mfa_storage;
pub mod papss;
pub mod payment;
pub mod plugin_registry;
pub mod plugins;
pub mod policies;
pub mod routing;
pub mod state;
pub mod storage_error;
pub mod telemetry;
pub mod traits;
pub mod types;
pub mod ui_events;
pub mod workers;

pub use compliance_hub::SovereignComplianceHub;
pub use clearinghouse::EnterpriseClearinghouse;
pub use hub::CrossBorderSwapExecutor;
pub use micro_routing::MicropaymentEngine;
pub use papss::PapssIntegrationGateway;
pub use plugin_registry::PluginRegistry;
pub use state::AppState;

use auth::is_allowed_cors_origin;
use clearinghouse::mock_enterprise_clearinghouse;
use clearing::RegionalClearinghouseEngine;
use fnn_client::EnterpriseFnnClient;
use config::{
    apply_ingress_size_boundaries, bootstrap_asset_registry, hub_funding_lock_timeout_secs,
    load_ws_allowed_origins, mesh_mtls_enabled, parse_simulation_edge_nodes,
    resolve_agent_ws_token, resolve_mfa_api_token, setup_prometheus_metrics_provider,
    spawn_mtls_server, try_init_papss_gateway, verify_clearinghouse_environmental_safety,
    BROADCAST_CAP, COMPLIANCE_BROADCAST_CAP,
    DEFAULT_HUB_FUNDING_SHANNONS, TELEMETRY_QUEUE,
};
use graph::CompleteMeshGraph;
use graph_persistence::{GraphPersistenceManager, resolve_graph_snapshot_path};
use handlers::{
    calculate_transaction_route_handler, establish_regulatory_surveillance_feed,
    get_simulation_handler, health_handler, ingest_b2b_remittance_handler,
    ingest_float_crisis_handler, ingest_gossip_telemetry_handler, ingest_multi_asset_clearing_handler,
    ingest_telemetry_handler,
    issue_compliance_stream_ticket_handler, set_simulation_handler, ui_monitor_ws_handler,
    ws_handler,
};
use mfa_storage::MfaModuleStore;
use policies::registry::PluginHotReloader;
use api::plugin_routes::plugin_router;
use hub::MultiHubRegistry;
use mesh_core::compliance::ComplianceAuditEnvelope;
use mesh_core::RING_SIZE;
use payment::PaymentEngineState;
use state::{load_mesh_pubkey_registry, HubConfig, PeerRegistry, SharedGraph};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use workers::background::{
    background_processor_worker, start_liquidity_copilot_worker, ExpiringLockManager,
    LiquidityCopilot,
};

use axum::http::{header, Method};
use axum::middleware as axum_middleware;
use axum::routing::{get, post};
use axum::Router;
use middleware::{inject_security_headers_middleware, require_mfa_api_auth};
use tower_http::cors::{AllowOrigin, CorsLayer};

const DEFAULT_LIQUIDITY_COPILOT_INTERVAL_MS: u64 = 2000;

fn liquidity_copilot_interval_ms() -> u64 {
    env::var("MESH_LIQUIDITY_COPILOT_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&ms| ms >= 500)
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_INTERVAL_MS)
}

pub fn telemetry_packet_from_mesh_pulse(
    pulse: &mesh_core::types::MeshPulsePayload,
    minimum_required_shannons: u64,
) -> Option<mesh_core::telemetry::TelemetryPacket> {
    use mesh_core::telemetry::{
        BalanceDepletedPayload, TelemetryAlertSeverity, TelemetryEvent, TelemetryPacket,
    };
    use uuid::Uuid;

    if pulse.status != "ALERT_BALANCE_DEPLETED" {
        return None;
    }

    let agent_fnn_pubkey = pulse.fnn_pubkey_hex.clone()?;
    let short_channel_id = pulse
        .peer_connect_address
        .clone()
        .unwrap_or_else(|| format!("fa-{}-primary", pulse.agent_id));

    Some(TelemetryPacket {
        packet_id: Uuid::new_v4(),
        timestamp_ms: pulse.timestamp.saturating_mul(1000),
        severity: TelemetryAlertSeverity::Critical,
        event: TelemetryEvent::BalanceDepleted(BalanceDepletedPayload {
            agent_id: pulse.agent_id,
            short_channel_id,
            available_outbound_shannons: pulse.local_capacity_shannons,
            minimum_required_shannons,
            agent_fnn_pubkey,
        }),
    })
}

pub async fn process_inbound_telemetry(
    packet: mesh_core::telemetry::TelemetryPacket,
    clearinghouse: Arc<EnterpriseClearinghouse>,
) {
    match packet.event {
        mesh_core::telemetry::TelemetryEvent::BalanceDepleted(payload) => {
            // Spawn the clearing operation to keep the primary ingestion pipeline unblocked
            tokio::spawn(async move {
                if let Err(err) = clearinghouse.handle_balance_depletion(payload).await {
                    log::error!("❌ [CLEARINGHOUSE ERROR] Autonomous rebalancing failed: {err}");
                }
            });
        }
        _ => {
            // Handle standard metrics logging and network heartbeats normally
        }
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let api_token = resolve_mfa_api_token()?;
    let agent_ws_token = resolve_agent_ws_token()?;
    verify_clearinghouse_environmental_safety().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    let (tx, rx) = mpsc::channel(TELEMETRY_QUEUE);
    let (ui_broadcast, _) = broadcast::channel(BROADCAST_CAP);

    let (compliance_broadcast, _compliance_rx) =
        broadcast::channel::<ComplianceAuditEnvelope>(COMPLIANCE_BROADCAST_CAP);

    let peers: PeerRegistry = Arc::new(RwLock::new(HashMap::new()));
    let graph: SharedGraph = Arc::new(RwLock::new(CompleteMeshGraph::with_lattice(RING_SIZE)));
    let graph_persistence = Arc::new(GraphPersistenceManager::new(
        graph.clone(),
        resolve_graph_snapshot_path(),
    ));
    graph_persistence.try_hydrate_graph(&graph).await;
    graph_persistence.clone().spawn_snapshot_worker();
    println!(
        "🗺️ [MFA] Graph persistence online — snapshot every {}s at {}",
        graph_persistence.snapshot_interval_secs(),
        graph_persistence.storage_path().display()
    );
    let asset_registry = bootstrap_asset_registry().await;
    let papss_gateway = try_init_papss_gateway();
    if papss_gateway.is_some() {
        println!("🌍 [MFA] PAPSS integration gateway online");
    }

    let hub_rpc_url = env::var("HUB_RPC_URL")
        .or_else(|_| env::var("FNN_RPC_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:8227".to_string());
    let corporate_treasury_vault_id = env::var("MFA_CORPORATE_TREASURY_VAULT_ID")
        .unwrap_or_else(|_| "corporate-clearing-vault".to_string());
    let enterprise_clearinghouse = Arc::new(EnterpriseClearinghouse::new(
        Arc::new(EnterpriseFnnClient::new(hub_rpc_url.clone())),
        corporate_treasury_vault_id,
    ));
    println!("🏦 [MFA] Enterprise clearinghouse online (treasury refuel path armed)");

    let hub_funding_shannons = env::var("HUB_FUNDING_SHANNONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_HUB_FUNDING_SHANNONS);
    let plugin_registry = PluginRegistry::empty();
    let module_store = Arc::new(MfaModuleStore::open()?);
    let plugin_hot_reloader = Arc::new(PluginHotReloader::new(
        plugin_registry.clone(),
        module_store.clone(),
        hub_funding_shannons,
    ));
    plugin_hot_reloader.hydrate_from_storage().await?;
    let running_plugins = plugin_registry.plugin_names().await;
    println!(
        "🔌 [MFA] Plugin registry online: running=[{}], clearing={}",
        running_plugins.join(", "),
        plugin_registry
            .clearing_plugin_name()
            .await
            .unwrap_or("none"),
    );
    println!("📦 [MFA] Module store online — hot-swap API at /api/modules/*");

    let shared_state = Arc::new(AppState {
        tx_queue: tx,
        peers: peers.clone(),
        graph: graph.clone(),
        ui_broadcast: ui_broadcast.clone(),
        compliance_broadcast: compliance_broadcast.clone(),
        compliance_tickets: crate::auth::EphemeralTicketRegistry::new(
            crate::auth::EPHEMERAL_TICKET_TTL_SECS,
        ),
        alert_dedupe: RwLock::new(HashSet::new()),
        alert_order: RwLock::new(VecDeque::new()),
        active_funding_locks: RwLock::new(ExpiringLockManager::new(
            hub_funding_lock_timeout_secs(),
        )),
        hub_config: HubConfig {
            rpc_url: hub_rpc_url,
            funding_allocation_shannons: hub_funding_shannons,
        },
        agent_ws_token,
        api_token: api_token.clone(),
        agent_fnn_pubkeys: RwLock::new(HashMap::new()),
        mesh_pubkey_registry: load_mesh_pubkey_registry(),
        payment_waiters: Arc::new(RwLock::new(HashMap::new())),
        payment_engine: PaymentEngineState::new(),
        simulation_edge_nodes: AtomicU16::new(parse_simulation_edge_nodes()),
        ws_allowed_origins: load_ws_allowed_origins(),
        agent_liquidity_snap: RwLock::new(HashMap::new()),
        liquidity_copilot: RwLock::new(LiquidityCopilot::new()),
        multi_hub_registry: RwLock::new(MultiHubRegistry::new()),
        asset_registry,
        papss_gateway,
        enterprise_clearinghouse,
        regional_clearing: Arc::new(RegionalClearinghouseEngine::new()),
        edge_hardware_profiles: Arc::new(RwLock::new(HashMap::new())),
        plugin_registry,
        module_store,
        plugin_hot_reloader,
    });

    let _compliance_hub = shared_state.sovereign_compliance_hub();
    println!("🏛️ [MFA] Sovereign compliance hub online (broadcast bus ready)");

    // background_processor_worker appends each telemetry packet to mesh_topology_journal.wal
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

    let http_allowed_origins = load_ws_allowed_origins();
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-mfa-api-token"),
        ])
        .allow_origin(AllowOrigin::predicate({
            let allowed = http_allowed_origins.clone();
            move |origin: &axum::http::HeaderValue, _parts: &axum::http::request::Parts| {
                origin
                    .to_str()
                    .map(|origin_str| is_allowed_cors_origin(origin_str, &allowed))
                    .unwrap_or(false)
            }
        }));

    let (metrics_router, _prometheus_handle) = setup_prometheus_metrics_provider();

    let public_routes = Router::new()
        .route("/", get(health_handler))
        .route(
            "/simulation",
            get(get_simulation_handler),
        )
        .route("/ws/:agent_id", get(ws_handler));

    let protected_routes = Router::new()
        .route("/simulation", post(set_simulation_handler))
        .route("/telemetry", post(ingest_telemetry_handler))
        .route("/clearing/float-crisis", post(ingest_float_crisis_handler))
        .route("/clearing/b2b-remittance", post(ingest_b2b_remittance_handler))
        .route("/clearing/multi-asset", post(ingest_multi_asset_clearing_handler))
        .route(
            "/compliance/surveillance",
            get(establish_regulatory_surveillance_feed),
        )
        .route(
            "/compliance/ticket",
            post(issue_compliance_stream_ticket_handler),
        )
        .route(
            "/api/v1/compliance/stream",
            get(establish_regulatory_surveillance_feed),
        )
        .route(
            "/api/v1/compliance/ticket",
            post(issue_compliance_stream_ticket_handler),
        )
        .route("/gossip/channel", post(ingest_gossip_telemetry_handler))
        .route("/route", post(calculate_transaction_route_handler))
        .route("/ws/monitor", get(ui_monitor_ws_handler))
        .merge(plugin_router())
        .layer(axum_middleware::from_fn_with_state(
            shared_state.clone(),
            require_mfa_api_auth,
        ));

    let authed_metrics = metrics_router.layer(axum_middleware::from_fn_with_state(
        shared_state.clone(),
        require_mfa_api_auth,
    ));

    let app = apply_ingress_size_boundaries(
        public_routes
            .merge(protected_routes)
            .layer(cors)
            .with_state(shared_state)
            .merge(authed_metrics)
            .layer(axum_middleware::from_fn(inject_security_headers_middleware)),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], 1025));

    if mesh_mtls_enabled() {
        println!(
            "🔒 [MFA-1025] Mesh Network Engine operational with mTLS at https://{addr}"
        );
        spawn_mtls_server(app, addr).await?;
        futures_util::future::pending::<()>().await;
    } else {
        println!(
            "🚀 [MFA-1025] Mesh Network Engine operational at http://{addr}"
        );
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    }
    Ok(())
}

pub mod test_support {
    use super::*;
    use mesh_core::AssetRegistryHub;
    use mesh_core::MeshPubkeyRegistry;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::atomic::AtomicU16;

    /// Runs an async future in unit tests without nesting Tokio runtimes.
    pub fn block_on_test_async<F, T>(future: F) -> T
    where
        F: std::future::Future<Output = T> + Send,
        T: Send,
    {
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        tokio::runtime::Runtime::new()
                            .expect("test runtime")
                            .block_on(future)
                    })
                    .join()
                    .expect("test async worker")
            })
        } else {
            tokio::runtime::Runtime::new()
                .expect("test runtime")
                .block_on(future)
        }
    }

    /// Builds plugin store + hot reloader for tests (isolated temp DB).
    pub fn test_plugin_stack(critical_floor: u64) -> (PluginRegistry, Arc<MfaModuleStore>, Arc<PluginHotReloader>) {
        let path = std::env::temp_dir().join(format!(
            "mfa-test-state-{}.db",
            uuid::Uuid::new_v4()
        ));
        std::env::set_var("MFA_SUPERVISOR_DB_PATH", path.to_string_lossy().to_string());
        let store = Arc::new(MfaModuleStore::open().expect("open test module store"));
        let registry = PluginRegistry::empty();
        let reloader = Arc::new(PluginHotReloader::new(
            registry.clone(),
            store.clone(),
            critical_floor,
        ));
        block_on_test_async(async {
            reloader.hydrate_from_storage().await.expect("hydrate");
        });
        (registry, store, reloader)
    }

    /// Builds an `AppState` wired to the given compliance bus and asset registry (integration tests).
    pub fn app_state_with_registry(
        compliance_broadcast: broadcast::Sender<ComplianceAuditEnvelope>,
        asset_registry: AssetRegistryHub,
    ) -> Arc<AppState> {
        let (plugin_registry, module_store, plugin_hot_reloader) = test_plugin_stack(10_000_000);
        let (tx, _) = mpsc::channel(8);
        Arc::new(AppState {
            tx_queue: tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(CompleteMeshGraph::new())),
            ui_broadcast: broadcast::channel(4).0,
            compliance_broadcast,
            compliance_tickets: crate::auth::EphemeralTicketRegistry::new(
                crate::auth::EPHEMERAL_TICKET_TTL_SECS,
            ),
            alert_dedupe: RwLock::new(HashSet::new()),
            alert_order: RwLock::new(VecDeque::new()),
            active_funding_locks: RwLock::new(ExpiringLockManager::new(60)),
            hub_config: HubConfig {
                rpc_url: "http://127.0.0.1:8227".to_string(),
                funding_allocation_shannons: 10_000_000,
            },
            agent_ws_token: "test".to_string(),
            api_token: "test-api-token-123456".to_string(),
            agent_fnn_pubkeys: RwLock::new(HashMap::new()),
            mesh_pubkey_registry: MeshPubkeyRegistry::from_map(HashMap::new()),
            payment_waiters: Arc::new(RwLock::new(HashMap::new())),
            payment_engine: PaymentEngineState::new(),
            simulation_edge_nodes: AtomicU16::new(16),
            ws_allowed_origins: vec![],
            agent_liquidity_snap: RwLock::new(HashMap::new()),
            liquidity_copilot: RwLock::new(LiquidityCopilot::new()),
            multi_hub_registry: RwLock::new(MultiHubRegistry::new()),
            asset_registry,
            papss_gateway: None,
            enterprise_clearinghouse: mock_enterprise_clearinghouse(),
            regional_clearing: Arc::new(RegionalClearinghouseEngine::new()),
            edge_hardware_profiles: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry,
            module_store,
            plugin_hot_reloader,
        })
    }
}
