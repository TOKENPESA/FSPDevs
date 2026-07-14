pub mod auth;
pub mod module_routes;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::sync::Mutex as TokioMutex;

use crate::api::auth::resolve_fiber_agent_api_token;
use crate::api::module_routes::{ModuleApiState, module_router};
use crate::module_host::SidecarHost;

/// Module API bind. Set `FIBER_AGENT_BIND_ADDR=0.0.0.0:19444` on VPS hosts.
fn resolve_module_api_bind_addr() -> SocketAddr {
    if let Ok(raw) = std::env::var("FIBER_AGENT_BIND_ADDR") {
        match raw.parse::<SocketAddr>() {
            Ok(addr) => return addr,
            Err(err) => log::warn!("FIBER_AGENT_BIND_ADDR invalid ({raw}): {err}"),
        }
    }
    let port: u16 = std::env::var("FIBER_AGENT_API_PORT")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(19444);
    SocketAddr::from(([127, 0, 0, 1], port))
}

pub async fn spawn_module_api_server(
    host: Arc<TokioMutex<SidecarHost>>,
) -> Result<(), String> {
    let api_token = resolve_fiber_agent_api_token()?;
    let addr = resolve_module_api_bind_addr();
    let app: Router = module_router(ModuleApiState { host }, Arc::new(api_token));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("module API bind failed: {err}"))?;
    log::info!(
        "🌐 [FA API] Module management API listening on http://{addr} (Bearer auth required)"
    );
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            log::error!("FA module API server stopped: {err}");
        }
    });
    Ok(())
}
