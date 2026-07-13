use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use mesh_core::types::FloatExhaustionTelemetry;
use serde::Deserialize;

use crate::clearing::MultiAssetCrossClearingIntent;
use crate::hub::CrossBorderSwapExecutor;
use crate::state::AppState;

pub async fn ingest_float_crisis_handler(
    State(state): State<Arc<AppState>>,
    Json(telemetry): Json<FloatExhaustionTelemetry>,
) -> impl IntoResponse {
    match state
        .plugin_registry
        .handle_float_crisis(state.clone(), telemetry)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "CLEARING_COMPLETE" })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "status": "CLEARING_ABORTED",
                "reason": reason,
            })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct B2bRemittanceRequest {
    pub agent_id: u16,
    pub source_iso: String,
    pub target_iso: String,
    pub principal_fiat_amount: f64,
    pub recipient_pubkey: String,
}

pub async fn ingest_b2b_remittance_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<B2bRemittanceRequest>,
) -> impl IntoResponse {
    let executor = CrossBorderSwapExecutor::from_state(state);

    match executor
        .execute_atomic_b2b_remittance(
            body.agent_id,
            &body.source_iso,
            &body.target_iso,
            body.principal_fiat_amount,
            &body.recipient_pubkey,
        )
        .await
    {
        Ok(message) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "SETTLED",
                "message": message,
            })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "status": "REMITTANCE_ABORTED",
                "reason": reason,
            })),
        )
            .into_response(),
    }
}

pub async fn ingest_multi_asset_clearing_handler(
    State(state): State<Arc<AppState>>,
    Json(intent): Json<MultiAssetCrossClearingIntent>,
) -> impl IntoResponse {
    match state
        .plugin_registry
        .handle_multi_asset_cross_clearing(state.clone(), intent)
        .await
    {
        Ok(legs) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "MULTI_ASSET_SETTLED",
                "legs": legs.iter().map(|leg| serde_json::json!({
                    "asset": leg.asset.ledger_label(),
                    "amount": leg.amount,
                    "path": leg.path,
                    "swap_id": leg.swap_id,
                })).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "status": "MULTI_ASSET_ABORTED",
                "reason": reason,
            })),
        )
            .into_response(),
    }
}
