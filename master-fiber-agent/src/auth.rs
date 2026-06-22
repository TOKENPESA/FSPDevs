use axum::http::{header, HeaderMap, StatusCode};

/// Allowed Origin values for agent and monitor WebSocket upgrades (CSWSH defense).
/// Loopback and private LAN hosts on any port (local dev; `serve` may pick a port if 8088 is busy).
pub fn is_allowed_ws_origin(origin_str: &str) -> bool {
    let Some(rest) = origin_str.strip_prefix("http://") else {
        return false;
    };
    let Some((host, _port)) = rest.rsplit_once(':') else {
        return false;
    };

    match host {
        "127.0.0.1" | "localhost" | "[::1]" => true,
        h if h.starts_with("192.168.") || h.starts_with("10.") => true,
        h if h.starts_with("172.") => {
            if let Some(second) = h.split('.').nth(1).and_then(|s| s.parse::<u8>().ok()) {
                (16..=31).contains(&second)
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn validate_agent_ws_token(query_token: Option<&str>, expected: &str) -> bool {
    query_token.is_some_and(|t| t == expected)
}

/// Reject cross-origin WebSocket handshakes from untrusted browser origins.
pub fn validate_ws_origin(headers: &HeaderMap, allowed_origins: &[String]) -> Result<(), StatusCode> {
    let Some(origin_header) = headers.get(header::ORIGIN) else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let origin_str = origin_header
        .to_str()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let matches = allowed_origins.iter().any(|allowed| allowed == origin_str)
        || is_allowed_ws_origin(origin_str);

    if !matches {
        eprintln!(
            "❌ [SECURITY FAULT] Blocked unauthorized connection from Origin: {origin_str}"
        );
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_ws_origin_allowlist() {
        assert!(is_allowed_ws_origin("http://127.0.0.1:8088"));
        assert!(is_allowed_ws_origin("http://localhost:60354"));
        assert!(is_allowed_ws_origin("http://[::1]:8088"));
        assert!(is_allowed_ws_origin("http://192.168.56.1:8088"));
        assert!(is_allowed_ws_origin("http://192.168.56.1:60354"));
        assert!(!is_allowed_ws_origin("http://evil.example"));
        assert!(is_allowed_ws_origin("http://127.0.0.1:9999"));
    }

    #[test]
    fn test_validate_ws_origin_requires_origin_header() {
        let headers = HeaderMap::new();
        let allowed = vec!["http://127.0.0.1:8088".to_string()];
        assert_eq!(
            validate_ws_origin(&headers, &allowed),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn test_validate_ws_origin_accepts_exact_allowlist_match() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:8088"),
        );
        let allowed = vec!["http://127.0.0.1:8088".to_string()];
        assert!(validate_ws_origin(&headers, &allowed).is_ok());
    }

    #[test]
    fn test_validate_ws_origin_accepts_private_lan_pattern() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://192.168.1.50:60354"),
        );
        assert!(validate_ws_origin(&headers, &[]).is_ok());
    }

    #[test]
    fn test_validate_ws_origin_blocks_untrusted_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example:8088"),
        );
        assert_eq!(
            validate_ws_origin(&headers, &[]),
            Err(StatusCode::FORBIDDEN)
        );
    }
}
