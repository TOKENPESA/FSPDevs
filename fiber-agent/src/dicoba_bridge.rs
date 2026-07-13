use std::sync::Arc;

use mesh_core::jungukuu_types::{JunguKuuVault, MicroContributionReceipt};
use serde_json::Value;
use uuid::Uuid;

use crate::fnn_client::FiberNodeRpc;
use crate::storage::AgentDb;

fn fnn_simulation_mode() -> bool {
    match std::env::var("FNN_MODE") {
        Ok(mode) => mode.eq_ignore_ascii_case("simulate") || mode.eq_ignore_ascii_case("sim"),
        Err(_) => false,
    }
}

pub struct DicobaEdgeClient {
    db: Arc<AgentDb>,
    fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    local_member_id: Uuid,
}

impl DicobaEdgeClient {
    pub fn new(
        db: Arc<AgentDb>,
        fnn: Arc<dyn FiberNodeRpc + Send + Sync>,
        member_id: Uuid,
    ) -> Self {
        Self {
            db,
            fnn_client: fnn,
            local_member_id: member_id,
        }
    }

    /// Dispatches a zero-friction off-chain micro-contribution to the main group vault.
    pub async fn stream_contribution(
        &self,
        target_vault: &JunguKuuVault,
        amount_shannons: u64,
    ) -> Result<MicroContributionReceipt, String> {
        log::info!(
            "💧 [DICOBA] Streaming {} shannons to group vault: {}",
            amount_shannons,
            target_vault.group_name
        );

        let target_pubkey = target_vault
            .governance_lock
            .leader_pubkeys
            .first()
            .ok_or_else(|| "Vault has no leader pubkeys configured".to_string())?;

        if fnn_simulation_mode() {
            log::info!(
                "🧪 [DICOBA] FNN_MODE=simulate — recording {} shannons locally (target {target_pubkey})",
                amount_shannons
            );
            return self
                .record_contribution(target_vault, amount_shannons)
                .await;
        }

        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "send_payment",
            "params": {
                "target_pubkey": target_pubkey,
                "amount": amount_shannons,
                "asset_type": &target_vault.base_asset_iso,
                "fee_limit": 10
            },
            "id": 1
        });

        let rpc_response = self
            .fnn_client
            .call_fnn_rpc(rpc_payload)
            .await
            .map_err(|err| format!("FNN Node loopback communications timeout: {err:?}"))?;

        if rpc_response
            .get("error")
            .is_some_and(|err| !err.is_null())
        {
            let message = rpc_response["error"]
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown RPC error");
            return Err(format!("Contribution routing rejected: {message}"));
        }

        self.record_contribution(target_vault, amount_shannons)
            .await
    }

    async fn record_contribution(
        &self,
        target_vault: &JunguKuuVault,
        amount_shannons: u64,
    ) -> Result<MicroContributionReceipt, String> {
        let receipt = MicroContributionReceipt {
            transaction_id: Uuid::new_v4(),
            member_id: self.local_member_id,
            vault_id: target_vault.vault_id,
            amount_shannons,
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        self.db
            .record_dicoba_contribution(&receipt, &target_vault.group_name)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnn_client::LiveFnnClient;
    use mesh_core::jungukuu_types::{CycleState, DicobaMember, MultisigQuorum};
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    static FNN_MODE_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        #[allow(dead_code)]
        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn temp_db() -> (AgentDb, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("fiber-agent-dicoba-test-{unique}.db"));
        let db = AgentDb::open_path(path.clone()).expect("open temp db");
        (db, path)
    }

    fn sample_vault() -> JunguKuuVault {
        JunguKuuVault {
            vault_id: Uuid::new_v4(),
            group_name: "Test Dicoba".to_string(),
            cycle_start_timestamp: 1_700_000_000,
            cycle_end_timestamp: 1_703_000_000,
            cycle_state: CycleState::Active,
            base_asset_iso: "TZS".to_string(),
            share_price_shannons: 100_000,
            social_fund_flat_fee_shannons: 25_000,
            base_interest_rate_bps: 500,
            peak_interest_rate_bps: 2_500,
            pool_shares_shannons: 0,
            pool_social_fund_shannons: 0,
            pool_fines_and_interest_shannons: 0,
            governance_lock: MultisigQuorum {
                total_signers: 3,
                required_signatures: 2,
                leader_pubkeys: vec!["03leader_test".to_string()],
            },
            members: vec![DicobaMember::new(Uuid::new_v4(), "03member")],
            l1_cell_outpoint: "0xabc:0".to_string(),
        }
    }

    #[test]
    fn record_contribution_persists_member_and_group() {
        let _lock = FNN_MODE_TEST_LOCK.lock().expect("fnn mode test lock");
        let _env = EnvVarGuard::set("FNN_MODE", "simulate");
        let (db, path) = temp_db();
        let member_id = Uuid::new_v4();
        let vault = sample_vault();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async {
            let client = DicobaEdgeClient::new(
                Arc::new(db),
                Arc::new(LiveFnnClient::new("http://127.0.0.1:1".to_string())),
                member_id,
            );
            client
                .stream_contribution(&vault, 2_500)
                .await
                .expect("contribution");
        });

        let contributors = {
            let db = AgentDb::open_path(path).expect("reopen");
            db.list_vault_contributors(
                &vault.vault_id.to_string(),
                &vault.group_name,
            )
            .expect("contributors")
        };
        assert_eq!(contributors.len(), 1);
        assert_eq!(contributors[0].member_id, member_id.to_string());
        assert_eq!(contributors[0].total_shannons, 2_500);
    }

    #[test]
    fn stream_contribution_rejects_when_fnn_unreachable() {
        let _lock = FNN_MODE_TEST_LOCK.lock().expect("fnn mode test lock");
        let _env = EnvVarGuard::set("FNN_MODE", "live");
        let (db, _path) = temp_db();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(async {
            let client = DicobaEdgeClient::new(
                Arc::new(db),
                Arc::new(LiveFnnClient::new("http://127.0.0.1:1".to_string())),
                Uuid::new_v4(),
            );
            client.stream_contribution(&sample_vault(), 2_000).await
        });
        assert!(result.is_err(), "live mode must not silently record offline");
    }

    #[test]
    fn stream_contribution_requires_leader_pubkey() {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async {
            let mut vault = sample_vault();
            vault.governance_lock.leader_pubkeys.clear();
            let (db, _path) = temp_db();
            let client = DicobaEdgeClient::new(
                Arc::new(db),
                Arc::new(LiveFnnClient::new("http://127.0.0.1:1".to_string())),
                Uuid::new_v4(),
            );
            let _env = EnvVarGuard::set("FNN_MODE", "simulate");
            let err = client
                .stream_contribution(&vault, 1_000)
                .await
                .expect_err("missing leader");
            assert!(err.contains("leader pubkeys"));
        });
    }
}
