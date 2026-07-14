//! Spawn the bundled FNN `externalBin` sidecar (`binaries/fnn`) for desktop testnet.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// Logical sidecar name — must match `bundle.externalBin` in `tauri.conf.json`
/// (not the target-triple filename).
pub const FNN_SIDECAR_NAME: &str = "binaries/fnn";

const DEFAULT_FNN_PASSWORD: &str = "fspdevs-local";
/// Local Fiber JSON-RPC (sidecar → desktop host). Never bind publicly.
const DEFAULT_RPC_LISTEN: &str = "127.0.0.1:8227";
const DEFAULT_RPC: &str = "http://127.0.0.1:8227";
/// Treasury Hub CKB node for Layer-1 state verification (overridable via `CKB_NODE_RPC_URL`).
const DEFAULT_CKB_NODE_RPC: &str = "http://134.122.120.65:8114";
/// Optional indexer; Fiber 0.8 exposes no dedicated flag — kept for env/docs parity.
const DEFAULT_CKB_INDEXER_RPC: &str = "http://134.122.120.65:8116";

/// Managed so the child is killed when the desktop app exits.
pub struct BundledFnnProcess {
    pub child: Option<CommandChild>,
}

impl BundledFnnProcess {
    pub fn kill(&mut self) {
        if let Some(child) = self.child.take() {
            if let Err(err) = child.kill() {
                log::warn!("[fnn] failed to stop bundled sidecar: {err}");
            } else {
                log::info!("[fnn] bundled sidecar stopped");
            }
        }
    }
}

fn simulate_mode() -> bool {
    matches!(
        env::var("FNN_MODE").unwrap_or_default().to_ascii_lowercase().as_str(),
        "simulate" | "sim"
    )
}

fn skip_bundled_spawn() -> bool {
    env::var("FNN_SKIP_SIDECAR")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

async fn rpc_reachable(rpc_url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build();
    let Ok(client) = client else {
        return false;
    };
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "node_info",
        "params": [],
    });
    client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn ensure_secret_password() {
    if env::var("FIBER_SECRET_KEY_PASSWORD").is_err() {
        env::set_var("FIBER_SECRET_KEY_PASSWORD", DEFAULT_FNN_PASSWORD);
    }
}

fn resolve_fnn_data_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    // Same absolute TokenPesa state root as SQLite (`%APPDATA%\TokenPesa\state\fnn`).
    fiber_agent::resolve_fnn_state_dir()
}

fn ensure_config(data_dir: &Path) -> Result<PathBuf, String> {
    let config_path = data_dir.join("config.yml");
    if config_path.is_file() {
        return Ok(config_path);
    }

    // Dev convenience: reuse fnn-testnet template when present next to the workspace.
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fnn-testnet/config/testnet/config.yml"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fnn-testnet/data/config.yml"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            std::fs::copy(&candidate, &config_path)
                .map_err(|e| format!("copy FNN config from {} failed: {e}", candidate.display()))?;
            log::info!("[fnn] seeded config from {}", candidate.display());
            return Ok(config_path);
        }
    }

    Err(
        "No FNN config.yml found. Set FNN_DATA_DIR to a folder with config.yml, \
         or keep fnn-testnet/config/testnet/config.yml in the repo for first-run seeding."
            .into(),
    )
}

fn ensure_dev_key(data_dir: &Path) -> Result<(), String> {
    let ckb_dir = data_dir.join("ckb");
    std::fs::create_dir_all(&ckb_dir).map_err(|e| format!("ckb dir create failed: {e}"))?;
    let key_path = ckb_dir.join("key");
    if key_path.is_file() {
        return Ok(());
    }
    // Same hex seed as fnn-testnet/setup-testnet-key.ps1 (dev/testnet only).
    let dev_key_hex = "2a".repeat(32);
    std::fs::write(&key_path, dev_key_hex).map_err(|e| format!("write CKB key failed: {e}"))?;
    log::info!("[fnn] created dev CKB key at {}", key_path.display());
    Ok(())
}

/// Spawn bundled FNN when needed. Returns `Ok(None)` when skipped (already up / simulate).
pub async fn spawn_bundled_fnn_if_needed(app: &AppHandle) -> Result<Option<CommandChild>, String> {
    if simulate_mode() {
        log::info!("[fnn] FNN_MODE=simulate — skipping bundled sidecar");
        return Ok(None);
    }
    if skip_bundled_spawn() {
        log::info!("[fnn] FNN_SKIP_SIDECAR set — skipping bundled sidecar");
        return Ok(None);
    }

    let rpc_url = env::var("FNN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC.to_string());
    if rpc_reachable(&rpc_url).await {
        log::info!("[fnn] RPC already reachable at {rpc_url} — not spawning sidecar");
        return Ok(None);
    }

    ensure_secret_password();
    let data_dir = resolve_fnn_data_dir(app)?;
    let config_path = ensure_config(&data_dir)?;
    ensure_dev_key(&data_dir)?;

    let config_arg = config_path
        .to_str()
        .ok_or_else(|| "FNN config path is not UTF-8".to_string())?
        .to_string();
    let data_arg = data_dir
        .to_str()
        .ok_or_else(|| "FNN data path is not UTF-8".to_string())?
        .to_string();

    // Real Fiber CLI flags (see `fnn --help`): `--ckb-rpc` / `--listen` are not valid.
    let ckb_rpc = env::var("CKB_NODE_RPC_URL").unwrap_or_else(|_| DEFAULT_CKB_NODE_RPC.to_string());
    let rpc_listen =
        env::var("RPC_LISTENING_ADDR").unwrap_or_else(|_| DEFAULT_RPC_LISTEN.to_string());
    // Fiber 0.8 has no `--ckb-indexer-rpc`; document intended hub indexer for ops.
    let ckb_indexer = env::var("CKB_INDEXER_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_CKB_INDEXER_RPC.to_string());

    log::info!(
        "[fnn] spawning sidecar `{FNN_SIDECAR_NAME}` (-c {config_arg} -d {data_arg} \
         --ckb-node-rpc-url {ckb_rpc} --rpc-listening-addr {rpc_listen}; \
         hub indexer expected at {ckb_indexer}, not passed — unsupported by this fnn build)"
    );

    let (mut rx, child) = app
        .shell()
        .sidecar(FNN_SIDECAR_NAME)
        .map_err(|err| format!("Failed to initialize the FNN sidecar: {err}"))?
        .args([
            "-c",
            &config_arg,
            "-d",
            &data_arg,
            // Layer-1 verification against the remote Treasury Hub CKB node.
            "--ckb-node-rpc-url",
            &ckb_rpc,
            // Fiber routing API strictly on localhost.
            "--rpc-listening-addr",
            &rpc_listen,
        ])
        .spawn()
        .map_err(|err| format!("FATAL: Failed to spawn the live FNN node process: {err}"))?;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                    let text = String::from_utf8_lossy(&line);
                    let trimmed = text.trim_end();
                    if !trimmed.is_empty() {
                        log::info!("[fnn] {trimmed}");
                    }
                }
                CommandEvent::Error(err) => log::error!("[fnn] sidecar error: {err}"),
                CommandEvent::Terminated(payload) => {
                    log::warn!("[fnn] sidecar terminated: {payload:?}");
                }
                _ => {}
            }
        }
    });

    // Give the ckb actor time to come up before module host probes RPC.
    for attempt in 1..=20 {
        if rpc_reachable(&rpc_url).await {
            log::info!("[fnn] bundled sidecar RPC ready at {rpc_url} (attempt {attempt})");
            return Ok(Some(child));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    log::warn!(
        "[fnn] sidecar spawned but RPC not ready at {rpc_url} — host will panic on testnet probe"
    );
    Ok(Some(child))
}
