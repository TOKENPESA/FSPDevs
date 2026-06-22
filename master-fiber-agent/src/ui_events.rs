//! Fire-and-forget UI monitor broadcast with explicit logging on failure.

use std::sync::Arc;

use mesh_core::types::{MonitorEnvelope, MonitorEventData};
use tokio::sync::broadcast;

use crate::state::AppState;

/// Sends a JSON event string to dashboard monitor clients; logs if the channel is full or closed.
pub fn send_ui_event(sender: &broadcast::Sender<String>, event_json: String) {
    if let Err(err) = sender.send(event_json) {
        eprintln!("⚠️ [MFA UI] Broadcast send failed: {err}");
    }
}

/// Validates and broadcasts a structured event envelope out to all connected monitor dashboards
#[allow(dead_code)]
pub fn broadcast_monitor_event(state: Arc<AppState>, event: MonitorEventData) {
    let envelope = MonitorEnvelope::wrap(event);

    match serde_json::to_string(&envelope) {
        Ok(json_string) => {
            if let Err(e) = state.ui_broadcast.send(json_string) {
                eprintln!("⚠️ [BROADCAST LAG] Failed to deliver monitor event frame: {e}");
            }
        }
        Err(err) => {
            eprintln!("❌ [SCHEMA ERROR] Serialization failed for monitor event: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::types::MESH_MONITOR_SCHEMA_VERSION;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::atomic::AtomicU16;
    use tokio::sync::{mpsc, RwLock};

    fn mock_state() -> Arc<AppState> {
        let (tx, _) = mpsc::channel(4);
        let (ui_broadcast, _) = broadcast::channel(4);
        Arc::new(AppState {
            tx_queue: tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(crate::graph::CompleteMeshGraph::new())),
            ui_broadcast,
            alert_dedupe: RwLock::new(HashSet::new()),
            alert_order: RwLock::new(VecDeque::new()),
            active_funding_locks: RwLock::new(crate::workers::background::FundingLockManager::new(60)),
            hub_config: crate::state::HubConfig {
                rpc_url: "http://127.0.0.1:8227".to_string(),
                funding_allocation_shannons: 1_000_000,
            },
            multi_hub_registry: RwLock::new(crate::hub::MultiHubRegistry::new()),
            agent_ws_token: "test".to_string(),
            agent_fnn_pubkeys: RwLock::new(HashMap::new()),
            mesh_pubkey_registry: mesh_core::MeshPubkeyRegistry::from_map(HashMap::new()),
            payment_waiters: Arc::new(RwLock::new(HashMap::new())),
            payment_engine: crate::payment::PaymentEngineState::new(),
            simulation_edge_nodes: AtomicU16::new(16),
            ws_allowed_origins: vec![],
            agent_liquidity_snap: RwLock::new(HashMap::new()),
            liquidity_copilot: RwLock::new(crate::workers::background::LiquidityCopilot::new()),
        })
    }

    #[test]
    fn broadcast_monitor_event_emits_schema_versioned_json() {
        let state = mock_state();
        let mut rx = state.ui_broadcast.subscribe();

        broadcast_monitor_event(
            state,
            MonitorEventData::IntentSwapSuccess {
                swap_id: uuid::Uuid::nil(),
                amount: 42,
            },
        );

        let frame = rx.try_recv().expect("monitor frame");
        let value: serde_json::Value = serde_json::from_str(&frame).expect("json");
        assert_eq!(value["schema_version"], MESH_MONITOR_SCHEMA_VERSION);
        assert_eq!(value["event"], "INTENT_SWAP_SUCCESS");
    }
}
