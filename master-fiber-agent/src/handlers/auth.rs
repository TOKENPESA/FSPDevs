//! Public sidecar onboarding — cryptographic agent identity issuance.

use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use mesh_core::RING_SIZE;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct RegistrationResponse {
    /// Display / env form used by sidecars (`FA-42`).
    pub agent_id: String,
    /// Numeric mesh id for `/ws/{agent_id}` and telemetry.
    pub agent_id_numeric: u16,
    /// 32-byte HMAC secret (64 hex chars) for `X-MFA-Agent-Auth` handshakes.
    pub agent_secret: String,
}

/// Public (unauthenticated) registration routes — mount behind ingress rate limiting.
pub fn auth_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/register", post(register_new_agent))
}

fn generate_agent_secret() -> Result<String, StatusCode> {
    let mut secret_bytes = [0u8; 32];
    getrandom::getrandom(&mut secret_bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(hex::encode(secret_bytes))
}

async fn register_new_agent(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let agent_secret = match generate_agent_secret() {
        Ok(secret) => secret,
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({
                    "error": "failed to generate agent secret"
                })),
            )
                .into_response();
        }
    };

    let min_agent_id = std::env::var("MFA_REGISTER_MIN_AGENT_ID")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(2)
        .clamp(1, RING_SIZE);
    // Full mesh range — do not clamp to simulation slider (that only affects UI pathfinding).
    let max_agent_id = RING_SIZE;

    match state
        .module_store
        .register_agent(min_agent_id, max_agent_id, &agent_secret)
    {
        Ok(record) => {
            println!(
                "🪪 [MFA REGISTER] Issued identity FA-{} (persisted)",
                record.agent_id
            );
            (
                StatusCode::CREATED,
                Json(RegistrationResponse {
                    agent_id: format!("FA-{}", record.agent_id),
                    agent_id_numeric: record.agent_id,
                    agent_secret: record.agent_secret,
                }),
            )
                .into_response()
        }
        Err(err) => {
            let status = if err.contains("capacity exhausted") {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            eprintln!("⚠️ [MFA REGISTER] Failed to issue agent identity: {err}");
            (
                status,
                Json(serde_json::json!({
                    "error": err
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_agent_secret;

    #[test]
    fn generate_agent_secret_is_64_hex_chars() {
        let secret = generate_agent_secret().expect("entropy");
        assert_eq!(secret.len(), 64);
        assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
