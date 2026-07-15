use crate::graph::CompleteMeshGraph;
use crate::mfa_storage::MfaModuleStore;
use crate::plugin_registry::PluginRegistry;
use crate::policies::registry::PluginHotReloader;
use crate::papss::PapssIntegrationGateway;
use crate::payment::PaymentEngineState;
use crate::types::{MeshPulsePayload, PaymentExecResult};
use crate::workers::background::{ExpiringLockManager, LiquidityCopilot};
use axum::extract::ws::Message as AxumMessage;
use mesh_core::{merge_registry_json, AssetRegistryHub, ComplianceAuditEnvelope, MeshPubkeyRegistry};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::sync::atomic::{AtomicU16, AtomicU64};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

pub static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

pub type PeerRegistry = Arc<RwLock<HashMap<u16, (mpsc::Sender<AxumMessage>, u64)>>>;
pub type SharedGraph = Arc<RwLock<CompleteMeshGraph>>;

/// Core Hub FNN JSON-RPC settings for automated channel funding.
pub struct HubConfig {
    /// Points to the JSON-RPC management port of this live node.
    pub rpc_url: String,
    /// On-chain capacity chunk per funding injection (shannons).
    pub funding_allocation_shannons: u64,
}

pub struct AppState {
    pub tx_queue: mpsc::Sender<MeshPulsePayload>,
    pub peers: PeerRegistry,
    pub graph: SharedGraph,
    pub ui_broadcast: broadcast::Sender<String>,
    pub compliance_broadcast: broadcast::Sender<ComplianceAuditEnvelope>,
    /// Single-use tickets for short-lived compliance stream / connection handoff.
    pub compliance_tickets: Arc<crate::auth::EphemeralTicketRegistry>,
    pub alert_dedupe: RwLock<HashSet<(u16, u16)>>,
    pub alert_order: RwLock<VecDeque<(u16, u16)>>,
    /// Nodes currently undergoing on-chain capacity injection (time-bounded locks).
    pub active_funding_locks: RwLock<ExpiringLockManager>,
    pub hub_config: HubConfig,
    /// Multi-hub storage pipeline registry (vault accounts and in-flight intent swaps).
    pub multi_hub_registry: RwLock<crate::hub::MultiHubRegistry>,
    /// Bearer / query token for mutating HTTP routes and compliance SSE.
    pub api_token: String,
    pub agent_ws_token: String,
    /// Latest Fiber node pubkeys reported by sidecar heartbeats.
    pub agent_fnn_pubkeys: RwLock<HashMap<u16, String>>,
    /// Latest Fiber P2P connect addresses advertised in heartbeats (`peer_connect_address`).
    pub agent_peer_addresses: RwLock<HashMap<u16, String>>,
    pub mesh_pubkey_registry: MeshPubkeyRegistry,
    pub payment_waiters: Arc<RwLock<HashMap<String, oneshot::Sender<PaymentExecResult>>>>,
    pub payment_engine: PaymentEngineState,
    /// Dashboard / routing cap: only FA 1..=N participate in the active simulation view.
    pub simulation_edge_nodes: AtomicU16,
    /// Exact-match WebSocket Origin allowlist (see `MFA_WS_ALLOWED_ORIGINS`).
    pub ws_allowed_origins: Vec<String>,
    /// Latest outbound shannons per agent (from heartbeats) for the liquidity copilot.
    pub agent_liquidity_snap: RwLock<HashMap<u16, u64>>,
    pub liquidity_copilot: RwLock<LiquidityCopilot>,
    pub asset_registry: AssetRegistryHub,
    pub papss_gateway: Option<PapssIntegrationGateway>,
    pub enterprise_clearinghouse: Arc<crate::clearinghouse::EnterpriseClearinghouse>,
    /// Per-hub liquidity pools for concurrent intent-swap reservation.
    pub regional_clearing: Arc<crate::clearing::RegionalClearinghouseEngine>,
    /// Latest hardware power profile reported by edge sidecars (for routing timeout tuning).
    pub edge_hardware_profiles: Arc<RwLock<HashMap<u16, String>>>,
    /// Policy plugins — business/regulatory hooks (never lock the graph).
    pub plugin_registry: PluginRegistry,
    /// SQLite app-store persistence for UI-managed plugin installs.
    pub module_store: Arc<MfaModuleStore>,
    /// Runtime hot-swap coordinator for plugins.
    pub plugin_hot_reloader: Arc<PluginHotReloader>,
}

/// Loads mesh pubkey registry from env, preserving MFA startup log lines.
pub fn load_mesh_pubkey_registry() -> MeshPubkeyRegistry {
    let mut map = HashMap::new();
    let path = env::var("MESH_PUBKEY_REGISTRY_PATH").ok();
    if let Some(path) = path {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            merge_registry_json(&mut map, &raw);
            println!("📒 [MFA] Loaded {} mesh pubkey(s) from {path}", map.len());
        } else {
            eprintln!("⚠️ [MFA] MESH_PUBKEY_REGISTRY_PATH not readable: {path}");
        }
    }
    if let Ok(raw) = env::var("MESH_PUBKEY_REGISTRY") {
        merge_registry_json(&mut map, &raw);
    }
    MeshPubkeyRegistry::from_map(map)
}

impl AppState {
    pub fn fiat_to_shannons(&self, source_iso: &str, fiat_amount: f64) -> u64 {
        crate::config::fiat_to_shannons(source_iso, fiat_amount)
    }
}
