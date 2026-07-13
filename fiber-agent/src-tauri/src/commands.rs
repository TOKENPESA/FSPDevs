use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use fiber_agent::clearing_client::{
    format_mfa_service_name, mfa_control_ws_url, probe_mfa_health, resolve_mfa_host,
};
use fiber_agent::mesh_ports::{fnn_p2p_multiaddr, resolve_fnn_rpc_url};
use fiber_agent::mesh::{
    agent_fnn_pubkey, mesh_neighbor_ids, resolve_dicoba_member_id, resolve_local_dicoba_member_id,
    RING_SIZE,
};
use fiber_agent::module_catalog::{catalog_entries, is_allowed_method, is_allowed_oob_peer_method};
use fiber_agent::module_host::SidecarHost;
use fiber_agent::power::{AdaptivePowerController, PowerProfile};
use fiber_agent::MfaControlBus;
use mesh_core::jungukuu_types::{JunguKuuVault, MicroContributionReceipt};
use mesh_core::types::{EdgeTransaction, FeeCalculationBreakdown};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex as TokioMutex;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatsSnapshot {
    pub agent_id: u16,
    pub fnn_mode: String,
    pub hardware_profile: String,
    pub power_profile: String,
    pub node_pubkey: String,
    pub fnn_backend: String,
    pub fnn_rpc_url: String,
    pub fnn_p2p_endpoint: String,
    pub fnn_connection_status: String,
    pub fnn_total_liquidity_shannons: u64,
    pub mfa_host: String,
    pub mfa_name: String,
    pub mfa_ws_url: String,
    pub mfa_connection_status: String,
    pub mfa_reachable: bool,
    pub mfa_control_connected: bool,
    pub mounted_modules: Vec<String>,
    pub sidecar_profile: String,
    pub profile_source: String,
    pub configured_modules: Vec<String>,
    pub mesh_channels_total: usize,
    pub mesh_channels_active: usize,
    pub total_local_balance_shannons: u64,
    pub total_remote_balance_shannons: u64,
    pub dicoba_contributions: u64,
    pub dicoba_vaults_total: u64,
    pub edge_pending: u64,
    pub edge_settled: u64,
    pub edge_failed: u64,
    pub fiat_edge_transactions: u64,
    pub queued_telemetry: u64,
    pub cached_channels: u64,
    pub mesh_peer_agent_id: u16,
    pub mesh_peer_pubkey: String,
    pub dicoba_member_id: String,
    pub mesh_peer_dicoba_member_id: String,
    pub fiat_conversion_rate: f64,
    pub critical_fiat_floor: f64,
    pub collected_at_unix: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionPayload {
    pub vault_config: JunguKuuVault,
    pub amount_fiat: f64,
    pub shannons_conversion_rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareProfile {
    SimulatedKiosk,
    LiveEdgeNode,
}

impl HardwareProfile {
    fn label(self) -> &'static str {
        match self {
            Self::SimulatedKiosk => "simulated_kiosk",
            Self::LiveEdgeNode => "live_edge_node",
        }
    }

    fn power_profile_name(self) -> &'static str {
        match self {
            Self::SimulatedKiosk => "BatterySaver",
            Self::LiveEdgeNode => "AggressiveRealTime",
        }
    }

    fn from_power_profile(name: &str) -> Option<Self> {
        match name {
            "AggressiveRealTime" => Some(Self::LiveEdgeNode),
            "BatterySaver" => Some(Self::SimulatedKiosk),
            _ => None,
        }
    }

    fn from_env() -> Self {
        match std::env::var("FNN_MODE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "simulate" | "sim" => Self::SimulatedKiosk,
            _ => Self::LiveEdgeNode,
        }
    }

    fn toggle(self) -> Self {
        match self {
            Self::SimulatedKiosk => Self::LiveEdgeNode,
            Self::LiveEdgeNode => Self::SimulatedKiosk,
        }
    }
}

pub struct HardwareProfileState(Mutex<(HardwareProfile, AdaptivePowerController)>);

impl HardwareProfileState {
    pub fn from_env() -> Self {
        Self(Mutex::new((
            HardwareProfile::from_env(),
            AdaptivePowerController::new(),
        )))
    }
}

async fn route_module_command(
    target_module: String,
    method: String,
    payload: serde_json::Value,
    host: &TokioMutex<SidecarHost>,
) -> Result<serde_json::Value, String> {
    validate_route_identifier(&target_module, "target_module")?;
    validate_route_identifier(&method, "method")?;
    validate_module_route(&target_module, &method)?;

    let host_guard = host.lock().await;
    if !host_guard.is_module_mounted(&target_module).await {
        return Err(format!(
            "Module '{target_module}' is not mounted on this sidecar (profile: {}).",
            host_guard.profile().preset_label()
        ));
    }

    host_guard
        .route_command(&target_module, &method, payload)
        .await
}

fn validate_route_identifier(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 64 {
        return Err(format!("{field} must be between 1 and 64 characters"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(format!("{field} contains invalid characters"));
    }
    Ok(())
}

fn validate_module_route(target_module: &str, method: &str) -> Result<(), String> {
    if !is_allowed_method(target_module, method) {
        if fiber_agent::module_catalog::allowed_methods(target_module).is_none() {
            return Err(format!("Module '{target_module}' is not registered on this sidecar."));
        }
        return Err(format!(
            "Method '{method}' is not allowed for module '{target_module}'."
        ));
    }
    Ok(())
}

fn agent_registered_on_mfa(health: &serde_json::Value, agent_id: u16) -> bool {
    health
        .get("connected_agent_ids")
        .and_then(|value| value.as_array())
        .map(|ids| {
            ids.iter().any(|id| {
                id.as_u64()
                    .map(|raw| raw as u16 == agent_id)
                    .or_else(|| id.as_i64().map(|raw| raw as u16 == agent_id))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[tauri::command]
pub async fn get_sidecar_stats(
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
    profile: State<'_, Arc<HardwareProfileState>>,
) -> Result<SidecarStatsSnapshot, String> {
    let host = host.lock().await;
    let hardware_profile = profile
        .0
        .lock()
        .map_err(|_| "hardware profile lock poisoned".to_string())?
        .0;

    let fnn_mode = std::env::var("FNN_MODE").unwrap_or_else(|_| "simulate".to_string());
    let is_simulate = fnn_mode.eq_ignore_ascii_case("simulate") || fnn_mode.eq_ignore_ascii_case("sim");
    let fnn_rpc_url = resolve_fnn_rpc_url(host.agent_id);
    let fnn_p2p_endpoint = fnn_p2p_multiaddr(host.agent_id);
    let fnn_backend = if is_simulate {
        "simulated".to_string()
    } else {
        "live".to_string()
    };
    let node_pubkey = host
        .fnn_client
        .node_pubkey()
        .await
        .unwrap_or_else(|_| "unavailable".to_string());
    let fnn_connection_status = if is_simulate {
        "simulated".to_string()
    } else if node_pubkey != "unavailable" {
        "online".to_string()
    } else {
        "offline".to_string()
    };
    let channels = host
        .fnn_client
        .list_channels()
        .await
        .unwrap_or_default();
    let total_local_balance_shannons: u64 = channels
        .iter()
        .map(|channel| channel.local_balance_shannons)
        .sum();
    let total_remote_balance_shannons: u64 = channels
        .iter()
        .map(|channel| channel.remote_balance_shannons)
        .sum();
    let mesh_channels_active = channels.iter().filter(|channel| channel.is_active).count();
    let fnn_total_liquidity_shannons = total_local_balance_shannons
        .saturating_add(total_remote_balance_shannons);
    let ledger = host.db.dashboard_ledger_counts()?;
    let mfa_host = resolve_mfa_host();
    let mfa_ws_url = mfa_control_ws_url(host.agent_id, Some(&mfa_host));
    let (mfa_connection_status, mfa_name, mfa_reachable, mfa_control_connected) =
        match probe_mfa_health(Some(&mfa_host)).await {
            Ok(health) => {
                let name = health
                    .get("service")
                    .and_then(|value| value.as_str())
                    .map(format_mfa_service_name)
                    .unwrap_or_else(|| "Master Fiber Agent".to_string());
                let registered = agent_registered_on_mfa(&health, host.agent_id);
                let status = if registered {
                    "registered"
                } else {
                    "reachable"
                };
                (status.to_string(), name, true, registered)
            }
            Err(_) => (
                "unreachable".to_string(),
                "Master Fiber Agent".to_string(),
                false,
                false,
            ),
        };
    let mesh_peer_agent_id = mesh_neighbor_ids(host.agent_id, RING_SIZE)
        .into_iter()
        .next()
        .unwrap_or(0);
    let mesh_peer_pubkey = if mesh_peer_agent_id > 0 {
        agent_fnn_pubkey(mesh_peer_agent_id)
    } else {
        String::new()
    };
    let dicoba_member_id = resolve_local_dicoba_member_id(host.agent_id).to_string();
    let mesh_peer_dicoba_member_id = if mesh_peer_agent_id > 0 {
        resolve_dicoba_member_id(mesh_peer_agent_id).to_string()
    } else {
        String::new()
    };
    let fiat_conversion_rate = std::env::var("MFA_FIAT_SHANNONS_RATE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(38.0);
    const CRITICAL_FIAT_FLOOR: f64 = 50_000.0;

    Ok(SidecarStatsSnapshot {
        agent_id: host.agent_id,
        fnn_mode,
        hardware_profile: hardware_profile.label().to_string(),
        power_profile: hardware_profile.power_profile_name().to_string(),
        node_pubkey,
        fnn_backend,
        fnn_rpc_url,
        fnn_p2p_endpoint,
        fnn_connection_status,
        fnn_total_liquidity_shannons,
        mfa_host,
        mfa_name,
        mfa_ws_url,
        mfa_connection_status,
        mfa_reachable,
        mfa_control_connected,
        mounted_modules: host.registered_module_names().await,
        sidecar_profile: host.profile().preset_label().to_string(),
        profile_source: host.profile().source.clone(),
        configured_modules: host
            .profile()
            .enabled_module_ids()
            .into_iter()
            .map(str::to_string)
            .collect(),
        mesh_channels_total: channels.len(),
        mesh_channels_active,
        total_local_balance_shannons,
        total_remote_balance_shannons,
        dicoba_contributions: ledger.dicoba_contributions,
        dicoba_vaults_total: ledger.dicoba_vaults_total,
        edge_pending: ledger.edge_pending,
        edge_settled: ledger.edge_settled,
        edge_failed: ledger.edge_failed,
        fiat_edge_transactions: ledger.fiat_edge_transactions,
        queued_telemetry: ledger.queued_telemetry,
        cached_channels: ledger.cached_channels,
        mesh_peer_agent_id,
        mesh_peer_pubkey,
        dicoba_member_id,
        mesh_peer_dicoba_member_id,
        fiat_conversion_rate,
        critical_fiat_floor: CRITICAL_FIAT_FLOOR,
        collected_at_unix: match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i64,
            Err(err) => {
                log::warn!(
                    "System clock rewind detected while collecting sidecar stats: {err}"
                );
                0
            }
        },
    })
}

#[tauri::command]
pub fn resolve_dicoba_member_id_for_agent(agent_id: u16) -> Result<String, String> {
    if !(1..=RING_SIZE).contains(&agent_id) {
        return Err(format!("agent_id must be 1..={RING_SIZE}, got {agent_id}"));
    }
    Ok(resolve_dicoba_member_id(agent_id).to_string())
}

#[tauri::command]
pub async fn dispatch_to_module(
    target_module: String,
    method: String,
    payload: serde_json::Value,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<serde_json::Value, String> {
    route_module_command(target_module, method, payload, &host).await
}

#[tauri::command]
pub async fn route_sidecar_command(
    target_module: String,
    method: String,
    payload: serde_json::Value,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<serde_json::Value, String> {
    route_module_command(target_module, method, payload, &host).await
}

#[tauri::command]
pub async fn execute_cash_in_transaction(
    customer_pubkey: String,
    amount_shannons: u64,
    fiat_received: f64,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<EdgeTransaction, String> {
    let host = host.lock().await;
    let response = host
        .route_command(
            "fiat_bridge",
            "process_cash_in",
            json!({
                "customer_pubkey": customer_pubkey,
                "amount_shannons": amount_shannons,
                "fiat_received": fiat_received,
            }),
        )
        .await?;
    serde_json::from_value(response).map_err(|err| format!("cash-in decode failed: {err}"))
}

#[tauri::command]
pub async fn trigger_manual_fiat_rebalance(
    current_fiat: f64,
    digital_l2_balance_shannons: Option<u64>,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let digital_l2_balance_shannons = digital_l2_balance_shannons.unwrap_or(6_200_000);
    let host = host.lock().await;
    let response = host
        .route_command(
            "fiat_bridge",
            "dispatch_float_crisis_clearing",
            json!({
                "current_fiat": current_fiat,
                "drain_rate": 450.0,
                "digital_l2_balance_shannons": digital_l2_balance_shannons,
            }),
        )
        .await?;

    if response["status"] == "safe" {
        return Ok("Reserves are within safe bounds. Rebalancing omitted.".to_string());
    }

    let telemetry = &response["telemetry"];
    let mfa_response = &response["mfa_response"];
    let alert_json = serde_json::to_string(telemetry)
        .map_err(|err| format!("telemetry serialization failed: {err}"))?;
    let _ = app_handle.emit("float-crisis", &alert_json);
    let _ = app_handle.emit("clearing-dispatch", mfa_response);

    let status = mfa_response
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("UNKNOWN");

    if status == "MFA_OFFLINE" {
        let queued = mfa_response
            .get("queued")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        return Ok(format!(
            "Float floor breached at TZS {current_fiat:.0}. MFA clearinghouse is offline — telemetry {} for retry. Start MFA (fnn-testnet/start-live-mfa.ps1) on 127.0.0.1:1025.",
            if queued { "queued locally" } else { "could not be queued" }
        ));
    }

    Ok(format!(
        "Float floor breached. Telemetry dispatched to MFA clearinghouse (status: {status})."
    ))
}

#[tauri::command]
pub async fn calculate_invoice_preview(
    target_fiat: f64,
    flat_commission: f64,
    proportional_ppm: u32,
    sovereign_levy: f64,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<FeeCalculationBreakdown, String> {
    let host = host.lock().await;
    let response = host
        .route_command(
            "fiat_bridge",
            "calculate_invoice_preview",
            json!({
                "target_fiat": target_fiat,
                "flat_commission": flat_commission,
                "proportional_ppm": proportional_ppm,
                "sovereign_levy": sovereign_levy,
            }),
        )
        .await?;
    serde_json::from_value(response).map_err(|err| format!("fee breakdown decode failed: {err}"))
}

#[tauri::command]
pub async fn toggle_hardware_profile(
    new_profile: Option<String>,
    profile: State<'_, Arc<HardwareProfileState>>,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
    mfa_bus: State<'_, Arc<MfaControlBus>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let (power_name, profile_label) = {
        let mut state = profile
            .0
            .lock()
            .map_err(|_| "hardware profile lock poisoned".to_string())?;
        let next = if let Some(name) = new_profile {
            HardwareProfile::from_power_profile(&name)
                .ok_or_else(|| format!("unknown power profile: {name}"))?
        } else {
            state.0.toggle()
        };
        let power = match next {
            HardwareProfile::SimulatedKiosk => PowerProfile::BatterySaver,
            HardwareProfile::LiveEdgeNode => PowerProfile::AggressiveRealTime,
        };
        state.1.set_profile(power);
        state.0 = next;
        (next.power_profile_name().to_string(), next.label().to_string())
    };

    let agent_id = host.lock().await.agent_id;
    let broadcast = serde_json::json!({
        "event": "FSP_HARDWARE_PROFILE_CHANGED",
        "new_profile": power_name,
        "agent_id": agent_id,
    });
    if let Err(err) = mfa_bus.try_publish_sys_broadcast(broadcast) {
        log::warn!("Failed to publish hardware profile sys_broadcast: {err}");
    }

    let _ = app_handle.emit("profile-changed-callback", &power_name);
    let _ = app_handle.emit("hardware-profile", profile_label);
    Ok(power_name)
}

fn validate_oob_peer_method(target_module: &str, method: &str) -> Result<(), String> {
    if !is_allowed_oob_peer_method(target_module, method) {
        return Err(format!(
            "OOB peer method '{method}' is not allowed for module '{target_module}'."
        ));
    }
    Ok(())
}

fn render_oob_qr_svg(uri: &str) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::M)
        .map_err(|e| format!("QR encode failed: {e}"))?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#0b1f33"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OobFallbackResponse {
    pub uri: String,
    pub qr_svg: String,
}

#[tauri::command]
pub async fn generate_oob_fallback_uri(
    target_module: String,
    target_agent: u16,
    method: String,
    payload: serde_json::Value,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<OobFallbackResponse, String> {
    validate_route_identifier(&target_module, "target_module")?;
    validate_route_identifier(&method, "method")?;
    validate_oob_peer_method(&target_module, &method)?;

    let host = host.lock().await;
    if !host.is_module_mounted(&target_module).await {
        return Err(format!(
            "Module '{target_module}' is not mounted on this sidecar."
        ));
    }

    let payload_json = serde_json::to_string(&payload)
        .map_err(|err| format!("payload serialization failed: {err}"))?;
    if payload_json.len() > 1024 {
        return Err(
            "Payload exceeds maximum safe bounds (1024 bytes) for EcLevel::M QR encoding"
                .to_string(),
        );
    }

    let uri = host.generate_oob_fallback(&target_module, target_agent, &method, payload)?;
    let qr_svg = render_oob_qr_svg(&uri)?;
    Ok(OobFallbackResponse { uri, qr_svg })
}

#[tauri::command]
pub async fn process_oob_fallback(
    uri_string: String,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<String, String> {
    let trimmed = uri_string.trim();
    if trimmed.is_empty() {
        return Err("OOB URI is empty".to_string());
    }

    let host = host.lock().await;
    host.process_oob_fallback(trimmed).await?;
    Ok("OOB payload routed to local module".to_string())
}

#[tauri::command]
pub async fn execute_dico_contribution(
    payload: ContributionPayload,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<MicroContributionReceipt, String> {
    let host = host.lock().await;
    let response = host
        .route_command(
            "dicoba",
            "stream_micro_contribution",
            json!({
                "vault_config": payload.vault_config,
                "amount_fiat": payload.amount_fiat,
                "shannons_conversion_rate": payload.shannons_conversion_rate,
            }),
        )
        .await?;
    serde_json::from_value(response).map_err(|err| format!("receipt decode failed: {err}"))
}

#[derive(Debug, Serialize)]
pub struct InstalledModuleSnapshot {
    pub id: String,
    pub module_name: String,
    pub is_active: bool,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSidecarModuleRequest {
    pub module_name: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleNameRequest {
    pub module_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleSidecarModuleRequest {
    pub module_name: String,
    pub is_active: bool,
}

#[tauri::command]
pub fn fetch_module_catalog() -> Vec<serde_json::Value> {
    catalog_entries()
}

#[tauri::command]
pub async fn fetch_installed_modules(
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<Vec<InstalledModuleSnapshot>, String> {
    let host = host.lock().await;
    let rows = host.db.get_installed_modules()?;
    Ok(rows
        .into_iter()
        .map(|row| InstalledModuleSnapshot {
            id: row.id,
            module_name: row.module_name,
            is_active: row.is_active,
            config: serde_json::from_str(&row.config_json).unwrap_or_else(|_| json!({})),
        })
        .collect())
}

#[tauri::command]
pub async fn install_sidecar_module(
    request: InstallSidecarModuleRequest,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<InstalledModuleSnapshot, String> {
    let reloader = {
        let host = host.lock().await;
        host.hot_reloader()
            .ok_or_else(|| "hot reloader not initialized".to_string())?
    };
    let record = reloader
        .install_and_mount(&request.module_name, request.config)
        .await?;
    Ok(InstalledModuleSnapshot {
        id: record.id,
        module_name: record.module_name,
        is_active: record.is_active,
        config: serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({})),
    })
}

#[tauri::command]
pub async fn uninstall_sidecar_module(
    request: ModuleNameRequest,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<(), String> {
    let reloader = {
        let host = host.lock().await;
        host.hot_reloader()
            .ok_or_else(|| "hot reloader not initialized".to_string())?
    };
    reloader.uninstall(&request.module_name).await
}

#[tauri::command]
pub async fn toggle_sidecar_module(
    request: ToggleSidecarModuleRequest,
    host: State<'_, Arc<TokioMutex<SidecarHost>>>,
) -> Result<InstalledModuleSnapshot, String> {
    let reloader = {
        let host = host.lock().await;
        host.hot_reloader()
            .ok_or_else(|| "hot reloader not initialized".to_string())?
    };
    let record = reloader
        .toggle(&request.module_name, request.is_active)
        .await?;
    Ok(InstalledModuleSnapshot {
        id: record.id,
        module_name: record.module_name,
        is_active: record.is_active,
        config: serde_json::from_str(&record.config_json).unwrap_or_else(|_| json!({})),
    })
}

#[cfg(test)]
mod route_validation_tests {
    use super::{validate_module_route, validate_route_identifier};

    #[test]
    fn route_identifier_rejects_empty_and_symbols() {
        assert!(validate_route_identifier("", "method").is_err());
        assert!(validate_route_identifier("bad-method", "method").is_err());
        assert!(validate_route_identifier("valid_method", "method").is_ok());
    }

    #[test]
    fn module_route_allowlist_blocks_unknown_targets() {
        assert!(validate_module_route("dicoba", "request_loan").is_ok());
        assert!(validate_module_route("unknown", "request_loan").is_err());
        assert!(validate_module_route("dicoba", "drop_table").is_err());
        assert!(validate_module_route("fiat_bridge", "process_cash_in").is_ok());
    }
}
