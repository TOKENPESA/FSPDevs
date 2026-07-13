use crate::auth::{
    check_websocket_handshake_origin, validate_agent_ws_connection,
};
use crate::payment::{
    handle_hop_settlement_callback, HopSettlementResult, PaymentEngineState,
    SidecarHopSettlementReply,
};
use crate::state::{AppState, PeerRegistry, NEXT_CONN_ID};
use crate::types::{PaymentExecResult, SidecarPaymentReply};
use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use mesh_core::valid_agent_id;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::config::PEER_TX_CAP;

async fn try_handle_incoming_ws_text(
    source_agent_id: u16,
    incoming_text: &str,
    payment_waiters: &Arc<RwLock<HashMap<String, oneshot::Sender<PaymentExecResult>>>>,
    payment_engine: &PaymentEngineState,
    peers: &PeerRegistry,
    edge_hardware_profiles: &Arc<RwLock<HashMap<u16, String>>>,
) {
    if let Ok(json_msg) = serde_json::from_str::<serde_json::Value>(incoming_text) {
        if json_msg.get("type").and_then(|v| v.as_str()) == Some("sys_broadcast") {
            if json_msg.get("event").and_then(|v| v.as_str())
                == Some("FSP_HARDWARE_PROFILE_CHANGED")
            {
                let profile = json_msg
                    .get("new_profile")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                edge_hardware_profiles
                    .write()
                    .await
                    .insert(source_agent_id, profile.to_string());
                log::info!(
                    "⚙️ [MFA] FA-{source_agent_id} reported hardware profile '{profile}' — routing timeouts adjusted"
                );
            }
            return;
        }

        if json_msg.get("type").and_then(|v| v.as_str()) == Some("p2p_relay") {
            if let Some(data) = json_msg.get("data") {
                if let Some(target_agent_id) = data
                    .get("target_agent_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u16)
                {
                    let registry = peers.read().await;
                    if let Some((tx, _)) = registry.get(&target_agent_id) {
                        if tx
                            .send(AxumMessage::Text(incoming_text.to_string()))
                            .await
                            .is_err()
                        {
                            log::warn!(
                                "⚠️ [P2P RELAY] Failed to forward packet to FA-{target_agent_id}"
                            );
                        } else {
                            log::info!(
                                "📡 [P2P RELAY] Forwarded module packet to FA-{target_agent_id}"
                            );
                        }
                    } else {
                        log::warn!(
                            "⚠️ [P2P RELAY] Target FA-{target_agent_id} is not connected"
                        );
                    }
                    return;
                }
            }
        }

        if json_msg.get("response").and_then(|v| v.as_str()) == Some("MESH_HOP_SETTLEMENT") {
            if let Some(payload) = json_msg.get("payload") {
                if let Ok(result_dto) = serde_json::from_value::<HopSettlementResult>(payload.clone())
                {
                    if let Err(err) = handle_hop_settlement_callback(
                        result_dto,
                        payment_engine,
                        peers,
                    )
                    .await
                    {
                        eprintln!("⚠️ [MULTI-HOP] Settlement callback error: {err}");
                    }
                    return;
                }
            }
        }
    }

    if let Ok(reply) = serde_json::from_str::<SidecarPaymentReply>(incoming_text) {
        if reply.command == "PAYMENT_RESULT" {
            let success = reply.status.eq_ignore_ascii_case("SUCCESS");
            if let Some(sender) = payment_waiters.write().await.remove(&reply.payment_id) {
                let _ = sender.send(PaymentExecResult {
                    success,
                    payment_hash: reply.payment_hash,
                    fee_shannons: reply.fee_shannons,
                    error: reply.error,
                });
            }
        }
        return;
    }

    if let Ok(hop_reply) = serde_json::from_str::<SidecarHopSettlementReply>(incoming_text) {
        if hop_reply.command == "HOP_SETTLEMENT" {
            if let Err(err) = handle_hop_settlement_callback(
                hop_reply.result,
                payment_engine,
                peers,
            )
            .await
            {
                eprintln!("⚠️ [MULTI-HOP] Settlement callback error: {err}");
            }
        }
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<u16>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !valid_agent_id(agent_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let token_ok = validate_agent_ws_connection(
        &headers,
        query.get("token").map(String::as_str),
        agent_id,
        &state.agent_ws_token,
    );
    if !token_ok {
        eprintln!("⚠️ [SECURITY] Rejected agent WS for FA-{agent_id}: invalid or missing token");
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if let Err(status) = check_websocket_handshake_origin(&headers, &state.ws_allowed_origins) {
        return status.into_response();
    }

    let peers = state.peers.clone();
    let payment_waiters = state.payment_waiters.clone();
    let payment_engine = state.payment_engine.clone();
    let edge_hardware_profiles = state.edge_hardware_profiles.clone();
    ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            agent_id,
            peers,
            payment_waiters,
            payment_engine,
            edge_hardware_profiles,
        )
    })
}

async fn handle_socket(
    socket: WebSocket,
    agent_id: u16,
    peers: PeerRegistry,
    payment_waiters: Arc<RwLock<HashMap<String, oneshot::Sender<PaymentExecResult>>>>,
    payment_engine: PaymentEngineState,
    edge_hardware_profiles: Arc<RwLock<HashMap<u16, String>>>,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::channel::<AxumMessage>(PEER_TX_CAP);

    let current_conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
    {
        let mut registry = peers.write().await;

        // Evict any existing dead handler block before assigning new sockets
        if registry.contains_key(&agent_id) {
            log::warn!(
                "⚠️ [REGISTRY] Force-evicting lingering stale channel for agent ID: {agent_id}"
            );
            registry.remove(&agent_id);
        }

        registry.insert(agent_id, (tx, current_conn_id));
    }

    let mut send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if ws_tx.send(message).await.is_err() {
                break;
            }
        }
    });
    let mut recv_task = tokio::spawn({
        let peers_for_hop = peers.clone();
        let edge_profiles_for_sys = edge_hardware_profiles.clone();
        async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                if let AxumMessage::Text(text) = &msg {
                    try_handle_incoming_ws_text(
                        agent_id,
                        text,
                        &payment_waiters,
                        &payment_engine,
                        &peers_for_hop,
                        &edge_profiles_for_sys,
                    )
                    .await;
                }
                if matches!(msg, AxumMessage::Close(_)) {
                    break;
                }
            }
        }
    });
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    let mut write_registry = peers.write().await;
    if let Some((_, registered_id)) = write_registry.get(&agent_id) {
        if *registered_id == current_conn_id {
            write_registry.remove(&agent_id);
            println!(
                "[CLEANUP] Safely unregistered disconnected Agent Client ID: {}",
                agent_id
            );
        } else {
            println!(
                "[IGNORING CLEANUP] Outdated connection close event intercepted for Agent ID: {}",
                agent_id
            );
        }
    }
}
