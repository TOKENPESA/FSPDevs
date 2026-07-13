use mesh_core::RING_SIZE;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::routing::get;
use axum::Router;
use hyper::body::Incoming;
use hyper_util::service::TowerToHyperService;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt as _;
use tower_http::limit::RequestBodyLimitLayer;

pub const PAYMENT_EXEC_TIMEOUT_SECS: u64 = 45;
pub const TELEMETRY_QUEUE: usize = 8192;
pub const BROADCAST_CAP: usize = 2048;
/// In-flight sovereign audit envelopes on the compliance SSE bus.
pub const COMPLIANCE_BROADCAST_CAP: usize = 100;
pub const PEER_TX_CAP: usize = 32;
pub const HEARTBEAT_UI_MIN_INTERVAL_MS: u64 = 250;
/// Max HTTP POST body size for telemetry and batch intake (64 KiB).
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const DEDUPE_CAP: usize = 2048;
pub const DEFAULT_HUB_FUNDING_SHANNONS: u64 = 50_000_000_000;
pub const DEFAULT_AGENT_WS_TOKEN: &str = "fspdevs-local-ws";
pub const DEFAULT_MFA_API_TOKEN: &str = "fspdevs-local-api-devonly";
pub const DEFAULT_HUB_FUNDING_LOCK_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_LIQUIDITY_COPILOT_LOW_WATERMARK_SHANNONS: u64 = 5_000_000_000;
pub const DEFAULT_LIQUIDITY_DEPLETION_HORIZON_SECS: u64 = 120;
pub const DEFAULT_LIQUIDITY_COPILOT_COOLDOWN_SECS: u64 = 300;

pub fn hub_funding_lock_timeout_secs() -> u64 {
    env::var("HUB_FUNDING_LOCK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&secs| secs > 0)
        .unwrap_or(DEFAULT_HUB_FUNDING_LOCK_TIMEOUT_SECS)
}

pub fn mesh_liquidity_copilot_enabled() -> bool {
    env::var("MESH_LIQUIDITY_COPILOT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn liquidity_copilot_low_watermark_shannons() -> u64 {
    env::var("MESH_LIQUIDITY_LOW_WATERMARK_SHANNONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_LOW_WATERMARK_SHANNONS)
}

pub fn liquidity_copilot_depletion_horizon_secs() -> f64 {
    env::var("MESH_LIQUIDITY_DEPLETION_HORIZON_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_DEPLETION_HORIZON_SECS) as f64
}

pub fn liquidity_copilot_cooldown_secs() -> u64 {
    env::var("MESH_LIQUIDITY_COPILOT_COOLDOWN_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LIQUIDITY_COPILOT_COOLDOWN_SECS)
}

pub fn parse_simulation_edge_nodes() -> u16 {
    env::var("MESH_SIMULATION_EDGE_NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| (1..=RING_SIZE).contains(&n))
        .unwrap_or(RING_SIZE)
}

pub fn simulation_grid_dim(edge_nodes: u16) -> u16 {
    (edge_nodes as f64).sqrt().ceil() as u16
}

pub fn simulation_fleet_hint(edge_nodes: u16) -> String {
    if edge_nodes >= RING_SIZE {
        "fnn-testnet/spawn-mesh-fleet.ps1".to_string()
    } else {
        format!("fnn-testnet/spawn-mesh-fleet.ps1 -To {edge_nodes}")
    }
}

pub fn mesh_sim_payments_enabled() -> bool {
    env::var("MESH_ALLOW_SIM_PAYMENTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(cfg!(debug_assertions))
}

/// Determines whether simulation modes or development keys are authorized.
/// Guarded strictly at the compiler level to eliminate accidental configuration leaks.
pub fn dev_keys_allowed() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("FIBER_AGENT_ALLOW_DEV_KEYS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
    #[cfg(not(debug_assertions))]
    {
        false // Hard compile-time safety constraint for pilot phase
    }
}

pub fn cors_strict_localhost() -> bool {
    env::var("MFA_CORS_STRICT_LOCALHOST")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(!cfg!(debug_assertions))
}

pub fn resolve_mfa_api_token() -> Result<String, String> {
    if let Ok(token) = env::var("MFA_API_TOKEN") {
        if token.len() < 16 {
            return Err("MFA_API_TOKEN must be at least 16 characters".to_string());
        }
        return Ok(token);
    }
    if cfg!(debug_assertions) {
        eprintln!("⚠️ [SECURITY] MFA_API_TOKEN unset — using dev default (debug build only)");
        Ok(DEFAULT_MFA_API_TOKEN.to_string())
    } else {
        Err("MFA_API_TOKEN is required in release builds".to_string())
    }
}

pub fn resolve_agent_ws_token() -> Result<String, String> {
    if let Ok(token) = env::var("MFA_AGENT_WS_TOKEN") {
        if token.len() < 16 && !cfg!(debug_assertions) {
            return Err("MFA_AGENT_WS_TOKEN must be at least 16 characters in release".to_string());
        }
        return Ok(token);
    }
    if cfg!(debug_assertions) {
        eprintln!("⚠️ [SECURITY] MFA_AGENT_WS_TOKEN unset — using dev default (debug build only)");
        Ok(DEFAULT_AGENT_WS_TOKEN.to_string())
    } else {
        Err("MFA_AGENT_WS_TOKEN is required in release builds".to_string())
    }
}

/// Extra WebSocket Origin values (exact match). Comma-separated via `MFA_WS_ALLOWED_ORIGINS`.
pub fn load_ws_allowed_origins() -> Vec<String> {
    let mut origins = vec![
        "http://127.0.0.1:8088".to_string(),
        "http://localhost:8088".to_string(),
        "http://[::1]:8088".to_string(),
        "http://127.0.0.1:5173".to_string(),
        "http://localhost:5173".to_string(),
        "http://[::1]:5173".to_string(),
    ];
    if let Ok(raw) = env::var("MFA_WS_ALLOWED_ORIGINS") {
        for origin in raw.split(',').map(str::trim).filter(|o| !o.is_empty()) {
            if !origins.iter().any(|existing| existing == origin) {
                origins.push(origin.to_string());
            }
        }
    }
    origins
}

/// Enforces a strict 64-kilobyte ceiling across telemetry payload parsing vectors.
pub fn apply_ingress_size_boundaries(router: Router) -> Router {
    router.layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
}

pub fn setup_prometheus_metrics_provider() -> (Router, PrometheusHandle) {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to initialize Prometheus data pipeline");

    let upkeep = handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            upkeep.run_upkeep();
        }
    });

    let scrape_handle = handle.clone();
    let router = Router::new().route(
        "/metrics",
        get(move || {
            let scrape_handle = scrape_handle.clone();
            async move { scrape_handle.render() }
        }),
    );

    (router, handle)
}

pub fn mesh_mtls_enabled() -> bool {
    env::var("MFA_MTLS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn telco_clearing_api_url() -> String {
    env::var("MFA_TELCO_CLEARING_API_URL").unwrap_or_default()
}

pub fn verify_clearinghouse_environmental_safety() -> Result<(), String> {
    let api_url = env::var("MFA_TELCO_CLEARING_API_URL").unwrap_or_default();
    let mock_explicit = env::var("MFA_TELCO_CLEARING_MOCK").is_ok();
    let mock_allowed = mock_explicit || cfg!(debug_assertions);

    if api_url.is_empty() && !mock_allowed {
        return Err(
            "🚨 [FATAL CONFIG] Production clearing disabled: MFA_TELCO_CLEARING_API_URL must be defined."
                .to_string(),
        );
    }
    Ok(())
}

/// When `MFA_TELCO_CLEARING_API_URL` is unset, treat telco payout as success in debug builds
/// (or when `MFA_TELCO_CLEARING_MOCK=1`) so local float-crisis clearing can complete.
pub fn telco_clearing_mock_when_unset() -> bool {
    env::var("MFA_TELCO_CLEARING_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(cfg!(debug_assertions))
}

/// Fiat → shannons scalar for a corridor ISO (override via `MFA_{ISO}_SHANNONS_RATE` or `MFA_FIAT_SHANNONS_RATE`).
pub fn fiat_shannons_exchange_rate(iso: &str) -> f64 {
    let per_iso = format!("MFA_{iso}_SHANNONS_RATE");
    env::var(&per_iso)
        .ok()
        .or_else(|| env::var("MFA_FIAT_SHANNONS_RATE").ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(38.0)
}

pub fn fiat_to_shannons(iso: &str, fiat_amount: f64) -> u64 {
    (fiat_amount * fiat_shannons_exchange_rate(iso))
        .max(0.0)
        .round() as u64
}

pub fn sovereign_levy_rate() -> f64 {
    env::var("MFA_SOVEREIGN_LEVY_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.001)
}

pub fn papss_macro_rebalance_threshold_fiat() -> f64 {
    env::var("MFA_PAPSS_MACRO_REBALANCE_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000.0)
}

use mesh_core::currency::{AssetRegistryHub, CurrencyAssetConfig, SpotMarketRate};
use mesh_core::types::FiatProvider;
use uuid::Uuid;

pub fn fiat_provider_corridor_iso(provider: &FiatProvider) -> (&'static str, &'static str) {
    match provider {
        FiatProvider::Mpesa => ("KES", "TZS"),
        FiatProvider::AirtelMoney | FiatProvider::MtnMoney => ("TZS", "KES"),
    }
}

pub async fn bootstrap_asset_registry() -> AssetRegistryHub {
    let registry = AssetRegistryHub::new();

    let tzs_cap = env::var("MFA_TZS_MACRO_VELOCITY_CAP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000_000.0);
    let kes_cap = env::var("MFA_KES_MACRO_VELOCITY_CAP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80_000_000.0);

    registry
        .introduce_currency_asset(CurrencyAssetConfig {
            iso_code: "TZS".to_string(),
            country_name: "Tanzania".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0xabc".to_string(),
            udt_args: "0x01".to_string(),
            macro_velocity_limit_24h: tzs_cap,
        })
        .await;
    registry
        .introduce_currency_asset(CurrencyAssetConfig {
            iso_code: "KES".to_string(),
            country_name: "Kenya".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0xdef".to_string(),
            udt_args: "0x02".to_string(),
            macro_velocity_limit_24h: kes_cap,
        })
        .await;

    let tzs_kes_rate = env::var("MFA_TZS_KES_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.05);

    registry
        .apply_spot_market_rate(SpotMarketRate {
            pair_id: Uuid::new_v4(),
            base_currency: "TZS".to_string(),
            quote_currency: "KES".to_string(),
            exchange_rate: tzs_kes_rate,
            last_oracle_update: chrono::Utc::now().timestamp() as u64,
            regulatory_spread_markup: 0.001,
        })
        .await;

    registry
}

pub fn try_init_papss_gateway() -> Option<crate::papss::PapssIntegrationGateway> {
    let endpoint = env::var("MFA_PAPSS_API_ENDPOINT").ok()?;
    let participant_id =
        env::var("MFA_PAPSS_PARTICIPANT_ID").unwrap_or_else(|_| "PAPSS-FSP-HUB".to_string());
    let ca_path = env::var("MFA_PAPSS_CA_PEM")
        .or_else(|_| env::var("MFA_MTLS_CA_CERT"))
        .unwrap_or_else(|_| "certs/ca.crt".to_string());
    let pem = std::fs::read(&ca_path).ok()?;
    crate::papss::PapssIntegrationGateway::new(&endpoint, &participant_id, &pem).ok()
}

fn mtls_cert_paths() -> (String, String, String) {
    let cert = env::var("MFA_MTLS_SUPERVISOR_CERT")
        .unwrap_or_else(|_| "certs/supervisor.crt".to_string());
    let key = env::var("MFA_MTLS_SUPERVISOR_KEY")
        .unwrap_or_else(|_| "certs/supervisor.key".to_string());
    let ca = env::var("MFA_MTLS_CA_CERT").unwrap_or_else(|_| "certs/ca.crt".to_string());
    (cert, key, ca)
}

fn load_pem_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    Ok(rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?)
}

fn load_pem_private_key(path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| format!("private key missing in {path}").into())
}

fn load_client_ca_store(ca_path: &str) -> Result<RootCertStore, Box<dyn std::error::Error>> {
    let file = File::open(ca_path)?;
    let mut reader = BufReader::new(file);
    let mut root_store = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut reader) {
        root_store.add(cert?)?;
    }
    Ok(root_store)
}

/// Starts an mTLS supervisor listener that requires authenticated sidecar client certificates.
pub async fn spawn_mtls_server(
    app: Router,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let (cert_path, key_path, ca_path) = mtls_cert_paths();
    let certs = load_pem_certs(&cert_path)?;
    let key = load_pem_private_key(&key_path)?;
    let root_store = load_client_ca_store(&ca_path)?;

    let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err.to_string()))?;

    let server_config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(certs, key)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err.to_string()))?;

    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tokio::spawn(async move {
        loop {
            let Ok((stream, _remote_addr)) = listener.accept().await else {
                continue;
            };
            let acceptor = tls_acceptor.clone();
            let router = app.clone();
            tokio::spawn(async move {
                let Ok(tls_stream) = acceptor.accept(stream).await else {
                    eprintln!("⚠️ [MFA mTLS] TLS handshake rejected");
                    return;
                };
                let io = hyper_util::rt::TokioIo::new(tls_stream);
                let hyper_service = TowerToHyperService::new(
                    router
                        .into_service()
                        .map_request(|req: Request<Incoming>| req.map(Body::new)),
                );
                if let Err(err) = hyper_util::server::conn::auto::Builder::new(
                    hyper_util::rt::TokioExecutor::new(),
                )
                .serve_connection_with_upgrades(io, hyper_service)
                .await
                {
                    eprintln!("⚠️ [MFA mTLS] connection error: {err}");
                }
            });
        }
    });

    Ok(())
}
