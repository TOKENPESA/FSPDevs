//! HTTP routes for runtime module install/uninstall/toggle.

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
use tokio::sync::Mutex as TokioMutex;

use crate::module_catalog::catalog_entries;
use crate::module_host::SidecarHost;

#[derive(Clone)]
pub struct ModuleApiState {
    pub host: Arc<TokioMutex<SidecarHost>>,
}

pub fn module_router(state: ModuleApiState) -> Router {
    Router::new()
        .route("/api/modules/catalog", get(list_catalog_handler))
        .route("/api/modules/installed", get(list_installed_handler))
        .route("/api/modules/install", post(install_module_handler))
        .route("/api/modules/uninstall", post(uninstall_module_handler))
        .route("/api/modules/toggle", put(toggle_module_handler))
        .with_state(state)
}

pub async fn list_catalog_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "modules": catalog_entries(),
        })),
    )
}

pub async fn list_installed_handler(State(state): State<ModuleApiState>) -> impl IntoResponse {
    let host = state.host.lock().await;
    match host.db.get_installed_modules() {
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
pub struct InstallModuleRequest {
    pub module_name: String,
    #[serde(default)]
    pub config: Value,
}

pub async fn install_module_handler(
    State(state): State<ModuleApiState>,
    Json(body): Json<InstallModuleRequest>,
) -> impl IntoResponse {
    let reloader = {
        let host = state.host.lock().await;
        match host.hot_reloader() {
            Some(reloader) => reloader,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "hot reloader not initialized" })),
                )
                    .into_response();
            }
        }
    };
    match reloader
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
pub struct UninstallModuleRequest {
    pub module_name: String,
}

pub async fn uninstall_module_handler(
    State(state): State<ModuleApiState>,
    Json(body): Json<UninstallModuleRequest>,
) -> impl IntoResponse {
    let reloader = {
        let host = state.host.lock().await;
        match host.hot_reloader() {
            Some(reloader) => reloader,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "hot reloader not initialized" })),
                )
                    .into_response();
            }
        }
    };
    match reloader.uninstall(&body.module_name).await {
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
pub struct ToggleModuleRequest {
    pub module_name: String,
    pub is_active: bool,
}

pub async fn toggle_module_handler(
    State(state): State<ModuleApiState>,
    Json(body): Json<ToggleModuleRequest>,
) -> impl IntoResponse {
    let reloader = {
        let host = state.host.lock().await;
        match host.hot_reloader() {
            Some(reloader) => reloader,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "hot reloader not initialized" })),
                )
                    .into_response();
            }
        }
    };
    match reloader.toggle(&body.module_name, body.is_active).await {
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
