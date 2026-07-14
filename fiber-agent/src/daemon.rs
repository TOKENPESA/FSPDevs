use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secp256k1::SecretKey;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::hot_swap::execute_hot_swap;
use crate::mfa_ws_auth::{inject_agent_ws_auth_headers, mfa_control_ws_url, mfa_http_base};
use crate::payment::execute_mesh_payment;
use crate::power::{AdaptivePowerController, PowerProfile};
use crate::utility_runtime::UtilityRuntime;
use crate::fnn_client::{ArcFnnBackend, FiberNodeRpc, LiveFnnClient};
use crate::{
    aggregate_active_balances, attach_telemetry_signature, flush_queued_telemetry,
    mesh_unix_timestamp_secs, refresh_pubkey_cache, resolve_agent_secret_key, resolve_fnn_backend,
    resolve_fnn_rpc_url, send_or_queue_telemetry, AgentDb, ConfigUpdatePayload, MeshPubkeyRegistry,
    MeshPulsePayload,
};
use crate::storage::{channel_cache_from_mesh, UtilityPaymentIntent};
use mesh_core::network::PeerModulePacket;
use mesh_core::types::{EdgeTransaction, EdgeTxType, L2Asset, SingleCapacityParams};

const BALANCE_ALERT_COOLDOWN_SECS: u64 = 300;

struct TelemetryPollContext {
    agent_id: u16,
    fnn_backend: Arc<Mutex<Box<dyn FiberNodeRpc + Send + Sync>>>,
    pubkey_cache: Arc<RwLock<HashMap<u16, String>>>,
    http_client: reqwest::Client,
    mfa_telemetry_url: String,
    db: Option<Arc<AgentDb>>,
    signing_key: SecretKey,
    local_fnn_pubkey: Option<String>,
    quiet: bool,
    telemetry_nonce: u64,
}

#[derive(Clone, Debug)]
pub struct SidecarConfig {
    pub mfa_host: String,
    pub ws_token: String,
    pub force_simulate_fnn: bool,
    pub quiet: bool,
}

impl SidecarConfig {
    pub fn from_env() -> Self {
        Self {
            mfa_host: std::env::var("MFA_HOST").unwrap_or_else(|_| "127.0.0.1:1025".to_string()),
            ws_token: std::env::var("MFA_AGENT_WS_TOKEN")
                .unwrap_or_else(|_| "fspdevs-local-ws".into()),
            force_simulate_fnn: std::env::var("FNN_MODE")
                .map(|v| v.eq_ignore_ascii_case("simulate") || v.eq_ignore_ascii_case("sim"))
                .unwrap_or(false),
            quiet: std::env::var("MESH_FLEET_QUIET")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}

fn log_agent(quiet: bool, agent_id: u16, msg: &str) {
    if !quiet {
        println!("FA-{agent_id} {msg}");
    }
}

fn log_agent_err(quiet: bool, agent_id: u16, msg: &str) {
    if !quiet {
        eprintln!("FA-{agent_id} {msg}");
    }
}

async fn resolve_backend(
    agent_id: u16,
    rpc_url: &str,
    force_simulate: bool,
) -> Box<dyn FiberNodeRpc + Send + Sync> {
    if force_simulate {
        return Box::new(crate::fnn_client::SimulatedFnnClient::new(agent_id));
    }
    resolve_fnn_backend(agent_id, rpc_url).await
}

/// Runs one Fiber Agent sidecar loop (MFA WS + telemetry + FNN poll) until the task is cancelled.
pub async fn run_agent_sidecar(agent_id: u16, config: SidecarConfig) {
    let mfa_telemetry_url = format!("{}/telemetry", mfa_http_base(&config.mfa_host));
    let ws_token = config.ws_token.clone();
    let mfa_ws_url = mfa_control_ws_url(agent_id, &config.mfa_host);
    let local_fnn_rpc = resolve_fnn_rpc_url(agent_id);
    let quiet = config.quiet;

    log_agent(
        quiet,
        agent_id,
        &format!("sidecar starting (FNN RPC {local_fnn_rpc})"),
    );

    let db = match AgentDb::open(agent_id) {
        Ok(db) => Some(Arc::new(db)),
        Err(e) => {
            log_agent_err(quiet, agent_id, &format!("storage disabled: {e}"));
            None
        }
    };

    let fnn_backend: Arc<Mutex<Box<dyn FiberNodeRpc + Send + Sync>>> = Arc::new(Mutex::new(
        resolve_backend(agent_id, &local_fnn_rpc, config.force_simulate_fnn).await,
    ));
    let pubkey_cache: Arc<RwLock<HashMap<u16, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let mesh_registry = Arc::new(MeshPubkeyRegistry::load());

    let signing_key = match resolve_agent_secret_key(agent_id) {
        Ok(key) => key,
        Err(e) => {
            log_agent_err(quiet, agent_id, &format!("signing key error: {e}"));
            return;
        }
    };

    let local_fnn_pubkey = {
        let backend = fnn_backend.lock().await;
        backend.node_pubkey().await.ok()
    };

    let http_client = reqwest::Client::new();

    if let Some(db_ref) = db.clone() {
        let utility_db = db_ref.clone();
        let utility_fnn = LiveFnnClient::new(local_fnn_rpc.clone());
        let utility_power = AdaptivePowerController::new();
        let utility_runtime = UtilityRuntime {
            flow_rate_units_per_shannon: utility_flow_rate(),
        };
        tokio::spawn(async move {
            run_utility_sidecar_loop(agent_id, utility_db, utility_fnn, utility_power, utility_runtime)
                .await;
        });
    }

    if let Some(db_ref) = db.clone() {
        let client = http_client.clone();
        let url = mfa_telemetry_url.clone();
        let quiet_flush = quiet;
        let flush_signing_key = signing_key;
        tokio::spawn(async move {
            loop {
                let sent =
                    flush_queued_telemetry(&db_ref, &client, &url, &flush_signing_key).await;
                if sent > 0 && !quiet_flush {
                    println!("FA-{agent_id} flushed {sent} queued telemetry packet(s)");
                }
                sleep(Duration::from_secs(15)).await;
            }
        });
    }

    spawn_mfa_control_ws(
        agent_id,
        mfa_ws_url,
        ws_token,
        fnn_backend.clone(),
        pubkey_cache.clone(),
        mesh_registry.clone(),
        db.clone(),
        quiet,
        None,
        None,
    );

    run_telemetry_poll_loop(TelemetryPollContext {
        agent_id,
        fnn_backend,
        pubkey_cache,
        http_client,
        mfa_telemetry_url,
        db,
        signing_key,
        local_fnn_pubkey,
        quiet,
        telemetry_nonce: 0,
    })
    .await;
}

/// Keeps the sidecar registered in MFA's peer registry via `/ws/:agent_id`.
pub fn spawn_sidecar_mfa_control_ws(
    agent_id: u16,
    fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    db: Arc<AgentDb>,
    peer_outbound_rx: Option<mpsc::Receiver<PeerModulePacket>>,
    sys_broadcast_rx: Option<mpsc::Receiver<String>>,
) {
    let config = SidecarConfig::from_env();
    let mfa_ws_url = mfa_control_ws_url(agent_id, &config.mfa_host);
    let fnn_backend = Arc::new(Mutex::new(
        Box::new(ArcFnnBackend(fnn_client)) as Box<dyn FiberNodeRpc + Send + Sync>
    ));
    let pubkey_cache = Arc::new(RwLock::new(HashMap::new()));
    let mesh_registry = Arc::new(MeshPubkeyRegistry::load());
    spawn_mfa_control_ws(
        agent_id,
        mfa_ws_url,
        config.ws_token,
        fnn_backend,
        pubkey_cache,
        mesh_registry,
        Some(db),
        false,
        peer_outbound_rx,
        sys_broadcast_rx,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_mfa_control_ws(
    agent_id: u16,
    mfa_ws_url: String,
    ws_token: String,
    fnn_backend: Arc<Mutex<Box<dyn FiberNodeRpc + Send + Sync>>>,
    pubkey_cache: Arc<RwLock<HashMap<u16, String>>>,
    mesh_registry: Arc<MeshPubkeyRegistry>,
    db: Option<Arc<AgentDb>>,
    quiet: bool,
    peer_outbound_rx: Option<mpsc::Receiver<PeerModulePacket>>,
    mut sys_broadcast_rx: Option<mpsc::Receiver<String>>,
) {
    let p2p_relay_active = peer_outbound_rx.is_some();
    let sys_broadcast_active = sys_broadcast_rx.is_some();
    let (p2p_ws_tx, mut p2p_ws_rx) = mpsc::channel::<String>(100);
    if let Some(mut outbound_rx) = peer_outbound_rx {
        tokio::spawn(async move {
            while let Some(packet) = outbound_rx.recv().await {
                let ws_message = serde_json::json!({
                    "type": "p2p_relay",
                    "data": packet
                });

                if let Err(e) = p2p_ws_tx.try_send(ws_message.to_string()) {
                    match e {
                        tokio::sync::mpsc::error::TrySendError::Full(_) => {
                            log::warn!(
                                "⚠️ [P2P RELAY] Backpressure: dropping outbound module packet (channel full)"
                            );
                        }
                        tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                            log::error!("Network drop: P2P relay channel closed: {e}");
                            break;
                        }
                    }
                }
            }
        });
    }

    tokio::spawn(async move {
        loop {
            let mut request = match mfa_ws_url.as_str().into_client_request() {
                Ok(req) => req,
                Err(e) => {
                    log_agent_err(quiet, agent_id, &format!("invalid WS URL ({e})"));
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            if let Err(err) =
                inject_agent_ws_auth_headers(request.headers_mut(), agent_id, &ws_token)
            {
                log_agent_err(quiet, agent_id, &format!("WS auth header build failed: {err}"));
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            let _ = request.headers_mut().insert(
                "Origin",
                "http://127.0.0.1:8088".parse().unwrap(),
            );

            match tokio_tungstenite::connect_async(request).await {
                Ok((ws_stream, _)) => {
                    log_agent(quiet, agent_id, "MFA control WS connected");
                    let (mut ws_tx, mut ws_rx) = ws_stream.split();
                    let mut keepalive = tokio::time::interval(Duration::from_secs(25));
                    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                    loop {
                        tokio::select! {
                            msg = ws_rx.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        match serde_json::from_str::<ConfigUpdatePayload>(&text) {
                                            Ok(cmd) if cmd.command == "MESH_CHANNEL_HOT_SWAP" => {
                                                execute_hot_swap(
                                                    &fnn_backend,
                                                    &pubkey_cache,
                                                    &mesh_registry,
                                                    &cmd,
                                                )
                                                .await;
                                            }
                                            Ok(cmd) if cmd.command == "MESH_SEND_PAYMENT" => {
                                                let reply = execute_mesh_payment(
                                                    &fnn_backend,
                                                    agent_id,
                                                    &cmd,
                                                    db.as_deref(),
                                                )
                                                .await;
                                                if let Ok(json) = serde_json::to_string(&reply) {
                                                    if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                                        break;
                                                    }
                                                }
                                            }
                                            Ok(_) => {}
                                            Err(e) => log_agent_err(quiet, agent_id, &format!("bad MFA cmd: {e}")),
                                        }
                                    }
                                    Some(Ok(Message::Ping(payload))) => {
                                        if ws_tx.send(Message::Pong(payload)).await.is_err() {
                                            break;
                                        }
                                    }
                                    Some(Ok(Message::Pong(_))) => {}
                                    Some(Ok(Message::Close(_))) | None => break,
                                    Some(Err(e)) => {
                                        log_agent_err(quiet, agent_id, &format!("WS read error: {e}"));
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            p2p_msg = p2p_ws_rx.recv(), if p2p_relay_active => {
                                if let Some(text) = p2p_msg {
                                    if let Err(e) = ws_tx.send(Message::Text(text.into())).await {
                                        log::error!("Network drop: Failed to relay P2P message: {e}");
                                        break;
                                    }
                                }
                            }
                            sys_msg = async {
                                match sys_broadcast_rx.as_mut() {
                                    Some(rx) => rx.recv().await,
                                    None => std::future::pending().await,
                                }
                            }, if sys_broadcast_active => {
                                if let Some(text) = sys_msg {
                                    if let Err(e) = ws_tx.send(Message::Text(text.into())).await {
                                        log::error!("Network drop: Failed to publish sys_broadcast: {e}");
                                        break;
                                    }
                                }
                            }
                            _ = keepalive.tick() => {
                                if ws_tx.send(Message::Ping(Vec::new().into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log_agent_err(quiet, agent_id, &format!("MFA WS unavailable ({e})"));
                }
            }
            sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn run_telemetry_poll_loop(mut ctx: TelemetryPollContext) {
    let agent_id = ctx.agent_id;
    let fnn_backend = ctx.fnn_backend.clone();
    let pubkey_cache = ctx.pubkey_cache.clone();
    let http_client = ctx.http_client.clone();
    let mfa_telemetry_url = ctx.mfa_telemetry_url.clone();
    let db = ctx.db.clone();
    let signing_key = ctx.signing_key;
    let local_fnn_pubkey = ctx.local_fnn_pubkey.clone();
    let quiet = ctx.quiet;
    let min_channels: usize = std::env::var("FIBER_AGENT_MIN_ACTIVE_CHANNELS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut last_balance_alert = None::<std::time::Instant>;

    let mut next_pulse = |status: String,
                          report_target: u16,
                          attempt: u8,
                          local_capacity_shannons: u64,
                          peer_connect_address: Option<String>| {
        ctx.telemetry_nonce = ctx.telemetry_nonce.saturating_add(1);
        MeshPulsePayload {
            agent_id,
            timestamp: mesh_unix_timestamp_secs(),
            nonce: ctx.telemetry_nonce,
            local_capacity_shannons,
            public_key_hex: None,
            signature_hex: None,
            status,
            active_mesh_neighbors: Vec::new(),
            report_target,
            attempt,
            fnn_pubkey_hex: local_fnn_pubkey.clone(),
            peer_connect_address,
            asset_capacities: Vec::new(),
        }
    };

    loop {
        let channels = {
            let backend = fnn_backend.lock().await;
            backend.list_channels().await
        };

        match channels {
            Ok(channels) => {
                if let Some(ref db_ref) = db {
                    let _ = db_ref.replace_channel_snapshot(&channel_cache_from_mesh(&channels));
                }

                {
                    let mut cache = pubkey_cache.write().await;
                    refresh_pubkey_cache(&channels, &mut cache);
                }

                let active_neighbors: Vec<u16> = channels
                    .iter()
                    .filter(|c| c.is_active)
                    .map(|c| c.peer_id)
                    .collect();
                let channel_count = active_neighbors.len();

                if let Some(broken_channel) = channels.iter().find(|c| !c.is_active) {
                    let mut pulse = next_pulse(
                        "ALERT_MFA_NODE_DROPPED".to_string(),
                        broken_channel.peer_id,
                        3,
                        0,
                        None,
                    );
                    pulse.active_mesh_neighbors = active_neighbors.clone();
                    let alert_payload = attach_telemetry_signature(pulse, &signing_key);
                    let _ = send_or_queue_telemetry(
                        &http_client,
                        &mfa_telemetry_url,
                        &db,
                        &alert_payload,
                        "ALERT_MFA_NODE_DROPPED",
                    )
                    .await;
                } else if channel_count < min_channels
                    && std::env::var("FIBER_AGENT_HUB_CHANNEL_FUNDING")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                {
                    let cooldown = Duration::from_secs(BALANCE_ALERT_COOLDOWN_SECS);
                    let due = last_balance_alert
                        .map(|t| t.elapsed() >= cooldown)
                        .unwrap_or(true);
                    if due {
                        last_balance_alert = Some(std::time::Instant::now());
                        let peer_addr = std::env::var(format!("HUB_PEER_ADDR_{agent_id}"))
                            .ok()
                            .or_else(|| std::env::var("HUB_PEER_ADDR").ok());
                        let depleted = attach_telemetry_signature(
                            {
                                let mut pulse = next_pulse(
                                    "ALERT_BALANCE_DEPLETED".to_string(),
                                    agent_id,
                                    1,
                                    0,
                                    peer_addr,
                                );
                                pulse.active_mesh_neighbors = active_neighbors.clone();
                                pulse
                            },
                            &signing_key,
                        );
                        let _ = send_or_queue_telemetry(
                            &http_client,
                            &mfa_telemetry_url,
                            &db,
                            &depleted,
                            "ALERT_BALANCE_DEPLETED",
                        )
                        .await;
                    }
                } else {
                    let (outbound_shannons, _inbound_shannons) =
                        aggregate_active_balances(&channels);
                    let heartbeat = attach_telemetry_signature(
                        {
                            let mut pulse = next_pulse(
                                "MESH_HEARTBEAT".to_string(),
                                agent_id,
                                0,
                                outbound_shannons,
                                None,
                            );
                            pulse.active_mesh_neighbors = active_neighbors;
                            pulse
                        },
                        &signing_key,
                    );
                    let _ = send_or_queue_telemetry(
                        &http_client,
                        &mfa_telemetry_url,
                        &db,
                        &heartbeat,
                        "MESH_HEARTBEAT",
                    )
                    .await;
                }
            }
            Err(e) => log_agent_err(quiet, agent_id, &format!("channel query failed: {e}")),
        }

        sleep(Duration::from_millis(telemetry_poll_interval_ms(quiet))).await;
    }
}

fn telemetry_poll_interval_ms(quiet: bool) -> u64 {
    if let Ok(raw) = std::env::var("MESH_FLEET_HEARTBEAT_MS") {
        if let Ok(ms) = raw.parse::<u64>() {
            return ms.clamp(1_000, 300_000);
        }
    }
    if quiet {
        30_000
    } else {
        let mut power = crate::power::AdaptivePowerController::new();
        power.poll_interval_ms()
    }
}

fn utility_flow_rate() -> f64 {
    std::env::var("UTILITY_FLOW_RATE_PER_SHANNON")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(0.001)
}

fn intent_to_edge_transaction(agent_id: u16, intent: &UtilityPaymentIntent) -> EdgeTransaction {
    EdgeTransaction::single_capacity(SingleCapacityParams {
        tx_id: uuid::Uuid::new_v4(),
        agent_id,
        tx_type: EdgeTxType::CashIn,
        asset: L2Asset::RusdStablecoin,
        amount_atomic: intent.amount_shannons,
        fiat_amount: 0.0,
        counterparty_pubkey: String::new(),
        payment_hash: Some(intent.payment_hash.clone()),
        preimage: None,
        timestamp: chrono::Utc::now().timestamp(),
        is_synchronized: false,
    })
}

fn check_local_ledger_for_intents(
    agent_id: u16,
    db: &AgentDb,
) -> Result<Option<(UtilityPaymentIntent, EdgeTransaction)>, String> {
    let intent = db.fetch_next_pending_utility_intent()?;
    Ok(intent.map(|row| {
        let payment = intent_to_edge_transaction(agent_id, &row);
        (row, payment)
    }))
}

fn payment_hash_format_valid(hash: &str) -> bool {
    let trimmed = hash.trim();
    !trimmed.is_empty()
        && trimmed.len() >= 8
        && trimmed
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, 'x' | 'X' | ':' | '-'))
}

async fn verify_utility_payment(
    payment: &EdgeTransaction,
    db: &AgentDb,
    fnn: &LiveFnnClient,
) -> bool {
    let Some(hash) = payment
        .payment_hash
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
    else {
        return false;
    };

    if !payment_hash_format_valid(hash) {
        return false;
    }

    match db.fiat_ledger_confirms_payment(hash, payment.total_atomic()) {
        Ok(true) => return true,
        Ok(false) => {}
        Err(err) => {
            log_agent_err(false, payment.agent_id, &format!("utility ledger check failed: {err}"));
        }
    }

    match fnn.payment_is_success(hash).await {
        Ok(true) => true,
        Ok(false) => false,
        Err(err) => {
            log_agent_err(
                false,
                payment.agent_id,
                &format!("utility FNN payment verification failed: {err}"),
            );
            false
        }
    }
}

/// Polls offline utility payment intents and dispenses physical resources when confirmed.
pub async fn run_utility_sidecar_loop(
    agent_id: u16,
    db: Arc<AgentDb>,
    fnn: LiveFnnClient,
    mut power: AdaptivePowerController,
    runtime: UtilityRuntime,
) {
    loop {
        match check_local_ledger_for_intents(agent_id, db.as_ref()) {
            Ok(Some((intent, pending_payment))) => {
                power.set_profile(PowerProfile::AggressiveRealTime);

                if verify_utility_payment(&pending_payment, db.as_ref(), &fnn).await {
                    if db
                        .update_utility_intent_status(intent.id, "confirmed")
                        .is_ok()
                    {
                        let _ = runtime.dispense_resource(&pending_payment);
                        let _ = db.update_utility_intent_status(intent.id, "cleared");
                    }
                } else {
                    log_agent_err(
                        false,
                        agent_id,
                        &format!(
                            "utility intent {} rejected — payment not verified on ledger/FNN",
                            intent.id
                        ),
                    );
                    let _ = db.update_utility_intent_status(intent.id, "rejected");
                }
            }
            Ok(None) => {
                power.set_profile(PowerProfile::BatterySaver);
                sleep(Duration::from_millis(power.poll_interval_ms())).await;
            }
            Err(err) => {
                log_agent_err(
                    false,
                    agent_id,
                    &format!("utility ledger poll failed: {err}"),
                );
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod utility_sidecar_tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_intent(hash: &str) -> UtilityPaymentIntent {
        UtilityPaymentIntent {
            id: 1,
            payment_hash: hash.to_string(),
            amount_shannons: 1_000,
            status: "pending".to_string(),
            synced: false,
        }
    }

    fn temp_db_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fiber-agent-utility-{label}-{}.db",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[test]
    fn payment_hash_format_valid_accepts_hex_payment_hash() {
        assert!(payment_hash_format_valid("0xdeadbeef"));
    }

    #[test]
    fn payment_hash_format_valid_rejects_empty_hash() {
        assert!(!payment_hash_format_valid(""));
    }

    #[tokio::test]
    async fn verify_utility_payment_accepts_ledger_settlement() {
        let path = temp_db_path("ledger-ok");
        let db = AgentDb::open_path(path.clone()).expect("open db");
        let edge_tx = EdgeTransaction::single_capacity(SingleCapacityParams {
            tx_id: uuid::Uuid::new_v4(),
            agent_id: 1,
            tx_type: EdgeTxType::CashIn,
            asset: L2Asset::RusdStablecoin,
            amount_atomic: 1_000,
            fiat_amount: 0.0,
            counterparty_pubkey: String::new(),
            payment_hash: Some("0xdeadbeef".to_string()),
            preimage: None,
            timestamp: 1,
            is_synchronized: true,
        });
        db.insert_fiat_edge_transaction(&edge_tx)
            .expect("insert ledger row");
        let payment = intent_to_edge_transaction(1, &sample_intent("0xdeadbeef"));
        let fnn = LiveFnnClient::new("http://127.0.0.1:1".to_string());
        assert!(verify_utility_payment(&payment, &db, &fnn).await);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn verify_utility_payment_rejects_unsettled_hash() {
        let path = temp_db_path("ledger-miss");
        let db = AgentDb::open_path(path.clone()).expect("open db");
        let payment = intent_to_edge_transaction(1, &sample_intent("0xdeadbeef"));
        let fnn = LiveFnnClient::new("http://127.0.0.1:1".to_string());
        assert!(!verify_utility_payment(&payment, &db, &fnn).await);
        let _ = std::fs::remove_file(path);
    }
}
