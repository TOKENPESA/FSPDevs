use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mesh_core::telemetry::BalanceDepletedPayload;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use fsp_fixed_math::apply_bps_spread_shannons;

use crate::fnn_client::FiberNodeRpc;

/// Default FX spread (bps) applied to treasury intent-swap refuel amounts.
const DEFAULT_REFUEL_SPREAD_BPS: u32 = 25;

/// Manages dynamic concurrency constraints to ensure a single channel isn't double-funded.
pub struct FundingLockManager {
    locks: HashMap<String, Instant>,
    lock_ttl: Duration,
}

impl FundingLockManager {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            locks: HashMap::new(),
            lock_ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn try_acquire_lock(&mut self, channel_id: &str) -> bool {
        let now = Instant::now();
        // Clear expired locks
        self.locks.retain(|_, expiry| *expiry > now);

        if self.locks.contains_key(channel_id) {
            false // A funding operation is currently in-flight for this channel
        } else {
            self.locks.insert(channel_id.to_string(), now + self.lock_ttl);
            true
        }
    }

    pub fn release_lock(&mut self, channel_id: &str) {
        self.locks.remove(channel_id);
    }
}

pub struct EnterpriseClearinghouse {
    enterprise_fnn_client: Arc<dyn FiberNodeRpc>,
    funding_locks: Arc<TokioMutex<FundingLockManager>>,
    corporate_treasury_vault_id: String,
    default_refuel_allocation_shannons: u64,
    fx_spread_bps: u32,
}

impl EnterpriseClearinghouse {
    pub fn new(
        enterprise_fnn_client: Arc<dyn FiberNodeRpc>,
        corporate_treasury_vault_id: String,
    ) -> Self {
        let fx_spread_bps = std::env::var("CLEARINGHOUSE_FX_SPREAD_BPS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(DEFAULT_REFUEL_SPREAD_BPS);
        Self {
            enterprise_fnn_client,
            funding_locks: Arc::new(TokioMutex::new(FundingLockManager::new(60))), // 60-second window
            corporate_treasury_vault_id,
            default_refuel_allocation_shannons: 10_000_000, // Default 10M Shannons injection
            fx_spread_bps,
        }
    }

    /// Integer-only refuel amount after FX spread markup (no float multiply).
    fn refuel_amount_shannons(&self) -> u64 {
        apply_bps_spread_shannons(self.default_refuel_allocation_shannons, self.fx_spread_bps)
    }

    /// Primary execution sequence for routing emergency liquidity outward from the central treasury.
    pub async fn handle_balance_depletion(&self, alert: BalanceDepletedPayload) -> Result<(), String> {
        let mut lock_guard = self.funding_locks.lock().await;

        // 1. Enforce strict concurrency barriers
        if !lock_guard.try_acquire_lock(&alert.short_channel_id) {
            log::warn!(
                "⚠️ [CLEARINGHOUSE] Funding aborted: Channel {} is already locked in an active funding cycle.",
                alert.short_channel_id
            );
            return Ok(());
        }
        drop(lock_guard); // Release state lock during heavy network async operations

        log::info!(
            "🚨 [CLEARINGHOUSE] Low liquidity detected on FA-{}. Available: {} Shannons. Target Minimum: {} Shannons. Initiating Enterprise Refuel Sequence...",
            alert.agent_id,
            alert.available_outbound_shannons,
            alert.minimum_required_shannons
        );

        // 2. Cryptographically generate the payment hash and secret preimage locally on the MFA
        let mut preimage_bytes = [0u8; 32];
        getrandom::getrandom(&mut preimage_bytes)
            .map_err(|e| format!("Cryptographic entropy failure: {e}"))?;
        let _preimage_hex = hex::encode(preimage_bytes);

        // Compute SHA-256 hash over the preimage
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(preimage_bytes);
        let payment_hash_hex = hex::encode(hasher.finalize());

        let amount_shannons = self.refuel_amount_shannons();

        // 3. Formulate the JSON-RPC payload for the MFA's own Enterprise FNN engine
        // Instead of asking the edge node to draw funds, the MFA pushes a cross-hub intent swap down the path.
        let mfa_rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "execute_cross_hub_intent_swap",
            "params": {
                "source_vault_id": self.corporate_treasury_vault_id,
                "target_node_pubkey": alert.agent_fnn_pubkey,
                "target_channel_id": alert.short_channel_id,
                "amount_shannons": amount_shannons,
                "fx_spread_bps": self.fx_spread_bps,
                "payment_hash": payment_hash_hex,
                "expiry_blocks": 144 // ~24 hour fallback resolution safety window
            },
            "id": Uuid::new_v4().to_string()
        });

        // 4. Dispatch the instruction directly through the MFA's Enterprise FNN Integration
        log::info!(
            "⚡ [CLEARINGHOUSE] Dispatching intent swap from treasury. Amount: {} Shannons (spread {} bps).",
            amount_shannons,
            self.fx_spread_bps
        );
        let rpc_response = self.enterprise_fnn_client.call_fnn_rpc(mfa_rpc_payload).await;

        let mut lock_guard = self.funding_locks.lock().await;
        lock_guard.release_lock(&alert.short_channel_id);

        match rpc_response {
            Ok(response) => {
                if response.get("error").is_some() {
                    return Err(format!(
                        "Enterprise FNN rejected swap routing: {:?}",
                        response["error"]
                    ));
                }

                log::info!(
                    "✅ [CLEARINGHOUSE] Successfully injected {} Shannons into channel {}. Payment Hash: {}",
                    amount_shannons,
                    alert.short_channel_id,
                    payment_hash_hex
                );
                Ok(())
            }
            Err(e) => Err(format!(
                "Network transmission failure on Enterprise FNN transport: {e}"
            )),
        }
    }
}

/// Test and mock builder helper for `AppState` fixtures.
pub fn mock_enterprise_clearinghouse() -> Arc<EnterpriseClearinghouse> {
    Arc::new(EnterpriseClearinghouse::new(
        Arc::new(crate::fnn_client::EnterpriseFnnClient::new("http://127.0.0.1:8227")),
        "test-vault".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::Value;

    struct MockFnn {
        last_payload: TokioMutex<Option<Value>>,
    }

    #[async_trait]
    impl FiberNodeRpc for MockFnn {
        async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
            *self.last_payload.lock().await = Some(payload);
            Ok(serde_json::json!({ "result": { "status": "ACCEPTED" } }))
        }
    }

    #[tokio::test]
    async fn handle_balance_depletion_dispatches_enterprise_swap() {
        let mock = Arc::new(MockFnn {
            last_payload: TokioMutex::new(None),
        });
        let house = EnterpriseClearinghouse::new(mock.clone(), "corporate-vault-01".to_string());

        let alert = BalanceDepletedPayload {
            agent_id: 44,
            short_channel_id: "0xabc123".to_string(),
            available_outbound_shannons: 50_000,
            minimum_required_shannons: 1_000_000,
            agent_fnn_pubkey: "03deadbeef".to_string(),
        };

        house.handle_balance_depletion(alert).await.expect("refuel");

        let payload = mock.last_payload.lock().await.clone().expect("rpc sent");
        assert_eq!(payload["method"], "execute_cross_hub_intent_swap");
        assert_eq!(payload["params"]["source_vault_id"], "corporate-vault-01");
        let expected = apply_bps_spread_shannons(
            10_000_000,
            std::env::var("CLEARINGHOUSE_FX_SPREAD_BPS")
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(DEFAULT_REFUEL_SPREAD_BPS),
        );
        assert_eq!(payload["params"]["amount_shannons"], expected);
        assert!(payload["params"]["fx_spread_bps"].as_u64().is_some());
    }

    #[test]
    fn funding_lock_blocks_duplicate_channel_acquire() {
        let mut mgr = FundingLockManager::new(60);
        assert!(mgr.try_acquire_lock("0xabc"));
        assert!(!mgr.try_acquire_lock("0xabc"));
        mgr.release_lock("0xabc");
        assert!(mgr.try_acquire_lock("0xabc"));
    }
}
