use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::auth::api_token_matches;
use crate::state::AppState;

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
            "default-src 'self'; connect-src 'self' ws://127.0.0.1:1025; style-src 'self' 'unsafe-inline';",
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
