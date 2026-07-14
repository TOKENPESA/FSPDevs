//! Bearer auth for the local Fiber Agent module management API.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

const MIN_API_TOKEN_LEN: usize = 16;
const DEBUG_DEFAULT_API_TOKEN: &str = "fspdevs-local-fa-api-devonly";

pub fn secure_compare(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

/// Resolve `FIBER_AGENT_API_TOKEN`. Release builds require a ≥16-char env value.
pub fn resolve_fiber_agent_api_token() -> Result<String, String> {
    match std::env::var("FIBER_AGENT_API_TOKEN") {
        Ok(token) => {
            let trimmed = token.trim().to_string();
            if trimmed.len() < MIN_API_TOKEN_LEN {
                return Err(format!(
                    "FIBER_AGENT_API_TOKEN must be at least {MIN_API_TOKEN_LEN} characters"
                ));
            }
            Ok(trimmed)
        }
        Err(_) if cfg!(debug_assertions) => Ok(DEBUG_DEFAULT_API_TOKEN.to_string()),
        Err(_) => Err(
            "FIBER_AGENT_API_TOKEN is required to expose /api/modules/* (min 16 chars)".into(),
        ),
    }
}

pub fn bearer_token_matches(headers: &HeaderMap, expected: &str) -> bool {
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
    false
}

pub async fn require_fa_api_auth(
    State(expected): State<Arc<String>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let uri = request.uri();
    if uri.query().is_some_and(|q| q.contains("token=")) {
        log::warn!(
            "🚨 [FA API] Rejected module API request that embeds credentials in the query string"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    if bearer_token_matches(request.headers(), expected.as_str()) {
        Ok(next.run(request).await)
    } else {
        log::warn!("🚨 [FA API] Unauthorized module API request blocked");
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn bearer_token_matches_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sixteen-byte-token"),
        );
        assert!(bearer_token_matches(&headers, "sixteen-byte-token"));
        assert!(!bearer_token_matches(&headers, "different-token-xxxx"));
    }
}
