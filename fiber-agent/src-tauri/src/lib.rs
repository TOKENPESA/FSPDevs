mod commands;
mod fnn_address;

use std::env;
use std::sync::Arc;

use fiber_agent::fnn_client::FiberNodeRpc;
use fiber_agent::mesh_ports::resolve_fnn_rpc_url;
use fiber_agent::module_registry::{boot_sidecar_host, SidecarBootContext};
use fiber_agent::resolve_fnn_backend;
use fiber_agent::resolve_local_dicoba_member_id;
use fiber_agent::resolve_runtime_identity;
use fiber_agent::spawn_sidecar_mfa_control_ws;
use fiber_agent::storage::AgentDb;
use fiber_agent::{MfaControlBus, SidecarConfig};
use tauri::Manager;
use tokio::sync::Mutex as TokioMutex;
use commands::{
    calculate_invoice_preview, dispatch_to_module, execute_cash_in_transaction,
    execute_dico_contribution, fetch_installed_modules, fetch_module_catalog,
    generate_oob_fallback_uri, get_sidecar_stats, install_sidecar_module,
    process_oob_fallback, resolve_dicoba_member_id_for_agent, route_sidecar_command,
    toggle_hardware_profile, toggle_sidecar_module, uninstall_sidecar_module,
    trigger_manual_fiat_rebalance, HardwareProfileState,
};
use fnn_address::get_fnn_address;

/// Prefer a live local FNN when reachable; otherwise fall back to the simulated engine
/// so DiCoBa / modules keep working without `fnn-testnet` running.
async fn resolve_fnn_backend_arc(agent_id: u16) -> Arc<dyn FiberNodeRpc + Send + Sync> {
    let rpc_url = env::var("FNN_RPC_URL").unwrap_or_else(|_| resolve_fnn_rpc_url(agent_id));
    Arc::from(resolve_fnn_backend(agent_id, &rpc_url).await)
}

async fn initialize_sidecar_host(agent_id: u16) -> Result<fiber_agent::SidecarHost, String> {
    let fnn_client = resolve_fnn_backend_arc(agent_id).await;
    let db = Arc::new(AgentDb::open(agent_id)?);
    let member_id = resolve_local_dicoba_member_id(agent_id);
    let boot_ctx = SidecarBootContext::load(agent_id, fnn_client, db, member_id)?;
    if let Some(mfa_host) = boot_ctx.profile.mfa_host.clone() {
        env::set_var("MFA_HOST", mfa_host);
    }
    boot_sidecar_host(boot_ctx).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Lock desktop/mobile sidecars to the TLS MFA control plane (Android cleartext policy).
    fiber_agent::mfa_ws_auth::apply_secure_mfa_env_defaults();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            if env::var("FNN_MODE").is_err() {
                env::set_var("FNN_MODE", "testnet");
            }
            // Desktop default: obtain an MFA-issued FA-N + HMAC secret unless the operator
            // explicitly disables registration and supplies AGENT_ID + MFA_AGENT_WS_TOKEN.
            if env::var("MFA_AUTO_REGISTER").is_err() && env::var("MFA_AGENT_WS_TOKEN").is_err() {
                env::set_var("MFA_AUTO_REGISTER", "true");
            }

            let mut sidecar_config = SidecarConfig::from_env();
            let identity = tauri::async_runtime::block_on(resolve_runtime_identity(&mut sidecar_config))
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
            let agent_id = identity.agent_id;
            env::set_var("AGENT_ID", agent_id.to_string());
            env::set_var("MFA_AGENT_WS_TOKEN", &identity.agent_secret);

            let mut sidecar_host = tauri::async_runtime::block_on(initialize_sidecar_host(agent_id))
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;

            let mfa_agent_id = sidecar_host.agent_id;
            let mfa_fnn = sidecar_host.fnn_client.clone();
            let mfa_db = sidecar_host.db.clone();
            let mfa_ws_secret = identity.agent_secret.clone();
            let peer_outbound_rx = sidecar_host.take_outbound_receiver();
            let (mfa_bus, sys_broadcast_rx) = MfaControlBus::channel();
            tauri::async_runtime::spawn(async move {
                spawn_sidecar_mfa_control_ws(
                    mfa_agent_id,
                    mfa_fnn,
                    mfa_db,
                    peer_outbound_rx,
                    Some(sys_broadcast_rx),
                    Some(mfa_ws_secret),
                );
            });

            let host_arc = Arc::new(TokioMutex::new(sidecar_host));
            let host_for_api = host_arc.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = fiber_agent::spawn_module_api_server(host_for_api).await {
                    log::error!("FA module API failed to start: {err}");
                }
            });

            app.manage(host_arc);
            app.manage(Arc::new(HardwareProfileState::from_env()));
            app.manage(Arc::new(mfa_bus));

            let window_label = format!("main-fa-{agent_id}");
            if let Some(window) = app
                .get_webview_window(&window_label)
                .or_else(|| app.get_webview_window("main"))
            {
                let _ = window.set_title(&format!(
                    "Fiber Sidecar - {}",
                    identity.agent_id_label
                ));
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dispatch_to_module,
            route_sidecar_command,
            execute_cash_in_transaction,
            trigger_manual_fiat_rebalance,
            calculate_invoice_preview,
            toggle_hardware_profile,
            execute_dico_contribution,
            get_sidecar_stats,
            get_fnn_address,
            generate_oob_fallback_uri,
            process_oob_fallback,
            resolve_dicoba_member_id_for_agent,
            fetch_module_catalog,
            fetch_installed_modules,
            install_sidecar_module,
            uninstall_sidecar_module,
            toggle_sidecar_module,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fiber Agent desktop");
}
