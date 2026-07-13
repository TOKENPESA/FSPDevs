use crate::auth::verify_telemetry_agent_binding;
use crate::ui_events::send_ui_event;
use crate::graph::FnnChannelUpdate;
use crate::hub::{trigger_hub_liquidity_provisioning, DEFAULT_HUB_ASSET};
use crate::state::AppState;
use crate::telemetry::{
    validate_telemetry, verify_telemetry_sequence, verify_telemetry_signature,
    verify_telemetry_timestamp,
};
use crate::types::MeshPulsePayload;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use mesh_core::is_live_fiber_pubkey;
use std::sync::Arc;

pub async fn ingest_telemetry_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MeshPulsePayload>,
) -> impl IntoResponse {
    if let Err(skew_error) = verify_telemetry_timestamp(&payload) {
        eprintln!(
            "⚠️ [STALE TELEMETRY] Blocked update from Node FA-{}: {}",
            payload.agent_id, skew_error
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "STALE_TELEMETRY",
                "reason": skew_error.to_string(),
            })),
        )
            .into_response();
    }

    if let Err(auth_error) = verify_telemetry_signature(&payload) {
        eprintln!(
            "⚠️ [UNAUTHORIZED TELEMETRY] Blocked update from Node FA-{}: {}",
            payload.agent_id, auth_error
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "status": "UNAUTHORIZED_TELEMETRY",
                "reason": auth_error,
            })),
        )
            .into_response();
    }

    if let Some(pubkey) = payload.public_key_hex.as_deref() {
        if let Err(binding_error) =
            verify_telemetry_agent_binding(payload.agent_id, pubkey, &state.mesh_pubkey_registry)
        {
            eprintln!(
                "⚠️ [UNAUTHORIZED TELEMETRY] Agent binding failed for FA-{}: {}",
                payload.agent_id, binding_error
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "status": "UNAUTHORIZED_TELEMETRY",
                    "reason": binding_error,
                })),
            )
                .into_response();
        }
    } else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "status": "UNAUTHORIZED_TELEMETRY",
                "reason": "public_key_hex required",
            })),
        )
            .into_response();
    }

    if let Err(replay_error) = verify_telemetry_sequence(&payload) {
        eprintln!(
            "⚠️ [REPLAY TELEMETRY] Blocked update from Node FA-{}: {}",
            payload.agent_id, replay_error
        );
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "status": "REPLAY_TELEMETRY",
                "reason": replay_error.to_string(),
            })),
        )
            .into_response();
    }

    if !validate_telemetry(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "INVALID_METRICS" })),
        )
            .into_response();
    }

    if payload.status == "ALERT_BALANCE_DEPLETED" {
        let agent_id = payload.agent_id;

        let target_pubkey = match payload.fnn_pubkey_hex.clone() {
            Some(key) if is_live_fiber_pubkey(&key) => key,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "status": "INVALID_METRICS",
                        "reason": "ALERT_BALANCE_DEPLETED requires fnn_pubkey_hex (Fiber node secp256k1 pubkey)"
                    })),
                )
                    .into_response();
            }
        };

        let mut locks = state.active_funding_locks.write().await;
        if !locks.try_acquire_lock(agent_id) {
            println!(
                "⏳ [HUB LIQUIDITY] Ignored duplicate depletion alert from FA-{agent_id}. Funding transaction already in flight."
            );
            return (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({
                    "status": "FUNDING_IN_FLIGHT"
                })),
            )
                .into_response();
        }

        drop(locks);

        let ui_engaged = serde_json::json!({
            "event": "LIQUIDITY_ENGAGED",
            "node": agent_id
        });
        send_ui_event(&state.ui_broadcast, ui_engaged.to_string());

        let state_clone = state.clone();
        tokio::spawn(async move {
            trigger_hub_liquidity_provisioning(
                agent_id,
                target_pubkey,
                state_clone,
                DEFAULT_HUB_ASSET,
            )
            .await;
        });

        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "LIQUIDITY_ENGAGED"
            })),
        )
            .into_response();
    }

    if payload.status == "ALERT_MFA_NODE_DROPPED" {
        println!(
            "🔧 [HEALING ENGINES] Authenticated fault alert from FA-{}",
            payload.agent_id
        );
    }

    match state.tx_queue.try_send(payload) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "PROCESSED" })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "QUEUE_FULL" })),
        )
            .into_response(),
    }
}

pub async fn ingest_gossip_telemetry_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FnnChannelUpdate>,
) -> impl IntoResponse {
    let mut graph = state.graph.write().await;

    match graph.ingest_channel_update(payload) {
        Ok(()) => {
            let version = graph.get_version();
            drop(graph);

            send_ui_event(
                &state.ui_broadcast,
                serde_json::json!({
                    "event": "TOPOLOGY_SYNC",
                    "version": version,
                })
                .to_string(),
            );

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "TOPOLOGY_SYNC",
                    "version": version,
                })),
            )
                .into_response()
        }
        Err(err) => {
            eprintln!("⚠️ [GOSSIP] Rejected channel update: {err}");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "GOSSIP_REJECTED",
                    "reason": err.to_string(),
                })),
            )
                .into_response()
        }
    }
}
