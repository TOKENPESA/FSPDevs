//! Spawn the bundled Fiber node for desktop testnet (Tauri externalBin or sibling fnn.exe).

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// Logical sidecar name — must match `bundle.externalBin` in `tauri.conf.json`.
pub const FNN_SIDECAR_NAME: &str = "binaries/fnn";

const DEFAULT_FNN_PASSWORD: &str = "fspdevs-local";
const DEFAULT_RPC_LISTEN: &str = "127.0.0.1:8227";
const DEFAULT_RPC: &str = "http://127.0.0.1:8227";
const DEFAULT_CKB_NODE_RPC: &str = "http://134.122.120.65:8114";
const DEFAULT_CKB_INDEXER_RPC: &str = "http://134.122.120.65:8116";
/// Installed builds often need >10s (CKB contract fetch + key migration).
const RPC_READY_ATTEMPTS: u32 = 60;
const RPC_READY_INTERVAL_MS: u64 = 500;

/// Embedded testnet template for NSIS installs (no repo checkout beside Program Files).
const EMBEDDED_FNN_CONFIG: &str =
    include_str!("../../../fnn-testnet/config/testnet/config.yml");

enum FnnChild {
    Sidecar(CommandChild),
    Native(Child),
}

/// Managed so the child is killed when the desktop app exits.
pub struct BundledFnnProcess {
    child: Option<FnnChild>,
}

impl BundledFnnProcess {
    pub fn new(child: Option<CommandChild>) -> Self {
        Self {
            child: child.map(FnnChild::Sidecar),
        }
    }

    pub fn from_native(child: Option<Child>) -> Self {
        Self {
            child: child.map(FnnChild::Native),
        }
    }

    pub fn kill(&mut self) {
        if !kill_fnn_on_exit() {
            // Detach: leave FNN running under AppData so channel DB survives FA restarts.
            // Next launch reuses RPC at 127.0.0.1:8227.
            if self.child.take().is_some() {
                log::info!(
                    "[fnn] leaving bundled FNN running (set FIBER_KILL_FNN_ON_EXIT=1 to stop it with the app)"
                );
            }
            return;
        }
        match self.child.take() {
            Some(FnnChild::Sidecar(child)) => {
                if let Err(err) = child.kill() {
                    log::warn!("[fnn] failed to stop bundled sidecar: {err}");
                } else {
                    log::info!("[fnn] bundled sidecar stopped");
                }
            }
            Some(FnnChild::Native(mut child)) => {
                if let Err(err) = child.kill() {
                    log::warn!("[fnn] failed to stop native FNN: {err}");
                } else {
                    let _ = child.wait();
                    log::info!("[fnn] native FNN stopped");
                }
            }
            None => {}
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
    fiber_agent::resolve_fnn_state_dir()
}

fn ensure_config(data_dir: &Path) -> Result<PathBuf, String> {
    let config_path = data_dir.join("config.yml");
    if config_path.is_file() {
        return Ok(config_path);
    }

    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fnn-testnet/config/testnet/config.yml"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fnn-testnet/data/config.yml"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            std::fs::copy(&candidate, &config_path)
                .map_err(|e| format!("copy FNN config from {} failed: {e}", candidate.display()))?;
            println!("[fnn] seeded config from {}", candidate.display());
            return Ok(config_path);
        }
    }

    // Packaged install: write the compile-time embedded testnet template.
    std::fs::write(&config_path, EMBEDDED_FNN_CONFIG)
        .map_err(|e| format!("write embedded FNN config failed: {e}"))?;
    println!(
        "[fnn] wrote embedded testnet config to {}",
        config_path.display()
    );
    Ok(config_path)
}

/// Old desktop builds wrote this fixed demo key (`"2a"` × 32) — every install shared one `ckt1`.
const UNIQUE_CKB_KEY_MARKER: &str = ".fsp-unique-ckb-key";

fn shared_demo_ckb_key_hex() -> String {
    "2a".repeat(32)
}

fn wipe_ckb_dir(ckb_dir: &Path) -> Result<(), String> {
    if !ckb_dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(ckb_dir)
        .map_err(|e| format!("read ckb dir failed: {e}"))?
    {
        let entry = entry.map_err(|e| format!("ckb dir entry failed: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("remove {} failed: {e}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("remove {} failed: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn generate_ckb_key_hex() -> Result<String, String> {
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).map_err(|e| format!("CSPRNG for CKB key failed: {e}"))?;
    Ok(hex::encode(key))
}

fn fiber_store_present(data_dir: &Path) -> bool {
    let store = data_dir.join("fiber").join("store");
    store.is_dir()
        && std::fs::read_dir(&store)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

/// Ensure this install has a **unique** CKB funding key (unique testnet `ckt1` address).
///
/// Migrates away from the shared demo key (`2a`×32) that older builds wrote on every machine.
/// Never wipe `ckb/` when a Fiber store already exists — that would orphan live channels.
fn ensure_ckb_key(data_dir: &Path) -> Result<(), String> {
    let ckb_dir = data_dir.join("ckb");
    std::fs::create_dir_all(&ckb_dir).map_err(|e| format!("ckb dir create failed: {e}"))?;
    let key_path = ckb_dir.join("key");
    let marker_path = ckb_dir.join(UNIQUE_CKB_KEY_MARKER);

    // Marker means this install already has a unique CKB identity (plaintext key may be gone after FNN import).
    if marker_path.is_file() {
        return Ok(());
    }

    let existing = std::fs::read_to_string(&key_path).unwrap_or_default();
    let trimmed = existing.trim();
    let is_shared_demo = trimmed.eq_ignore_ascii_case(&shared_demo_ckb_key_hex());
    let has_fiber_store = fiber_store_present(data_dir);

    if key_path.is_file() && !is_shared_demo && !trimmed.is_empty() {
        // Pre-existing unique key (manual or prior good install) — mark and keep.
        std::fs::write(&marker_path, b"1")
            .map_err(|e| format!("write CKB key marker failed: {e}"))?;
        return Ok(());
    }

    // Live Fiber DB already present: never rotate keys (would drop channel persistence).
    if has_fiber_store {
        log::warn!(
            "[fnn] Fiber store already exists — keeping CKB material and marking install unique (no wipe)"
        );
        std::fs::write(&marker_path, b"1")
            .map_err(|e| format!("write CKB key marker failed: {e}"))?;
        if !key_path.is_file() {
            // FNN may have ingested plaintext into its encrypted store already.
            println!(
                "[fnn] no plaintext ckb/key (normal after first boot) — Fiber store left intact"
            );
        }
        return Ok(());
    }

    if is_shared_demo {
        println!(
            "[fnn] replacing shared demo CKB key so this install gets its own testnet address"
        );
        log::warn!(
            "[fnn] rotating shared demo CKB key — faucet coins on the old shared ckt1 address stay on-chain under that key"
        );
        wipe_ckb_dir(&ckb_dir)?;
        std::fs::create_dir_all(&ckb_dir).map_err(|e| format!("ckb dir recreate failed: {e}"))?;
    } else if !key_path.is_file() {
        println!("[fnn] seeding a unique CKB key for this install");
    }

    let key_hex = generate_ckb_key_hex()?;
    std::fs::write(&key_path, &key_hex).map_err(|e| format!("write CKB key failed: {e}"))?;
    std::fs::write(&marker_path, b"1").map_err(|e| format!("write CKB key marker failed: {e}"))?;
    println!("[fnn] created unique CKB key at {}", key_path.display());
    Ok(())
}

/// Whether to terminate the bundled FNN when Fiber Agent exits.
/// Default **false**: keep FNN running so RocksDB channel state survives app restarts.
fn kill_fnn_on_exit() -> bool {
    env::var("FIBER_KILL_FNN_ON_EXIT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn sibling_fnn_exe() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?;
    for name in ["fnn.exe", "fnn"] {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

async fn wait_for_rpc(rpc_url: &str) -> bool {
    for attempt in 1..=RPC_READY_ATTEMPTS {
        if rpc_reachable(rpc_url).await {
            println!("[fnn] RPC ready at {rpc_url} (attempt {attempt})");
            return true;
        }
        tokio::time::sleep(Duration::from_millis(RPC_READY_INTERVAL_MS)).await;
    }
    false
}

/// Spawn bundled/local FNN when needed.
pub async fn spawn_bundled_fnn_if_needed(app: &AppHandle) -> Result<BundledFnnProcess, String> {
    if simulate_mode() {
        println!("[fnn] FNN_MODE=simulate — skipping bundled sidecar");
        return Ok(BundledFnnProcess { child: None });
    }
    if skip_bundled_spawn() {
        println!("[fnn] FNN_SKIP_SIDECAR set — skipping bundled sidecar");
        return Ok(BundledFnnProcess { child: None });
    }

    let rpc_url = env::var("FNN_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC.to_string());
    if rpc_reachable(&rpc_url).await {
        // Reuse persistent FNN left running after a previous FA session (channel DB intact).
        println!(
            "[fnn] RPC already reachable at {rpc_url} — reusing existing FNN (channel state preserved)"
        );
        return Ok(BundledFnnProcess { child: None });
    }

    ensure_secret_password();
    let data_dir = resolve_fnn_data_dir(app)?;
    let config_path = ensure_config(&data_dir)?;
    ensure_ckb_key(&data_dir)?;

    let config_arg = config_path
        .to_str()
        .ok_or_else(|| "FNN config path is not UTF-8".to_string())?
        .to_string();
    let data_arg = data_dir
        .to_str()
        .ok_or_else(|| "FNN data path is not UTF-8".to_string())?
        .to_string();
    let ckb_rpc = env::var("CKB_NODE_RPC_URL").unwrap_or_else(|_| DEFAULT_CKB_NODE_RPC.to_string());
    let rpc_listen =
        env::var("RPC_LISTENING_ADDR").unwrap_or_else(|_| DEFAULT_RPC_LISTEN.to_string());
    let _ckb_indexer = env::var("CKB_INDEXER_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_CKB_INDEXER_RPC.to_string());

    println!(
        "[fnn] spawning FNN (-c {config_arg} -d {data_arg} --ckb-node-rpc-url {ckb_rpc} --rpc-listening-addr {rpc_listen})"
    );

    // Prefer sibling fnn.exe (NSIS installs place it next to the app) — more reliable than
    // shell sidecar resolution when the install path contains spaces ("Fiber Agent").
    if let Some(fnn_exe) = sibling_fnn_exe() {
        println!("[fnn] launching native binary {}", fnn_exe.display());
        let mut cmd = Command::new(&fnn_exe);
        cmd.args([
            "-c",
            &config_arg,
            "-d",
            &data_arg,
            "--ckb-node-rpc-url",
            &ckb_rpc,
            "--rpc-listening-addr",
            &rpc_listen,
        ])
        .current_dir(&data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        // fnn.exe is a Windows console binary — hide the empty black terminal on launch.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = cmd
            .spawn()
            .map_err(|err| format!("FATAL: Failed to spawn fnn.exe: {err}"))?;

        if wait_for_rpc(&rpc_url).await {
            return Ok(BundledFnnProcess::from_native(Some(child)));
        }
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "FATAL: FNN started but RPC not ready at {rpc_url} within {}s. Check {data_arg}\\config.yml and CKB RPC {ckb_rpc}.",
            (RPC_READY_ATTEMPTS as u64 * RPC_READY_INTERVAL_MS) / 1000
        ));
    }

    // Dev / externalBin path via Tauri shell plugin.
    let (mut rx, child) = app
        .shell()
        .sidecar(FNN_SIDECAR_NAME)
        .map_err(|err| format!("Failed to initialize the FNN sidecar: {err}"))?
        .args([
            "-c",
            &config_arg,
            "-d",
            &data_arg,
            "--ckb-node-rpc-url",
            &ckb_rpc,
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
                        println!("[fnn] {trimmed}");
                    }
                }
                CommandEvent::Error(err) => eprintln!("[fnn] sidecar error: {err}"),
                CommandEvent::Terminated(payload) => {
                    eprintln!("[fnn] sidecar terminated: {payload:?}");
                }
                _ => {}
            }
        }
    });

    if wait_for_rpc(&rpc_url).await {
        return Ok(BundledFnnProcess::new(Some(child)));
    }

    let mut proc = BundledFnnProcess::new(Some(child));
    proc.kill();
    Err(format!(
        "FATAL: FNN sidecar spawned but RPC not ready at {rpc_url} within {}s.",
        (RPC_READY_ATTEMPTS as u64 * RPC_READY_INTERVAL_MS) / 1000
    ))
}
