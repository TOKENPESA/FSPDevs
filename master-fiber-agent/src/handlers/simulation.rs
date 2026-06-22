use crate::config::{simulation_fleet_hint, simulation_grid_dim};
use crate::state::AppState;
use crate::types::{SimulationConfigPayload, SimulationConfigResponse};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use mesh_core::RING_SIZE;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn simulation_response(state: &AppState) -> SimulationConfigResponse {
    let edge_nodes = state.simulation_edge_nodes.load(Ordering::Relaxed);
    SimulationConfigResponse {
        edge_nodes,
        ring_max: RING_SIZE,
        grid_dim: simulation_grid_dim(edge_nodes),
        fleet_hint: simulation_fleet_hint(edge_nodes),
    }
}

pub async fn get_simulation_handler(
    State(state): State<Arc<AppState>>,
) -> Json<SimulationConfigResponse> {
    Json(simulation_response(&state))
}

pub async fn set_simulation_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SimulationConfigPayload>,
) -> impl IntoResponse {
    if payload.edge_nodes == 0 || payload.edge_nodes > RING_SIZE {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "OUT_OF_BOUNDS",
                "reason": format!("edge_nodes must be 1..={RING_SIZE}"),
            })),
        )
            .into_response();
    }

    state
        .simulation_edge_nodes
        .store(payload.edge_nodes, Ordering::Relaxed);
    println!(
        "🎛️ [MFA SIM] Edge node count set to {} (grid {}×{})",
        payload.edge_nodes,
        simulation_grid_dim(payload.edge_nodes),
        simulation_grid_dim(payload.edge_nodes),
    );

    (
        StatusCode::OK,
        Json(simulation_response(&state)),
    )
        .into_response()
}
