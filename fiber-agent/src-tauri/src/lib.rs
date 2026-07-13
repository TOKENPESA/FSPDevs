mod commands;

use std::env;
use std::sync::Arc;

use fiber_agent::fnn_client::{FiberNodeRpc, LiveFnnClient, SimulatedFnnClient};
use fiber_agent::mesh_ports::resolve_fnn_rpc_url;
use fiber_agent::module_registry::{boot_sidecar_host, SidecarBootContext};
use fiber_agent::parse_agent_id;
use fiber_agent::resolve_local_dicoba_member_id;
use fiber_agent::spawn_sidecar_mfa_control_ws;
use fiber_agent::storage::AgentDb;
use fiber_agent::MfaControlBus;
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

fn resolve_fnn_backend_arc(agent_id: u16) -> Arc<dyn FiberNodeRpc + Send + Sync> {
    let mode = env::var("FNN_MODE").unwrap_or_else(|_| "simulate".to_string());
    if mode.eq_ignore_ascii_case("testnet") || mode.eq_ignore_ascii_case("live") {
        let rpc_url = env::var("FNN_RPC_URL").unwrap_or_else(|_| resolve_fnn_rpc_url(agent_id));
        Arc::new(LiveFnnClient::new(rpc_url))
    } else {
        Arc::new(SimulatedFnnClient::new(agent_id))
    }
}

async fn initialize_sidecar_host(agent_id: u16) -> Result<fiber_agent::SidecarHost, String> {
    let fnn_client = resolve_fnn_backend_arc(agent_id);
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
    tauri::Builder::default()
        .setup(|app| {
            if env::var("FNN_MODE").is_err() {
                env::set_var("FNN_MODE", "simulate");
            }
            let agent_id = parse_agent_id().map_err(|err| -> Box<dyn std::error::Error> {
                err.into()
            })?;

            let mut sidecar_host = tauri::async_runtime::block_on(initialize_sidecar_host(agent_id))
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;

            let mfa_agent_id = sidecar_host.agent_id;
            let mfa_fnn = sidecar_host.fnn_client.clone();
            let mfa_db = sidecar_host.db.clone();
            let peer_outbound_rx = sidecar_host.take_outbound_receiver();
            let (mfa_bus, sys_broadcast_rx) = MfaControlBus::channel();
            tauri::async_runtime::spawn(async move {
                spawn_sidecar_mfa_control_ws(
                    mfa_agent_id,
                    mfa_fnn,
                    mfa_db,
                    peer_outbound_rx,
                    Some(sys_broadcast_rx),
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
                let _ = window.set_title(&format!("Fiber Sidecar - FA-{agent_id}"));
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
