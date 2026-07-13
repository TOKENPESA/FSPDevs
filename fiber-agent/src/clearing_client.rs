//! Sidecar → MFA regional clearinghouse float-crisis intake.

use mesh_core::types::FloatExhaustionTelemetry;
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde_json::Value;

pub const DEFAULT_MFA_API_TOKEN: &str = "fspdevs-local-api-devonly";

pub fn resolve_mfa_api_token() -> Option<String> {
    std::env::var("MFA_API_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .or_else(|| {
            if cfg!(debug_assertions) {
                Some(DEFAULT_MFA_API_TOKEN.to_string())
            } else {
                None
            }
        })
}

pub fn mfa_auth_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(token) = resolve_mfa_api_token() {
        if let Ok(value) = format!("Bearer {token}").parse() {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}

pub fn resolve_mfa_host() -> String {
    std::env::var("MFA_HOST").unwrap_or_else(|_| "127.0.0.1:1025".to_string())
}

pub fn normalize_mfa_host(host: &str) -> String {
    host.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

pub fn mfa_health_url(mfa_host: Option<&str>) -> String {
    let host = mfa_host
        .map(normalize_mfa_host)
        .unwrap_or_else(resolve_mfa_host);
    format!("http://{host}/")
}

pub fn mfa_control_ws_url(agent_id: u16, mfa_host: Option<&str>) -> String {
    let host = mfa_host
        .map(normalize_mfa_host)
        .unwrap_or_else(resolve_mfa_host);
    let ws_token =
        std::env::var("MFA_AGENT_WS_TOKEN").unwrap_or_else(|_| "fspdevs-local-ws".into());
    format!("ws://{host}/ws/{agent_id}?token={ws_token}")
}

pub fn format_mfa_service_name(raw: &str) -> String {
    match raw {
        "master_fiber_agent" => "Master Fiber Agent".to_string(),
        other => other
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub async fn probe_mfa_health(mfa_host: Option<&str>) -> Result<Value, String> {
    let url = mfa_health_url(mfa_host);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|err| format!("MFA health client error: {err}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("MFA health probe failed: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("MFA health HTTP {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|err| format!("MFA health decode failed: {err}"))
}

pub fn mfa_clearing_url(mfa_host: Option<&str>) -> String {
    let host = mfa_host
        .map(normalize_mfa_host)
        .or_else(|| std::env::var("MFA_HOST").ok().map(|value| normalize_mfa_host(&value)))
        .unwrap_or_else(resolve_mfa_host);
    format!("http://{host}/clearing/float-crisis")
}

pub async fn post_float_crisis_to_mfa(
    client: &reqwest::Client,
    telemetry: &FloatExhaustionTelemetry,
    mfa_host: Option<&str>,
) -> Result<Value, String> {
    let url = mfa_clearing_url(mfa_host);
    println!(
        "📡 [CLEARING DISPATCH] POST float-crisis telemetry for FA-{} → {url}",
        telemetry.agent_id
    );

    let response = client
        .post(&url)
        .headers(mfa_auth_headers())
        .json(telemetry)
        .send()
        .await
        .map_err(|err| format!("MFA clearing POST transport error: {err}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .unwrap_or_else(|_| serde_json::json!({ "status": "UNPARSEABLE_RESPONSE" }));

    if status.is_success() {
        println!("✅ [CLEARING DISPATCH] MFA accepted intake (HTTP {status})");
        Ok(body)
    } else {
        Err(format!(
            "MFA clearing rejected HTTP {status}: {}",
            body.get("reason")
                .or(body.get("status"))
                .map(|v| v.to_string())
                .unwrap_or_else(|| body.to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::types::FiatProvider;

    #[test]
    fn mfa_control_ws_url_uses_agent_route() {
        std::env::set_var("MFA_HOST", "127.0.0.1:1025");
        std::env::set_var("MFA_AGENT_WS_TOKEN", "test-token");
        assert_eq!(
            mfa_control_ws_url(7, None),
            "ws://127.0.0.1:1025/ws/7?token=test-token"
        );
    }

    #[test]
    fn mfa_clearing_url_normalizes_host() {
        std::env::remove_var("MFA_HOST");
        assert_eq!(
            mfa_clearing_url(Some("http://127.0.0.1:1025")),
            "http://127.0.0.1:1025/clearing/float-crisis"
        );
    }

    #[test]
    fn float_crisis_payload_serializes_for_mfa() {
        let telemetry = FloatExhaustionTelemetry {
            agent_id: 44,
            provider: FiatProvider::Mpesa,
            current_fiat_balance: 40_000.0,
            critical_fiat_floor: 50_000.0,
            digital_l2_balance_shannons: 6_200_000,
            drain_velocity_per_sec: 450.0,
        };
        let json = serde_json::to_value(&telemetry).expect("serialize");
        assert_eq!(json["agent_id"], 44);
    }
}
