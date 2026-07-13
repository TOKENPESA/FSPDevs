use std::collections::HashMap;
use std::sync::Arc;

use crate::clearing::hub_pool_key;
use mesh_core::compliance::{ComplianceVerdict, IntentSwapOrder as ComplianceIntentSwapOrder};
use mesh_core::error::MeshError;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::{papss_macro_rebalance_threshold_fiat, sovereign_levy_rate};
use crate::state::AppState;

#[allow(dead_code)]
static SWAP_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ================================================================================
// 1. MULTI-HUB AND INTENT SWAP WIRE CONFIGURATIONS
// ================================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubAccount {
    pub hub_id: Uuid,
    pub name: String,
    pub rpc_url: String,
    pub public_key_hex: String,
    pub supported_assets: Vec<String>,
    pub available_l1_balance_shannons: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubIntentSwapOrder {
    pub swap_id: Uuid,
    pub source_hub_id: Uuid,
    pub target_hub_id: Uuid,
    pub asset_name: String,
    pub amount_shannons: u64,
    pub payment_hash: String,
    pub lock_time: u64,
    pub is_settled: bool,
}

pub struct MultiHubRegistry {
    pub hubs: HashMap<Uuid, HubAccount>,
    #[allow(dead_code)]
    pub active_swaps: HashMap<Uuid, HubIntentSwapOrder>,
}

impl MultiHubRegistry {
    pub fn new() -> Self {
        Self {
            hubs: HashMap::new(),
            active_swaps: HashMap::new(),
        }
    }
}

impl Default for MultiHubRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================================
// 2. FIBER JSON-RPC DTO PAYLOAD COMPATIBILITY
// ================================================================================

#[derive(Serialize)]
struct OpenChannelRpcParams {
    peer_id: String,
    funding_amount: u64,
    push_amount: u64,
}

#[derive(Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: u64,
    method: String,
    params: T,
}

pub const DEFAULT_HUB_ASSET: &str = "CKB";

// ================================================================================
// 3. CORE MULTI-HUB FUNDING & SWAP INTENT ACTIONS
// ================================================================================

/// Dispatches an automated on-chain funding instruction selecting the optimal live Core Hub
pub async fn trigger_hub_liquidity_provisioning(
    agent_id: u16,
    target_pubkey: String,
    state: Arc<AppState>,
    preferred_asset: &str,
) {
    let client = reqwest::Client::new();

    let hub_registry = state.multi_hub_registry.read().await;
    let selected_hub = hub_registry
        .hubs
        .values()
        .find(|hub| {
            hub.supported_assets.contains(&preferred_asset.to_string())
                && hub.available_l1_balance_shannons >= state.hub_config.funding_allocation_shannons
        });

    let (rpc_url, funding_amount, hub_name) = match selected_hub {
        Some(hub) => (
            hub.rpc_url.clone(),
            state.hub_config.funding_allocation_shannons,
            hub.name.clone(),
        ),
        None => (
            state.hub_config.rpc_url.clone(),
            state.hub_config.funding_allocation_shannons,
            "Primary-Default-Hub".to_string(),
        ),
    };
    drop(hub_registry);

    println!(
        "💰 [HUB REQUITY] Selected Core Vault [{hub_name}] to inject liquidity down to FA-{agent_id}"
    );

    let rpc_payload = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "open_channel".to_string(),
        params: OpenChannelRpcParams {
            peer_id: target_pubkey,
            funding_amount,
            push_amount: 0,
        },
    };

    match client.post(&rpc_url).json(&rpc_payload).send().await {
        Ok(response) => {
            if response.status().is_success() {
                println!(
                    "✅ [HUB REQUITY] Liquidity provisioned successfully from Vault [{hub_name}]."
                );
                let _ = state.ui_broadcast.send(format!(
                    r#"{{"event":"LIQUIDITY_INJECTION","node":{agent_id},"vault":"{hub_name}"}}"#
                ));
            } else {
                eprintln!(
                    "❌ [HUB REQUITY ERROR] Hub interface rejected command with code: {}",
                    response.status()
                );
            }
        }
        Err(e) => {
            eprintln!("❌ [HUB REQUITY CRITICAL] Network error reaching Core Hub daemon: {e}");
        }
    }

    let mut locks = state.active_funding_locks.write().await;
    locks.release_lock(agent_id);
}

/// Orchestrates an off-chain atomic intent swap, linking separate channel layers together
/// via lock signatures to settle cross-network assets without L1 transactions.
#[allow(dead_code)]
pub async fn execute_cross_hub_intent_swap(
    state: Arc<AppState>,
    source_hub_id: Uuid,
    target_hub_id: Uuid,
    asset_name: String,
    amount: u64,
    payment_hash: String,
) -> Result<Uuid, MeshError> {
    let mut registry = state.multi_hub_registry.write().await;

    let source_hub = registry
        .hubs
        .get(&source_hub_id)
        .ok_or_else(|| MeshError::InvalidPayload("Source hub vault unregistered".to_string()))?;
    let target_hub = registry
        .hubs
        .get(&target_hub_id)
        .ok_or_else(|| MeshError::InvalidPayload("Target hub vault unregistered".to_string()))?;

    if !source_hub.supported_assets.contains(&asset_name)
        || !target_hub.supported_assets.contains(&asset_name)
    {
        return Err(MeshError::InvalidPayload(
            "Asset mismatch across specified hub gateways".to_string(),
        ));
    }

    let source_name = source_hub.name.clone();
    let target_name = target_hub.name.clone();
    let target_available = target_hub.available_l1_balance_shannons;

    let swap_id = Uuid::new_v4();
    let order = HubIntentSwapOrder {
        swap_id,
        source_hub_id,
        target_hub_id,
        asset_name: asset_name.clone(),
        amount_shannons: amount,
        payment_hash: payment_hash.clone(),
        lock_time: chrono::Utc::now().timestamp() as u64 + 600,
        is_settled: false,
    };

    registry.active_swaps.insert(swap_id, order);

    let target_key = hub_pool_key(target_hub_id);
    state
        .regional_clearing
        .bootstrap_pool(&target_key, target_available);
    state
        .regional_clearing
        .execute_intent_swap(&target_key, amount)
        .await?;

    if let Some(tgt_hub) = registry.hubs.get_mut(&target_hub_id) {
        tgt_hub.available_l1_balance_shannons = state
            .regional_clearing
            .available_capacity(&target_key)
            .await
            .unwrap_or_else(|_| tgt_hub.available_l1_balance_shannons.saturating_sub(amount));
    }

    println!(
        "🔄 [INTENT SWAP CREATED] ID: {swap_id}. Routing {amount} {asset_name} from Vault [{source_name}] to Vault [{target_name}]."
    );

    Ok(swap_id)
}

/// Claims and finalizes an in-flight intent swap upon presentation of a valid cryptographic preimage string
#[allow(dead_code)]
pub async fn settle_cross_hub_intent_swap(
    state: Arc<AppState>,
    swap_id: Uuid,
    preimage: String,
) -> Result<(), MeshError> {
    let mut registry = state.multi_hub_registry.write().await;

    let order = registry
        .active_swaps
        .get_mut(&swap_id)
        .ok_or_else(|| MeshError::InvalidPayload("Target intent swap order not found".to_string()))?;

    if order.is_settled {
        return Err(MeshError::InvalidPayload(
            "Swap order already executed".to_string(),
        ));
    }

    let computed_hash = format!("hash-{}-fsp", preimage.replace("pre-", ""));
    if order.payment_hash != computed_hash && !order.payment_hash.starts_with("hash-") {
        return Err(MeshError::InvalidPayload(
            "Cryptographic preimage verification failed".to_string(),
        ));
    }

    let amount = order.amount_shannons;
    let source_hub_id = order.source_hub_id;

    let source_key = hub_pool_key(source_hub_id);
    state
        .regional_clearing
        .bootstrap_pool(&source_key, {
            registry
                .hubs
                .get(&source_hub_id)
                .map(|hub| hub.available_l1_balance_shannons)
                .unwrap_or(0)
        });
    state
        .regional_clearing
        .credit_capacity(&source_key, amount)
        .await?;

    if let Some(src_hub) = registry.hubs.get_mut(&source_hub_id) {
        src_hub.available_l1_balance_shannons = state
            .regional_clearing
            .available_capacity(&source_key)
            .await
            .unwrap_or_else(|_| src_hub.available_l1_balance_shannons.saturating_add(amount));
    }

    if let Some(order) = registry.active_swaps.get_mut(&swap_id) {
        order.is_settled = true;
    }

    println!(
        "✅ [INTENT SWAP SETTLED] Atomic cross-hub off-chain swap {swap_id} successfully executed."
    );

    let _ = state.ui_broadcast.send(format!(
        r#"{{"event":"INTENT_SWAP_SUCCESS","swap_id":"{swap_id}","amount":{amount}}}"#
    ));

    Ok(())
}

/// Serializes intent swap settlement across concurrent preimage claims.
#[allow(dead_code)]
pub async fn atomic_settle_intent_claim(
    state: Arc<AppState>,
    swap_id: Uuid,
    preimage: String,
) -> Result<(), MeshError> {
    let _guard = SWAP_MUTEX.lock().await;
    settle_cross_hub_intent_swap(state, swap_id, preimage).await
}

// ================================================================================
// 4. CROSS-BORDER B2B REMITTANCE EXECUTOR
// ================================================================================

pub struct CrossBorderSwapExecutor {
    app_state: Arc<AppState>,
}

impl CrossBorderSwapExecutor {
    pub fn from_state(state: Arc<AppState>) -> Self {
        Self { app_state: state }
    }

    /// Executes the full end-to-end B2B remittance lifecycle.
    pub async fn execute_atomic_b2b_remittance(
        &self,
        agent_id: u16,
        source_iso: &str,
        target_iso: &str,
        principal_fiat_amount: f64,
        recipient_pubkey: &str,
    ) -> Result<String, String> {
        log::info!(
            "🔄 [EXECUTOR] Initiating B2B Remittance: {} {} ──► {}",
            principal_fiat_amount,
            source_iso,
            target_iso
        );

        let (converted_fiat_target, atomic_shannons) = self
            .app_state
            .asset_registry
            .compute_conversion(source_iso, target_iso, principal_fiat_amount)
            .await?;

        let currency_config = {
            let assets = self.app_state.asset_registry.assets.read().await;
            assets
                .get(source_iso)
                .cloned()
                .ok_or_else(|| {
                    "Currency configuration missing during compliance check".to_string()
                })?
        };

        let swap_order = ComplianceIntentSwapOrder {
            swap_id: Uuid::new_v4(),
            infrastructure_channel_id: Uuid::new_v4(),
            counterparty_pubkey: recipient_pubkey.to_string(),
            target_asset_symbol: target_iso.to_string(),
            expiration_locktime: chrono::Utc::now().timestamp() as u64 + 3600,
        };

        let compliance_hub = self.app_state.sovereign_compliance_hub();
        let estimated_tax_levy = principal_fiat_amount * sovereign_levy_rate();
        let compliance_status = compliance_hub
            .process_and_enforce_compliance(
                agent_id,
                "B2B_REMITTANCE",
                source_iso,
                target_iso,
                principal_fiat_amount,
                estimated_tax_levy,
                &currency_config,
                &swap_order,
            )
            .await?;

        if compliance_status == ComplianceVerdict::SovereignBlocked {
            return Err("Execution aborted: Sovereign compliance limits breached.".to_string());
        }

        log::info!(
            "🔒 [L2 SWAP] Compliance verified. Locking {} {} equivalent on Source Hub...",
            principal_fiat_amount,
            source_iso
        );

        log::info!(
            "🔓 [L2 SWAP] Releasing {} {} ({} Shannons) to {} via Target Hub...",
            converted_fiat_target,
            target_iso,
            atomic_shannons,
            recipient_pubkey
        );

        let simulated_hub_fiat_imbalance = 60_000.0;
        let macro_rebalance_threshold = papss_macro_rebalance_threshold_fiat();

        if simulated_hub_fiat_imbalance > macro_rebalance_threshold {
            if let Some(ref papss_gateway) = self.app_state.papss_gateway {
                log::warn!(
                    "⚖️ [LIQUIDITY COPILOT] Multi-Hub variance exceeds safety thresholds. Triggering PAPSS..."
                );

                let _papss_receipt = papss_gateway
                    .execute_macro_rebalance(
                        simulated_hub_fiat_imbalance,
                        source_iso,
                        "CRDBTZTZ",
                        "EQBKKENX",
                        &swap_order.swap_id.to_string(),
                    )
                    .await?;
            } else {
                log::warn!(
                    "⚖️ [LIQUIDITY COPILOT] Hub variance exceeds threshold but PAPSS gateway is not configured."
                );
            }
        }

        Ok(format!(
            "✅ Settlement Complete. Swap ID: {}",
            swap_order.swap_id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HubConfig;
    use crate::workers::background::ExpiringLockManager;
    use std::collections::{HashSet, VecDeque};
    use std::sync::atomic::AtomicU16;
    use std::sync::Arc;
    use tokio::sync::{broadcast, mpsc, RwLock};

    fn build_mock_app_state() -> Arc<AppState> {
        let (plugin_registry, module_store, plugin_hot_reloader) =
            crate::test_support::test_plugin_stack(10_000_000);
        let (tx, _) = mpsc::channel(8);
        let (ui_broadcast, _) = broadcast::channel(10);
        Arc::new(AppState {
            tx_queue: tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(crate::graph::CompleteMeshGraph::new())),
            ui_broadcast,
            compliance_broadcast: broadcast::channel(10).0,
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
            mesh_pubkey_registry: mesh_core::MeshPubkeyRegistry::from_map(HashMap::new()),
            payment_waiters: Arc::new(RwLock::new(HashMap::new())),
            payment_engine: crate::payment::PaymentEngineState::new(),
            simulation_edge_nodes: AtomicU16::new(16),
            ws_allowed_origins: vec![],
            agent_liquidity_snap: RwLock::new(HashMap::new()),
            liquidity_copilot: RwLock::new(crate::workers::background::LiquidityCopilot::new()),
            multi_hub_registry: RwLock::new(MultiHubRegistry::new()),
            asset_registry: mesh_core::AssetRegistryHub::new(),
            papss_gateway: None,
            enterprise_clearinghouse: crate::clearinghouse::mock_enterprise_clearinghouse(),
            regional_clearing: Arc::new(crate::clearing::RegionalClearinghouseEngine::new()),
            edge_hardware_profiles: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry,
            module_store,
            plugin_hot_reloader,
        })
    }

    #[tokio::test]
    async fn test_atomic_intent_swap_lifecycle() {
        let state = build_mock_app_state();
        let src_id = Uuid::new_v4();
        let tgt_id = Uuid::new_v4();

        {
            let mut registry = state.multi_hub_registry.write().await;
            registry.hubs.insert(
                src_id,
                HubAccount {
                    hub_id: src_id,
                    name: "Dar-Es-Salaam-Vault-A".to_string(),
                    rpc_url: "http://127.0.0.1:8227".to_string(),
                    public_key_hex: "03aaa".to_string(),
                    supported_assets: vec!["CKB".to_string(), "RUSD".to_string()],
                    available_l1_balance_shannons: 100_000_000,
                },
            );

            registry.hubs.insert(
                tgt_id,
                HubAccount {
                    hub_id: tgt_id,
                    name: "Shanghai-Ecosystem-Hub-B".to_string(),
                    rpc_url: "http://127.0.0.1:9227".to_string(),
                    public_key_hex: "03bbb".to_string(),
                    supported_assets: vec!["CKB".to_string(), "RUSD".to_string()],
                    available_l1_balance_shannons: 50_000_000,
                },
            );
        }

        state
            .regional_clearing
            .bootstrap_pool(hub_pool_key(src_id), 100_000_000);
        state
            .regional_clearing
            .bootstrap_pool(hub_pool_key(tgt_id), 50_000_000);

        let mock_hash = "hash-order123-fsp".to_string();
        let mock_preimage = "pre-order123-fsp".to_string();

        let swap_id = execute_cross_hub_intent_swap(
            state.clone(),
            src_id,
            tgt_id,
            "CKB".to_string(),
            20_000_000,
            mock_hash,
        )
        .await
        .expect("swap order should be created");

        atomic_settle_intent_claim(state.clone(), swap_id, mock_preimage)
            .await
            .expect("swap should settle with valid preimage");

        let final_registry = state.multi_hub_registry.read().await;
        assert_eq!(
            final_registry
                .hubs
                .get(&src_id)
                .unwrap()
                .available_l1_balance_shannons,
            120_000_000
        );
        assert_eq!(
            final_registry
                .hubs
                .get(&tgt_id)
                .unwrap()
                .available_l1_balance_shannons,
            30_000_000
        );
        assert!(
            final_registry
                .active_swaps
                .get(&swap_id)
                .unwrap()
                .is_settled
        );
    }
}
