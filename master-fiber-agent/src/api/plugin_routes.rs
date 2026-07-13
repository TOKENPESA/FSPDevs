//! HTTP routes for runtime MFA plugin install/uninstall/toggle.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::policies::catalog::catalog_entries;
use crate::state::AppState;

pub fn plugin_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/modules/catalog", get(list_catalog_handler))
        .route("/api/modules/installed", get(list_installed_handler))
        .route("/api/modules/install", post(install_module_handler))
        .route("/api/modules/uninstall", post(uninstall_module_handler))
        .route("/api/modules/toggle", put(toggle_module_handler))
}

async fn list_catalog_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "modules": catalog_entries() })),
    )
}

async fn list_installed_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.module_store.get_installed_modules() {
        Ok(rows) => (
            StatusCode::OK,
            Json(json!({
                "installed": rows.iter().map(|row| json!({
                    "id": row.id,
                    "module_name": row.module_name,
                    "is_active": row.is_active,
                    "config": serde_json::from_str::<Value>(&row.config_json).unwrap_or_else(|_| json!({})),
                })).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": reason })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct InstallModuleRequest {
    module_name: String,
    #[serde(default)]
    config: Value,
}

async fn install_module_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InstallModuleRequest>,
) -> impl IntoResponse {
    match state
        .plugin_hot_reloader
        .install_and_mount(&body.module_name, body.config)
        .await
    {
        Ok(record) => (
            StatusCode::OK,
            Json(json!({
                "status": "INSTALLED",
                "module_name": record.module_name,
                "is_active": record.is_active,
            })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "INSTALL_FAILED", "reason": reason })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UninstallModuleRequest {
    module_name: String,
}

async fn uninstall_module_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UninstallModuleRequest>,
) -> impl IntoResponse {
    match state.plugin_hot_reloader.uninstall(&body.module_name).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "status": "UNINSTALLED", "module_name": body.module_name })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "UNINSTALL_FAILED", "reason": reason })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct ToggleModuleRequest {
    module_name: String,
    is_active: bool,
}

async fn toggle_module_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ToggleModuleRequest>,
) -> impl IntoResponse {
    match state
        .plugin_hot_reloader
        .toggle(&body.module_name, body.is_active)
        .await
    {
        Ok(record) => (
            StatusCode::OK,
            Json(json!({
                "status": "TOGGLED",
                "module_name": record.module_name,
                "is_active": record.is_active,
            })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "TOGGLE_FAILED", "reason": reason })),
        )
            .into_response(),
    }
}
