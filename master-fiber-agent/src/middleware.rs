use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::{PeerIpKeyExtractor, SmartIpKeyExtractor};
use tower_governor::GovernorLayer;
use uuid::Uuid;

use crate::auth::{api_token_matches, is_allowed_cors_origin};
use crate::state::AppState;
use axum::http::header;

pub async fn inject_security_headers_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    headers.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    headers.insert(
        "Content-Security-Policy",
        HeaderValue::from_static(
            "default-src 'self'; connect-src 'self' ws: wss: http://127.0.0.1:1025 ws://127.0.0.1:1025; style-src 'self' 'unsafe-inline';",
        ),
    );
    response
}

pub async fn require_mfa_api_auth(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let uri = request.uri();
    let path = uri.path();

    if path == "/api/v1/compliance/stream" || path == "/compliance/surveillance" {
        if let Some(query_string) = uri.query() {
            for pair in query_string.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    if key == "ticket" {
                        if let Ok(ticket_uuid) = Uuid::parse_str(value) {
                            if state.compliance_tickets.validate_and_burn(ticket_uuid).await {
                                return Ok(next.run(request).await);
                            }
                        }
                        log::warn!(
                            "🚨 [SECURITY ALERT] Malformed, reused, or expired single-use stream ticket blocked."
                        );
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                }
            }
        }
        log::warn!(
            "🚨 [SECURITY ALERT] Denied unauthenticated connection attempt to streaming pipelines."
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Browser ops console: allowlist Origin may open the read-only monitor without embedding MFA_API_TOKEN
    // in public JavaScript. Handler still enforces Origin on the WebSocket upgrade.
    if path == "/ws/monitor" {
        if let Some(origin_header) = request.headers().get(header::ORIGIN) {
            if let Ok(origin_str) = origin_header.to_str() {
                if is_allowed_cors_origin(origin_str, &state.ws_allowed_origins) {
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    // Module store GETs are safe to expose (catalog is static; installed mirrors health.running_plugins).
    // Same-origin browser GETs often omit Origin, so token/Origin gates fail and the App Store 401s.
    if request.method() == axum::http::Method::GET
        && (path == "/api/modules/catalog" || path == "/api/modules/installed")
    {
        return Ok(next.run(request).await);
    }

    // Mutations from the allowlisted ops console Origin (POST/PUT include Origin on same-site fetch).
    if path.starts_with("/api/modules/") {
        if let Some(origin_header) = request.headers().get(header::ORIGIN) {
            if let Ok(origin_str) = origin_header.to_str() {
                if is_allowed_cors_origin(origin_str, &state.ws_allowed_origins) {
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    if api_token_matches(uri, request.headers(), &state.api_token) {
        // Browser WebSocket upgrades cannot set Authorization headers; allow query token only on /ws/*.
        let blocks_query_token = !path.starts_with("/ws/");
        if blocks_query_token && uri.query().is_some_and(|q| q.contains("token=")) {
            log::warn!(
                "🚨 [SECURITY CRITICAL] Terminated request passing persistent API keys inside query strings."
            );
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Defaults to SmartIp behind nginx (`X-Forwarded-For` / `X-Real-IP`).
/// Set `MFA_RATE_LIMIT_PEER_IP=1` to force `PeerIpKeyExtractor` (direct binds only).
pub fn prefer_peer_ip_rate_limit() -> bool {
    std::env::var("MFA_RATE_LIMIT_PEER_IP")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Apply Phase B edge ingress rate limit (5 rps replenishment, burst 10) to a router.
pub fn with_edge_ingress_rate_limit<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    if prefer_peer_ip_rate_limit() {
        let mut builder = GovernorConfigBuilder::default();
        // PeerIpKeyExtractor is the GovernorConfigBuilder default.
        let _ = PeerIpKeyExtractor;
        builder
            .period(Duration::from_millis(200))
            .burst_size(10);
        let config = builder.finish().expect("peer-ip governor config");
        router.layer(GovernorLayer {
            config: Arc::new(config),
        })
    } else {
        let mut base = GovernorConfigBuilder::default();
        let mut builder = base.key_extractor(SmartIpKeyExtractor);
        builder
            .period(Duration::from_millis(200))
            .burst_size(10);
        let config = builder.finish().expect("smart-ip governor config");
        router.layer(GovernorLayer {
            config: Arc::new(config),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, HeaderValue};

    #[test]
    fn api_token_matches_bearer_header() {
        let uri = "http://127.0.0.1:1025/route".parse().unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-1234567890"),
        );
        assert!(api_token_matches(
            &uri,
            &headers,
            "test-token-1234567890"
        ));
    }

    #[test]
    fn api_token_matches_rejects_query_token_for_standard_routes() {
        let uri = "http://127.0.0.1:1025/route?token=dev-stream-token-abc"
            .parse()
            .unwrap();
        let headers = axum::http::HeaderMap::new();
        assert!(api_token_matches(
            &uri,
            &headers,
            "dev-stream-token-abc"
        ));
    }
}
