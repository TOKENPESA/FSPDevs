use std::env;

use async_trait::async_trait;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde_json::Value;

/// Loads `FNN_BISCUIT_TOKEN` for Fiber RPC Bearer auth (required in production custody).
pub fn resolve_fnn_biscuit_token() -> Option<String> {
    match env::var("FNN_BISCUIT_TOKEN") {
        Ok(token) => {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    }
}

/// Unified enterprise Fiber node JSON-RPC abstraction for MFA treasury operations.
#[async_trait]
pub trait FiberNodeRpc: Send + Sync {
    /// Dispatches a raw JSON-RPC payload to the underlying Fiber node.
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String>;
}

/// HTTP JSON-RPC client for the MFA corporate treasury FNN endpoint.
pub struct EnterpriseFnnClient {
    rpc_url: String,
    client: reqwest::Client,
    biscuit_token: Option<String>,
}

impl EnterpriseFnnClient {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self::with_biscuit_token(rpc_url, resolve_fnn_biscuit_token())
    }

    pub fn with_biscuit_token(
        rpc_url: impl Into<String>,
        biscuit_token: Option<String>,
    ) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            client: reqwest::Client::new(),
            biscuit_token,
        }
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.biscuit_token.as_deref() {
            Some(token) if HeaderValue::from_str(&format!("Bearer {token}")).is_ok() => {
                request.header(AUTHORIZATION, format!("Bearer {token}"))
            }
            _ => request,
        }
    }
}

#[async_trait]
impl FiberNodeRpc for EnterpriseFnnClient {
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        let response = self
            .authorize(self.client.post(&self.rpc_url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("FNN Network Timeout: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "FNN Node rejected connection: HTTP {}",
                response.status()
            ));
        }

        response
            .json::<Value>()
            .await
            .map_err(|e| format!("FNN JSON parse error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_fnn_client_stores_rpc_url() {
        let client = EnterpriseFnnClient::with_biscuit_token(
            "http://127.0.0.1:8227",
            Some("test-biscuit".into()),
        );
        assert_eq!(client.rpc_url, "http://127.0.0.1:8227");
        assert_eq!(client.biscuit_token.as_deref(), Some("test-biscuit"));
    }
}
