use crate::ui_events::send_ui_event;
use crate::config::{mesh_sim_payments_enabled, PAYMENT_EXEC_TIMEOUT_SECS};
use crate::state::{AppState, PeerRegistry};
use crate::types::{PaymentExecResult, RouteResponse};
use axum::extract::ws::Message as AxumMessage;
use chrono::Utc;
use mesh_core::MeshError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, RwLock};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

// ================================================================================
// 1. MULTI-HOP WIRE TYPES & COMMAND DTOs
// ================================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiHopPaymentRequest {
    pub source_agent: u16,
    pub dest_agent: u16,
    pub amount_shannons: u64,
    pub path: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopInstruction {
    pub payment_id: Uuid,
    pub current_hop: u16,
    pub next_hop: Option<u16>,
    pub amount_to_forward: u64,
    pub fee_to_retain: u64,
    pub payment_hash: String,
    pub expiry_timelock: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HopSettlementResult {
    Success {
        payment_id: Uuid,
        hop: u16,
        preimage: String,
    },
    Failed {
        payment_id: Uuid,
        hop: u16,
        reason: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarHopSettlementReply {
    pub command: String,
    #[serde(flatten)]
    pub result: HopSettlementResult,
}

pub struct ActivePaymentTracker {
    pub path: Vec<u16>,
    pub current_step_idx: usize,
    pub amount_map: HashMap<u16, u64>,
    pub payment_hash: String,
    pub completion_sender: Option<oneshot::Sender<Result<String, MeshError>>>,
}

#[derive(Clone)]
pub struct PaymentEngineState {
    pub active_payments: Arc<RwLock<HashMap<Uuid, ActivePaymentTracker>>>,
}

impl Default for PaymentEngineState {
    fn default() -> Self {
        Self::new()
    }
}

impl PaymentEngineState {
    pub fn new() -> Self {
        Self {
            active_payments: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

fn calculate_backwards_hop_liquidity(
    path: &[u16],
    target_amount: u64,
    fee_base_shannons: u64,
    fee_proportional_millionths: u64,
) -> HashMap<u16, u64> {
    let mut amount_map = HashMap::new();
    let mut current_required = target_amount;

    for &node in path.iter().rev() {
        amount_map.insert(node, current_required);
        let hop_liquidity_premium =
            (current_required.saturating_mul(fee_proportional_millionths)) / 1_000_000;
        let total_fee_shannons = fee_base_shannons.saturating_add(hop_liquidity_premium);
        current_required = current_required.saturating_add(total_fee_shannons);
    }
    amount_map
}

pub async fn dispatch_multi_hop_payment(
    req: MultiHopPaymentRequest,
    engine_state: &PaymentEngineState,
    peers: &PeerRegistry,
) -> Result<String, MeshError> {
    if req.path.len() < 2 {
        return Err(MeshError::InvalidPayload(
            "Dijkstra path must contain at least 2 agents".to_string(),
        ));
    }

    let payment_id = Uuid::new_v4();
    let mock_hash = format!("hash-{payment_id}-fsp");

    let amount_map = calculate_backwards_hop_liquidity(&req.path, req.amount_shannons, 1000, 10);

    let (tx, rx) = oneshot::channel();
    let tracker = ActivePaymentTracker {
        path: req.path.clone(),
        current_step_idx: 0,
        amount_map,
        payment_hash: mock_hash,
        completion_sender: Some(tx),
    };

    engine_state
        .active_payments
        .write()
        .await
        .insert(payment_id, tracker);

    let source_node = req.path[0];
    match trigger_next_hop_execution(payment_id, source_node, engine_state, peers).await {
        Ok(()) => match timeout(
            Duration::from_secs(PAYMENT_EXEC_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                engine_state.active_payments.write().await.remove(&payment_id);
                Err(MeshError::NetworkError(
                    "Payment pipeline broken. Node disconnected mid-route.".to_string(),
                ))
            }
            Err(_) => {
                engine_state.active_payments.write().await.remove(&payment_id);
                Err(MeshError::NetworkError(format!(
                    "timed out after {PAYMENT_EXEC_TIMEOUT_SECS}s waiting for multi-hop settlement"
                )))
            }
        },
        Err(err) => {
            engine_state.active_payments.write().await.remove(&payment_id);
            Err(err)
        }
    }
}

pub async fn trigger_next_hop_execution(
    payment_id: Uuid,
    target_node: u16,
    engine_state: &PaymentEngineState,
    peers: &PeerRegistry,
) -> Result<(), MeshError> {
    let (instruction, next_exists) = {
        let active_map = engine_state.active_payments.read().await;
        let tracker = active_map.get(&payment_id).ok_or_else(|| {
            MeshError::InvalidPayload("Payment context dead".to_string())
        })?;

        let next_node = if tracker.current_step_idx + 1 < tracker.path.len() {
            Some(tracker.path[tracker.current_step_idx + 1])
        } else {
            None
        };

        let amount = *tracker.amount_map.get(&target_node).unwrap_or(&0);

        let instruction = HopInstruction {
            payment_id,
            current_hop: target_node,
            next_hop: next_node,
            amount_to_forward: amount,
            fee_to_retain: if next_node.is_some() { 1000 } else { 0 },
            payment_hash: tracker.payment_hash.clone(),
            expiry_timelock: Utc::now().timestamp() as u64 + 3600,
        };
        (instruction, next_node.is_some())
    };

    let _ = next_exists;
    let msg_string = serde_json::to_string(&serde_json::json!({
        "command": "MESH_FORWARD_HTLC",
        "payload": instruction
    }))
    .map_err(|e| MeshError::InvalidPayload(e.to_string()))?;

    let registry = peers.read().await;
    if let Some((ws_tx, _)) = registry.get(&target_node) {
        ws_tx
            .try_send(AxumMessage::Text(msg_string))
            .map_err(|_| {
                MeshError::NetworkError(format!(
                    "Failed to write to FA-{target_node} socket pipeline"
                ))
            })?;
        Ok(())
    } else {
        Err(MeshError::NetworkError(format!(
            "FA-{target_node} is offline. Cannot route multi-hop loop."
        )))
    }
}

pub async fn handle_hop_settlement_callback(
    result: HopSettlementResult,
    engine_state: &PaymentEngineState,
    peers: &PeerRegistry,
) -> Result<(), MeshError> {
    match result {
        HopSettlementResult::Success {
            payment_id,
            hop,
            preimage,
        } => {
            let next_hop = {
                let mut active_map = engine_state.active_payments.write().await;
                let Some(tracker) = active_map.get_mut(&payment_id) else {
                    return Ok(());
                };

                if tracker.path[tracker.current_step_idx] != hop {
                    return Err(MeshError::InvalidPayload(
                        "Out-of-order hop sequence callback captured".to_string(),
                    ));
                }

                tracker.current_step_idx += 1;

                if tracker.current_step_idx >= tracker.path.len() {
                    if let Some(sender) = tracker.completion_sender.take() {
                        let _ = sender.send(Ok(preimage));
                    }
                    active_map.remove(&payment_id);
                    None
                } else {
                    Some(tracker.path[tracker.current_step_idx])
                }
            };

            if let Some(next_node) = next_hop {
                trigger_next_hop_execution(payment_id, next_node, engine_state, peers).await?;
            }
        }
        HopSettlementResult::Failed {
            payment_id,
            hop,
            reason,
        } => {
            if let Some(mut tracker) = engine_state
                .active_payments
                .write()
                .await
                .remove(&payment_id)
            {
                if let Some(sender) = tracker.completion_sender.take() {
                    let _ = sender.send(Err(MeshError::InvalidPayload(format!(
                        "Multi-hop breakdown at FA-{hop}: {reason}"
                    ))));
                }
            }
        }
    }
    Ok(())
}

pub async fn dispatch_route_payment(
    state: &AppState,
    source: u16,
    destination: u16,
    amount_shannons: u64,
    path: &[u16],
) -> RouteResponse {
    if path.len() > 2 {
        return dispatch_multi_hop_route_payment(state, source, destination, amount_shannons, path)
            .await;
    }

    dispatch_single_hop_payment(state, source, destination, amount_shannons, path).await
}

async fn dispatch_multi_hop_route_payment(
    state: &AppState,
    source: u16,
    destination: u16,
    amount_shannons: u64,
    path: &[u16],
) -> RouteResponse {
    send_ui_event(
        &state.ui_broadcast,
        serde_json::json!({
            "event": "PAYMENT_STARTED",
            "source": source,
            "destination": destination,
            "amount_shannons": amount_shannons,
            "path": path,
            "mode": "MULTI_HOP",
        })
        .to_string(),
    );

    let req = MultiHopPaymentRequest {
        source_agent: source,
        dest_agent: destination,
        amount_shannons,
        path: path.to_vec(),
    };

    let payment_result = match dispatch_multi_hop_payment(req, &state.payment_engine, &state.peers).await
    {
        Ok(preimage) => PaymentExecResult {
            success: true,
            payment_hash: Some(preimage),
            fee_shannons: None,
            error: None,
        },
        Err(err) => PaymentExecResult {
            success: false,
            payment_hash: None,
            fee_shannons: None,
            error: Some(err.to_string()),
        },
    };

    finalize_route_payment_response(
        state,
        source,
        destination,
        amount_shannons,
        path,
        payment_result,
    )
    .await
}

async fn dispatch_single_hop_payment(
    state: &AppState,
    source: u16,
    destination: u16,
    amount_shannons: u64,
    path: &[u16],
) -> RouteResponse {
    let dest_pubkey = {
        let heartbeat = state.agent_fnn_pubkeys.read().await;
        state.mesh_pubkey_registry.resolve_for_payment(
            destination,
            &heartbeat,
            mesh_sim_payments_enabled(),
        )
    };

    let Some(dest_pubkey) = dest_pubkey else {
        return RouteResponse {
            status: "ROUTE_FOUND".to_string(),
            path: path.to_vec(),
            execution_latency_ms: 0,
            payment_status: Some("FAILED".to_string()),
            payment_hash: None,
            payment_error: Some(format!(
                "FA-{destination} has no payment pubkey — for live testnet add a secp256k1 key to mesh-pubkeys.json; for sim fleet ensure MESH_ALLOW_SIM_PAYMENTS is not false"
            )),
            payment_fee_shannons: None,
        };
    };

    let payment_id = format!(
        "pay-{source}-{destination}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let (tx, rx) = oneshot::channel();
    state
        .payment_waiters
        .write()
        .await
        .insert(payment_id.clone(), tx);

    let cmd = serde_json::json!({
        "command": "MESH_SEND_PAYMENT",
        "payment_id": payment_id,
        "destination_agent": destination,
        "target_fnn_pubkey": dest_pubkey,
        "amount_shannons": amount_shannons,
    });

    let delivered = {
        let registry = state.peers.read().await;
        if let Some((agent_tx, _)) = registry.get(&source) {
            agent_tx
                .try_send(AxumMessage::Text(cmd.to_string()))
                .is_ok()
        } else {
            false
        }
    };

    if !delivered {
        state.payment_waiters.write().await.remove(&payment_id);
        send_ui_event(
            &state.ui_broadcast,
            serde_json::json!({
                "event": "PAYMENT_FAILED",
                "source": source,
                "destination": destination,
                "amount_shannons": amount_shannons,
                "path": path,
                "reason": format!("FA-{source} sidecar not connected"),
            })
            .to_string(),
        );
        return RouteResponse {
            status: "ROUTE_FOUND".to_string(),
            path: path.to_vec(),
            execution_latency_ms: 0,
            payment_status: Some("SKIPPED_NO_SIDECAR".to_string()),
            payment_hash: None,
            payment_error: Some(format!(
                "FA-{source} sidecar not connected — start fiber-agent-daemon with AGENT_ID={source}"
            )),
            payment_fee_shannons: None,
        };
    }

    send_ui_event(
        &state.ui_broadcast,
        serde_json::json!({
            "event": "PAYMENT_STARTED",
            "source": source,
            "destination": destination,
            "amount_shannons": amount_shannons,
            "path": path,
            "payment_id": payment_id,
            "mode": "SINGLE_HOP",
        })
        .to_string(),
    );

    let payment_result = match timeout(
        Duration::from_secs(PAYMENT_EXEC_TIMEOUT_SECS),
        rx,
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            state.payment_waiters.write().await.remove(&payment_id);
            PaymentExecResult {
                success: false,
                payment_hash: None,
                fee_shannons: None,
                error: Some("payment waiter cancelled — agent disconnected".into()),
            }
        }
        Err(_) => {
            state.payment_waiters.write().await.remove(&payment_id);
            PaymentExecResult {
                success: false,
                payment_hash: None,
                fee_shannons: None,
                error: Some(format!(
                    "timed out after {PAYMENT_EXEC_TIMEOUT_SECS}s waiting for FA-{source} payment result"
                )),
            }
        }
    };

    finalize_route_payment_response(
        state,
        source,
        destination,
        amount_shannons,
        path,
        payment_result,
    )
    .await
}

async fn finalize_route_payment_response(
    state: &AppState,
    source: u16,
    destination: u16,
    amount_shannons: u64,
    path: &[u16],
    payment_result: PaymentExecResult,
) -> RouteResponse {
    let payment_status = if payment_result.success {
        "SUCCESS".to_string()
    } else if payment_result
        .error
        .as_deref()
        .is_some_and(|e| e.contains("timed out"))
    {
        "TIMEOUT".to_string()
    } else {
        "FAILED".to_string()
    };

    let ui_event = if payment_result.success {
        serde_json::json!({
            "event": "PAYMENT_EXECUTED",
            "source": source,
            "destination": destination,
            "amount_shannons": amount_shannons,
            "path": path,
            "payment_hash": payment_result.payment_hash,
            "fee_shannons": payment_result.fee_shannons,
        })
    } else {
        serde_json::json!({
            "event": "PAYMENT_FAILED",
            "source": source,
            "destination": destination,
            "amount_shannons": amount_shannons,
            "path": path,
            "reason": payment_result.error,
        })
    };
    send_ui_event(&state.ui_broadcast, ui_event.to_string());

    RouteResponse {
        status: "ROUTE_FOUND".to_string(),
        path: path.to_vec(),
        execution_latency_ms: 0,
        payment_status: Some(payment_status),
        payment_hash: payment_result.payment_hash,
        payment_error: payment_result.error,
        payment_fee_shannons: payment_result.fee_shannons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backwards_fee_scaling_logic() {
        let path = vec![1, 2, 3];
        let target_destination_liquidity = 10_000_000;

        let allocation_map = calculate_backwards_hop_liquidity(
            &path,
            target_destination_liquidity,
            1000,
            100,
        );

        assert_eq!(*allocation_map.get(&3).unwrap(), 10_000_000);
        assert!(*allocation_map.get(&2).unwrap() > 10_000_000);
        assert!(*allocation_map.get(&1).unwrap() > *allocation_map.get(&2).unwrap());
    }
}
