use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::http::{header, HeaderMap, Request, StatusCode};
use hmac::{Hmac, Mac};
use mesh_core::{normalize_pubkey_hex, valid_agent_id, MeshPubkeyRegistry, peer_id_from_agent_pubkey};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub const AGENT_AUTH_HEADER: &str = "X-MFA-Agent-Auth";
pub const AGENT_ID_HEADER: &str = "X-Agent-ID";
pub const AGENT_TIMESTAMP_HEADER: &str = "X-MFA-Timestamp";
const AGENT_HANDSHAKE_MAX_SKEW_SECS: u64 = 300;

/// Allowed Origin values for agent and monitor WebSocket upgrades (CSWSH defense).
/// Loopback hosts are always permitted; additional hosts require `MFA_WS_ALLOWED_ORIGINS`.
pub fn is_allowed_ws_origin(origin_str: &str) -> bool {
    let Some(rest) = origin_str.strip_prefix("http://") else {
        return false;
    };
    let (host, _) = rest.rsplit_once(':').unwrap_or((rest, ""));

    let dev_mode = std::env::var("MFA_DEV_MODE").unwrap_or_default() == "true";
    if dev_mode && (host == "127.0.0.1" || host == "localhost" || host == "[::1]") {
        return true;
    }

    // Explicit configurations only; no fallback LAN tracking allowed
    if let Ok(allowed_env) = std::env::var("MFA_WS_ALLOWED_ORIGINS") {
        return allowed_env.split(',').any(|configured| configured.trim() == host);
    }

    false
}

/// Cryptographically secure, constant-time string comparison.
/// Prevents timing attacks against MFA API tokens and WS auth strings.
pub fn secure_compare(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

pub fn api_token_matches(
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    expected: &str,
) -> bool {
    if expected.is_empty() {
        return false;
    }

    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(raw) = value.to_str() {
            if let Some(token) = raw.strip_prefix("Bearer ") {
                if secure_compare(token.trim(), expected) {
                    return true;
                }
            }
        }
    }

    if let Some(value) = headers.get("X-MFA-API-Token") {
        if let Ok(raw) = value.to_str() {
            if secure_compare(raw.trim(), expected) {
                return true;
            }
        }
    }

    if let Some(query) = uri.query() {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if key == "token" && secure_compare(value, expected) {
                    return true;
                }
            }
        }
    }

    false
}

pub fn validate_agent_ws_token(query_token: Option<&str>, expected: &str) -> bool {
    if expected.len() < 16 && !cfg!(debug_assertions) {
        return false;
    }
    query_token.is_some_and(|token| secure_compare(token, expected))
}

pub fn agent_handshake_headers_present(headers: &HeaderMap) -> bool {
    headers.get(AGENT_AUTH_HEADER).is_some()
        && headers.get(AGENT_ID_HEADER).is_some()
        && headers.get(AGENT_TIMESTAMP_HEADER).is_some()
}

/// Builds the HMAC hex token sidecars send in `X-MFA-Agent-Auth`.
pub fn sign_agent_handshake_token(
    agent_id: u16,
    timestamp_secs: u64,
    secret: &str,
) -> Result<String, StatusCode> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let message = format!("{agent_id}:{timestamp_secs}");
    mac.update(message.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn verify_timestamp_fresh(timestamp_str: &str) -> Result<(), StatusCode> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs();
    let client_time: u64 = timestamp_str
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if (current_time as i64 - client_time as i64).unsigned_abs() > AGENT_HANDSHAKE_MAX_SKEW_SECS {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Verifies signed agent handshake headers and returns the claimed agent id.
pub fn verify_agent_handshake_headers(
    headers: &HeaderMap,
    secret: &str,
) -> Result<u16, StatusCode> {
    let token_header = headers
        .get(AGENT_AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let agent_id_str = headers
        .get(AGENT_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let timestamp_str = headers
        .get(AGENT_TIMESTAMP_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    verify_timestamp_fresh(timestamp_str)?;

    let agent_id: u16 = agent_id_str
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if !valid_agent_id(agent_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let expected_signature = sign_agent_handshake_token(agent_id, {
        timestamp_str
            .parse()
            .map_err(|_| StatusCode::BAD_REQUEST)?
    }, secret)?;

    if secure_compare(token_header, &expected_signature) {
        Ok(agent_id)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// HMAC upgrade auth for HTTP/WebSocket handshakes (header-based).
pub fn verify_agent_handshake_token<B>(
    request: &Request<B>,
    secret: &str,
) -> Result<u16, StatusCode> {
    verify_agent_handshake_headers(request.headers(), secret)
}

/// Accepts either HMAC headers or the legacy `?token=` query parameter.
pub fn validate_agent_ws_connection(
    headers: &HeaderMap,
    query_token: Option<&str>,
    path_agent_id: u16,
    expected_secret: &str,
) -> bool {
    if agent_handshake_headers_present(headers) {
        return matches!(
            verify_agent_handshake_headers(headers, expected_secret),
            Ok(claimed) if claimed == path_agent_id
        );
    }
    validate_agent_ws_token(query_token, expected_secret)
}

/// Ensures signed telemetry pubkey is authorized for the claimed agent id.
pub fn verify_telemetry_agent_binding(
    agent_id: u16,
    pubkey_hex: &str,
    registry: &MeshPubkeyRegistry,
) -> Result<(), &'static str> {
    let claimed = normalize_pubkey_hex(pubkey_hex);

    if let Some(reg_pk) = registry.get(agent_id) {
        if normalize_pubkey_hex(reg_pk) == claimed {
            return Ok(());
        }
        return Err("public_key_hex does not match mesh registry for agent_id");
    }

    if peer_id_from_agent_pubkey(pubkey_hex) == Some(agent_id) {
        if crate::config::dev_keys_allowed() {
            return Ok(());
        }
        return Err("dev-derived pubkey rejected without registry entry in production");
    }

    Err("public_key_hex not authorized for agent_id")
}

/// HTTP / WebSocket Origin gate — exact dashboard allowlist plus dev loopback policy.
pub fn is_allowed_cors_origin(origin_str: &str, allowed_origins: &[String]) -> bool {
    allowed_origins.iter().any(|allowed| allowed == origin_str)
        || is_allowed_ws_origin(origin_str)
}

pub fn check_websocket_handshake_origin(
    headers: &HeaderMap,
    allowed_origins: &[String],
) -> Result<(), StatusCode> {
    if let Some(origin_header) = headers.get(header::ORIGIN) {
        let origin_str = origin_header
            .to_str()
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        let is_valid = is_allowed_cors_origin(origin_str, allowed_origins);
        if !is_valid {
            eprintln!(
                "❌ [SECURITY ALERT] CSWSH attempt blocked from origin domain: {origin_str}"
            );
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

pub struct EphemeralTicketRegistry {
    tickets: RwLock<HashMap<Uuid, Instant>>,
    ttl: Duration,
}

/// Browser EventSource compliance stream tickets expire after 30 seconds.
pub const EPHEMERAL_TICKET_TTL_SECS: u64 = 30;

impl EphemeralTicketRegistry {
    pub fn new(ttl_secs: u64) -> Arc<Self> {
        let registry = Arc::new(Self {
            tickets: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        });

        let gc_target = Arc::clone(&registry);
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    let mut guard = gc_target.tickets.write().await;
                    let now = Instant::now();
                    guard.retain(|_, expiry| *expiry > now);
                }
            });
        }

        registry
    }

    pub async fn issue_ticket(&self) -> Uuid {
        let ticket = Uuid::new_v4();
        self.tickets.write().await.insert(ticket, Instant::now() + self.ttl);
        ticket
    }

    pub async fn validate_and_burn(&self, ticket: Uuid) -> bool {
        let mut guard = self.tickets.write().await;
        let now = Instant::now();
        guard.retain(|_, expiry| *expiry > now); // Lazy cleanup
        guard.remove(&ticket).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, HeaderValue};

    #[test]
    fn test_ws_origin_allowlist() {
        let prev_dev = std::env::var("MFA_DEV_MODE").ok();
        let prev_ws = std::env::var("MFA_WS_ALLOWED_ORIGINS").ok();
        std::env::set_var("MFA_DEV_MODE", "true");
        std::env::remove_var("MFA_WS_ALLOWED_ORIGINS");
        assert!(is_allowed_ws_origin("http://127.0.0.1:8088"));
        assert!(is_allowed_ws_origin("http://localhost:60354"));
        assert!(is_allowed_ws_origin("http://[::1]:8088"));
        assert!(!is_allowed_ws_origin("http://192.168.56.1:8088"));
        assert!(!is_allowed_ws_origin("http://evil.example"));
        assert!(is_allowed_ws_origin("http://127.0.0.1:9999"));
        std::env::remove_var("MFA_DEV_MODE");
        std::env::remove_var("MFA_WS_ALLOWED_ORIGINS");
        assert!(!is_allowed_ws_origin("http://127.0.0.1:8088"));
        if let Some(value) = prev_dev {
            std::env::set_var("MFA_DEV_MODE", value);
        }
        if let Some(value) = prev_ws {
            std::env::set_var("MFA_WS_ALLOWED_ORIGINS", value);
        }
    }

    #[test]
    fn cors_origin_accepts_dashboard_allowlist_without_dev_mode() {
        let prev_dev = std::env::var("MFA_DEV_MODE").ok();
        std::env::remove_var("MFA_DEV_MODE");
        let allowed = vec!["http://127.0.0.1:8088".to_string()];
        assert!(is_allowed_cors_origin("http://127.0.0.1:8088", &allowed));
        assert!(!is_allowed_cors_origin("http://evil.example:8088", &allowed));
        if let Some(value) = prev_dev {
            std::env::set_var("MFA_DEV_MODE", value);
        }
    }

    #[test]
    fn test_check_websocket_handshake_origin_requires_origin_header() {
        let headers = HeaderMap::new();
        let allowed = vec!["http://127.0.0.1:8088".to_string()];
        assert_eq!(
            check_websocket_handshake_origin(&headers, &allowed),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn test_check_websocket_handshake_origin_accepts_exact_allowlist_match() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:8088"),
        );
        let allowed = vec!["http://127.0.0.1:8088".to_string()];
        assert!(check_websocket_handshake_origin(&headers, &allowed).is_ok());
    }

    #[test]
    fn test_check_websocket_handshake_origin_blocks_unlisted_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example:8088"),
        );
        assert_eq!(
            check_websocket_handshake_origin(&headers, &[]),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn test_check_websocket_handshake_origin_blocks_unlisted_lan_without_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://192.168.1.50:60354"),
        );
        assert_eq!(
            check_websocket_handshake_origin(&headers, &[]),
            Err(StatusCode::FORBIDDEN)
        );
        let allowed = vec!["http://192.168.1.50:60354".to_string()];
        assert!(check_websocket_handshake_origin(&headers, &allowed).is_ok());
    }

    #[test]
    fn agent_handshake_hmac_round_trip() {
        let secret = "fspdevs-local-ws-token";
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let signature =
            sign_agent_handshake_token(44, timestamp, secret).expect("sign handshake");

        let mut headers = HeaderMap::new();
        headers.insert(AGENT_AUTH_HEADER, signature.parse().unwrap());
        headers.insert(AGENT_ID_HEADER, "44".parse().unwrap());
        headers.insert(
            AGENT_TIMESTAMP_HEADER,
            timestamp.to_string().parse().unwrap(),
        );

        let claimed = verify_agent_handshake_headers(&headers, secret).expect("verify");
        assert_eq!(claimed, 44);
    }

    #[test]
    fn agent_handshake_rejects_stale_timestamp() {
        let secret = "fspdevs-local-ws-token";
        let stale = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(AGENT_HANDSHAKE_MAX_SKEW_SECS + 60);
        let signature = sign_agent_handshake_token(44, stale, secret).expect("sign");

        let mut headers = HeaderMap::new();
        headers.insert(AGENT_AUTH_HEADER, signature.parse().unwrap());
        headers.insert(AGENT_ID_HEADER, "44".parse().unwrap());
        headers.insert(
            AGENT_TIMESTAMP_HEADER,
            stale.to_string().parse().unwrap(),
        );

        assert_eq!(
            verify_agent_handshake_headers(&headers, secret),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn validate_agent_ws_connection_accepts_legacy_query_token() {
        let headers = HeaderMap::new();
        assert!(validate_agent_ws_connection(
            &headers,
            Some("shared-secret-token"),
            44,
            "shared-secret-token",
        ));
    }

    #[test]
    fn verify_telemetry_agent_binding_accepts_registry_match() {
        use mesh_core::MeshPubkeyRegistry;
        use std::collections::HashMap;

        let pk = mesh_core::agent_fnn_pubkey(44);
        let registry = MeshPubkeyRegistry::from_map(HashMap::from([(44, pk.clone())]));
        assert!(verify_telemetry_agent_binding(44, &pk, &registry).is_ok());
    }

    #[test]
    fn verify_telemetry_agent_binding_rejects_spoofed_agent() {
        use mesh_core::MeshPubkeyRegistry;
        use std::collections::HashMap;

        let pk44 = mesh_core::agent_fnn_pubkey(44);
        let pk45 = mesh_core::agent_fnn_pubkey(45);
        let registry = MeshPubkeyRegistry::from_map(HashMap::from([(44, pk44)]));
        assert!(verify_telemetry_agent_binding(44, &pk45, &registry).is_err());
    }

    #[tokio::test]
    async fn ephemeral_ticket_single_use() {
        let registry = EphemeralTicketRegistry::new(60);
        let ticket = registry.issue_ticket().await;
        assert!(registry.validate_and_burn(ticket).await);
        assert!(!registry.validate_and_burn(ticket).await);
    }

    #[tokio::test]
    async fn ephemeral_ticket_rejects_unknown_id() {
        let registry = EphemeralTicketRegistry::new(60);
        assert!(!registry.validate_and_burn(Uuid::new_v4()).await);
    }
}
