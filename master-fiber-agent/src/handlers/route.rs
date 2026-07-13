use mesh_core::types::L2Asset;
use serde_json::Value;
use crate::payment::dispatch_route_payment;
use crate::state::AppState;
use crate::telemetry::validate_route;
use crate::traits::{ClearanceVerdict, RoutingIntent};
use crate::types::{RouteRequestPayload, RouteResponse};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn calculate_transaction_route_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RouteRequestPayload>,
) -> impl IntoResponse {
    let max_bound = payload
        .active_network_limit
        .unwrap_or_else(|| state.simulation_edge_nodes.load(Ordering::Relaxed));
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    if payload.source == 0
        || payload.source > max_bound
        || payload.destination == 0
        || payload.destination > max_bound
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "OUT_OF_BOUNDS",
                "reason": format!(
                    "Source or destination exceeds current network limit of {max_bound}"
                ),
            })),
        )
            .into_response();
    }

    if !validate_route(&payload, max_bound) || payload.amount_shannons == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(RouteResponse {
                status: "INVALID_NODE_ID".to_string(),
                path: Vec::new(),
                execution_latency_ms: 0,
                payment_status: None,
                payment_hash: None,
                payment_error: None,
                payment_fee_shannons: None,
            }),
        )
            .into_response();
    }

    let execute = payload.execute.unwrap_or(true);
    let graph_read = state.graph.read().await;
    let latency = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            - start_time
    };

    let target_asset = payload
        .target_asset
        .clone()
        .unwrap_or(L2Asset::CkbNative);

    let route_result = graph_read.compute_asset_route(
        payload.source,
        payload.destination,
        payload.amount_shannons,
        target_asset.clone(),
        Some(max_bound),
        Some(&state.plugin_registry),
    );

    let (path, _route_cost) = match route_result {
        Some(result) => result,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(RouteResponse {
                    status: "MESH_UNREACHABLE".to_string(),
                    path: Vec::new(),
                    execution_latency_ms: latency(),
                    payment_status: None,
                    payment_hash: None,
                    payment_error: None,
                    payment_fee_shannons: None,
                }),
            )
                .into_response();
        }
    };
    drop(graph_read);

    let routing_intent = RoutingIntent {
        source: payload.source,
        destination: payload.destination,
        amount_atomic: payload.amount_shannons,
        target_asset,
        path: path.clone(),
        metadata: payload.route_metadata.clone().unwrap_or(Value::Null),
    };

    match state
        .plugin_registry
        .pre_route_clearance(&routing_intent)
        .await
    {
        Ok(ClearanceVerdict::Approved) => {}
        Ok(ClearanceVerdict::Rejected(reason)) => {
            return (
                StatusCode::FORBIDDEN,
                Json(RouteResponse {
                    status: "POLICY_REJECTED".to_string(),
                    path: Vec::new(),
                    execution_latency_ms: latency(),
                    payment_status: None,
                    payment_hash: None,
                    payment_error: Some(reason),
                    payment_fee_shannons: None,
                }),
            )
                .into_response();
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RouteResponse {
                    status: "POLICY_ERROR".to_string(),
                    path: Vec::new(),
                    execution_latency_ms: latency(),
                    payment_status: None,
                    payment_hash: None,
                    payment_error: Some(err.to_string()),
                    payment_fee_shannons: None,
                }),
            )
                .into_response();
        }
    }

    if execute {
        let mut payment_response = dispatch_route_payment(
            &state,
            payload.source,
            payload.destination,
            payload.amount_shannons,
            &path,
        )
        .await;
        payment_response.execution_latency_ms = latency();
        return (StatusCode::OK, Json(payment_response)).into_response();
    }

    (
        StatusCode::OK,
        Json(RouteResponse {
            status: "ROUTE_FOUND".to_string(),
            path,
            execution_latency_ms: latency(),
            payment_status: Some("SKIPPED".to_string()),
            payment_hash: None,
            payment_error: None,
            payment_fee_shannons: None,
        }),
    )
        .into_response()
}
