pub mod module_routes;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::sync::Mutex as TokioMutex;

use crate::api::module_routes::{ModuleApiState, module_router};
use crate::module_host::SidecarHost;

pub async fn spawn_module_api_server(
    host: Arc<TokioMutex<SidecarHost>>,
) -> Result<(), String> {
    let port: u16 = std::env::var("FIBER_AGENT_API_PORT")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(19444);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let app = Router::new().merge(module_router(ModuleApiState { host }));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("module API bind failed: {err}"))?;
    log::info!("🌐 [FA API] Module management API listening on http://{addr}");
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            log::error!("FA module API server stopped: {err}");
        }
    });
    Ok(())
}
