use async_trait::async_trait;
use serde_json::Value;

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
}

impl EnterpriseFnnClient {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl FiberNodeRpc for EnterpriseFnnClient {
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        let response = self
            .client
            .post(&self.rpc_url)
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
        let client = EnterpriseFnnClient::new("http://127.0.0.1:8227");
        assert_eq!(client.rpc_url, "http://127.0.0.1:8227");
    }
}
