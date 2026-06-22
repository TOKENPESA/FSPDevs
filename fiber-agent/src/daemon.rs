use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secp256k1::SecretKey;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::hot_swap::execute_hot_swap;
use crate::payment::execute_mesh_payment;
use crate::{
    aggregate_active_balances, attach_telemetry_signature, flush_queued_telemetry,
    fnn_client::FiberNodeRpc, mesh_unix_timestamp_secs, refresh_pubkey_cache,
    resolve_agent_secret_key, resolve_fnn_backend, resolve_fnn_rpc_url, send_or_queue_telemetry,
    AgentDb, ConfigUpdatePayload, MeshPubkeyRegistry, MeshPulsePayload,
};
use crate::storage::channel_cache_from_mesh;

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
                .unwrap_or_else(|_| "tpxdevs-local-ws".into()),
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
    let mfa_telemetry_url = format!("http://{}/telemetry", config.mfa_host);
    let ws_token = config.ws_token.clone();
    let mfa_ws_url = format!(
        "ws://{}/ws/{agent_id}?token={ws_token}",
        config.mfa_host
    );
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

    spawn_mfa_ws_loop(
        agent_id,
        mfa_ws_url,
        fnn_backend.clone(),
        pubkey_cache.clone(),
        mesh_registry.clone(),
        db.clone(),
        quiet,
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
    })
    .await;
}

fn spawn_mfa_ws_loop(
    agent_id: u16,
    mfa_ws_url: String,
    fnn_backend: Arc<Mutex<Box<dyn FiberNodeRpc + Send + Sync>>>,
    pubkey_cache: Arc<RwLock<HashMap<u16, String>>>,
    mesh_registry: Arc<MeshPubkeyRegistry>,
    db: Option<Arc<AgentDb>>,
    quiet: bool,
) {
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
                                                    if ws_tx.send(Message::Text(json)).await.is_err() {
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
                            _ = keepalive.tick() => {
                                if ws_tx.send(Message::Ping(Vec::new())).await.is_err() {
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

async fn run_telemetry_poll_loop(ctx: TelemetryPollContext) {
    let TelemetryPollContext {
        agent_id,
        fnn_backend,
        pubkey_cache,
        http_client,
        mfa_telemetry_url,
        db,
        signing_key,
        local_fnn_pubkey,
        quiet,
    } = ctx;
    let min_channels: usize = std::env::var("FIBER_AGENT_MIN_ACTIVE_CHANNELS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut last_balance_alert = None::<std::time::Instant>;

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
                    let alert_payload = attach_telemetry_signature(
                        MeshPulsePayload {
                            status: "ALERT_MFA_NODE_DROPPED".to_string(),
                            agent: agent_id,
                            active_mesh_neighbors: active_neighbors.clone(),
                            report_target: broken_channel.peer_id,
                            attempt: 3,
                            timestamp: mesh_unix_timestamp_secs(),
                            public_key_hex: None,
                            signature_hex: None,
                            fnn_pubkey_hex: local_fnn_pubkey.clone(),
                            peer_connect_address: None,
                            outbound_shannons: None,
                            inbound_shannons: None,
                        },
                        &signing_key,
                    );
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
                            MeshPulsePayload {
                                status: "ALERT_BALANCE_DEPLETED".to_string(),
                                agent: agent_id,
                                active_mesh_neighbors: active_neighbors.clone(),
                                report_target: agent_id,
                                attempt: 1,
                                timestamp: mesh_unix_timestamp_secs(),
                                public_key_hex: None,
                                signature_hex: None,
                                fnn_pubkey_hex: local_fnn_pubkey.clone(),
                                peer_connect_address: peer_addr,
                                outbound_shannons: None,
                                inbound_shannons: None,
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
                    let (outbound_shannons, inbound_shannons) =
                        aggregate_active_balances(&channels);
                    let heartbeat = attach_telemetry_signature(
                        MeshPulsePayload {
                            status: "MESH_HEARTBEAT".to_string(),
                            agent: agent_id,
                            active_mesh_neighbors: active_neighbors,
                            report_target: agent_id,
                            attempt: 0,
                            timestamp: mesh_unix_timestamp_secs(),
                            public_key_hex: None,
                            signature_hex: None,
                            fnn_pubkey_hex: local_fnn_pubkey.clone(),
                            peer_connect_address: None,
                            outbound_shannons: Some(outbound_shannons),
                            inbound_shannons: Some(inbound_shannons),
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
        3_000
    }
}
