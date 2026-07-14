//! MFA control-plane WebSocket auth helpers (HMAC headers, no query tokens).

use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const AGENT_AUTH_HEADER: &str = "X-MFA-Agent-Auth";
pub const AGENT_ID_HEADER: &str = "X-Agent-ID";
pub const AGENT_TIMESTAMP_HEADER: &str = "X-MFA-Timestamp";

/// Live testnet MFA host (TLS). Sidecars/mobile default here to avoid cleartext blocks.
pub const DEFAULT_MFA_HOST: &str = "mfa.fsprotocol.com";

/// Apply production MFA endpoint defaults when env vars are unset.
/// Safe for Android/iOS (no cleartext HTTP to the control plane).
pub fn apply_secure_mfa_env_defaults() {
    if std::env::var("MFA_HOST").is_err() {
        std::env::set_var("MFA_HOST", DEFAULT_MFA_HOST);
    }
    if std::env::var("MFA_WS_SECURE").is_err() {
        std::env::set_var("MFA_WS_SECURE", "true");
    }
}

fn mfa_ws_secure_enabled() -> bool {
    match std::env::var("MFA_WS_SECURE") {
        Ok(raw) => {
            let v = raw.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        }
        // Default secure so unschemed hosts use https/wss (Android cleartext policy).
        Err(_) => true,
    }
}

/// Builds the HMAC-SHA256 hex token MFA verifies in `X-MFA-Agent-Auth`.
pub fn sign_agent_handshake_token(
    agent_id: u16,
    timestamp_secs: u64,
    secret: &str,
) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|err| format!("HMAC key rejected: {err}"))?;
    let message = format!("{agent_id}:{timestamp_secs}");
    mac.update(message.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn unix_now_secs() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|err| format!("system clock error: {err}"))
}

fn insert_header(headers: &mut HeaderMap, name: &str, value: &str) -> Result<(), String> {
    let header_name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|err| format!("invalid header name {name}: {err}"))?;
    let header_value = HeaderValue::from_str(value)
        .map_err(|err| format!("invalid header value for {name}: {err}"))?;
    headers.insert(header_name, header_value);
    Ok(())
}

/// Injects MFA agent handshake headers into a WebSocket upgrade request.
pub fn inject_agent_ws_auth_headers(
    headers: &mut HeaderMap,
    agent_id: u16,
    secret: &str,
) -> Result<(), String> {
    if secret.len() < 16 && !cfg!(debug_assertions) {
        return Err("MFA_AGENT_WS_TOKEN must be at least 16 characters".into());
    }
    let timestamp_secs = unix_now_secs()?;
    let signature = sign_agent_handshake_token(agent_id, timestamp_secs, secret)?;
    insert_header(headers, AGENT_ID_HEADER, &agent_id.to_string())?;
    insert_header(headers, AGENT_TIMESTAMP_HEADER, &timestamp_secs.to_string())?;
    insert_header(headers, AGENT_AUTH_HEADER, &signature)?;
    Ok(())
}

/// Resolve `ws://` / `wss://` control URL without embedding secrets in the query string.
pub fn mfa_control_ws_url(agent_id: u16, mfa_host: &str) -> String {
    let (scheme, rest) = resolve_mfa_host_parts(mfa_host);
    let ws_scheme = if scheme == "https" { "wss" } else { "ws" };
    format!("{ws_scheme}://{rest}/ws/{agent_id}")
}

/// HTTP(S) origin for MFA REST calls (`/telemetry`, `/clearing/*`, …).
pub fn mfa_http_base(mfa_host: &str) -> String {
    let (scheme, rest) = resolve_mfa_host_parts(mfa_host);
    format!("{scheme}://{rest}")
}

fn resolve_mfa_host_parts(mfa_host: &str) -> (&'static str, &str) {
    let host = mfa_host.trim().trim_end_matches('/');
    if let Some(rest) = host.strip_prefix("wss://") {
        ("https", rest)
    } else if let Some(rest) = host.strip_prefix("ws://") {
        ("http", rest)
    } else if let Some(rest) = host.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = host.strip_prefix("http://") {
        ("http", rest)
    } else {
        let secure = mfa_ws_secure_enabled();
        (if secure { "https" } else { "http" }, host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_agent_handshake_is_deterministic() {
        let a = sign_agent_handshake_token(1, 1_700_000_000, "sixteen-byte-secret!!").unwrap();
        let b = sign_agent_handshake_token(1, 1_700_000_000, "sixteen-byte-secret!!").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn control_ws_url_never_embeds_token() {
        let url = mfa_control_ws_url(44, "http://167.99.150.153");
        assert_eq!(url, "ws://167.99.150.153/ws/44");
        assert!(!url.contains("token="));
    }

    #[test]
    fn control_ws_url_defaults_unschemed_host_to_wss() {
        std::env::set_var("MFA_WS_SECURE", "true");
        assert_eq!(
            mfa_control_ws_url(1, "mfa.fsprotocol.com"),
            "wss://mfa.fsprotocol.com/ws/1"
        );
    }

    #[test]
    fn control_ws_url_upgrades_https_host_to_wss() {
        assert_eq!(
            mfa_control_ws_url(1, "https://mfa.example.com"),
            "wss://mfa.example.com/ws/1"
        );
    }

    #[test]
    fn inject_headers_sets_required_names() {
        let mut headers = HeaderMap::new();
        inject_agent_ws_auth_headers(&mut headers, 7, "sixteen-byte-secret!!").unwrap();
        assert!(headers.get(AGENT_AUTH_HEADER).is_some());
        assert_eq!(
            headers.get(AGENT_ID_HEADER).and_then(|v| v.to_str().ok()),
            Some("7")
        );
        assert!(headers.get(AGENT_TIMESTAMP_HEADER).is_some());
    }
}
