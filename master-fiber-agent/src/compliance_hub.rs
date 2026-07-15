#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::Arc;

use mesh_core::compliance::{
    CentralBankMacroTelemetry, ComplianceAuditEnvelope, ComplianceVerdict, IntentSwapOrder,
    RevenueAuthorityTaxTelemetry,
};
use mesh_core::currency::CurrencyAssetConfig;
use tokio::sync::broadcast::Sender;
use uuid::Uuid;

use crate::state::AppState;

pub struct SovereignComplianceHub {
    app_state: Arc<AppState>,
    compliance_broadcast_bus: Sender<ComplianceAuditEnvelope>,
}

impl SovereignComplianceHub {
    pub fn new(state: Arc<AppState>, bus: Sender<ComplianceAuditEnvelope>) -> Self {
        Self {
            app_state: state,
            compliance_broadcast_bus: bus,
        }
    }

    /// Pre-clears and intercepts multi-currency trade entries before any off-chain channel balances shift.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_and_enforce_compliance(
        &self,
        agent_id: u16,
        tx_type: &str,
        source_iso: &str,
        target_iso: &str,
        principal_fiat: f64,
        calculated_levy: f64,
        currency_config: &CurrencyAssetConfig,
        order: &IntentSwapOrder,
    ) -> Result<ComplianceVerdict, String> {
        let audit_uuid = Uuid::new_v4();
        let current_unix_time = chrono::Utc::now().timestamp() as u64;

        let simulated_rolling_24h_total = 72_450_000.0 + principal_fiat;
        let macro_velocity_percent =
            (simulated_rolling_24h_total / currency_config.macro_velocity_limit_24h) * 100.0;

        let final_verdict = if simulated_rolling_24h_total > currency_config.macro_velocity_limit_24h
        {
            ComplianceVerdict::SovereignBlocked
        } else if principal_fiat >= (currency_config.macro_velocity_limit_24h * 0.15) {
            ComplianceVerdict::AuditFlagged
        } else {
            ComplianceVerdict::ClearedClean
        };

        let central_bank_feed = CentralBankMacroTelemetry {
            clearing_node_id: order.infrastructure_channel_id,
            source_corridor_iso: source_iso.to_string(),
            destination_corridor_iso: target_iso.to_string(),
            volume_fiat_value: principal_fiat,
            rolling_24h_corridor_total: simulated_rolling_24h_total,
            macro_velocity_percent,
            masked_kyc_token: format!(
                "FSP-KYC-{}-{}",
                source_iso,
                Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0000")
                    .to_uppercase()
            ),
        };

        let revenue_authority_feed = RevenueAuthorityTaxTelemetry {
            originating_agent_id: agent_id,
            transaction_type: tx_type.to_string(),
            gross_value_fiat: principal_fiat,
            agent_commission_earned: principal_fiat * 0.005,
            calculated_sovereign_tax_levy: calculated_levy,
            revenue_tax_code_reference: format!("REV-LEVY-{source_iso}-NPS-2026"),
        };

        let audit_envelope = ComplianceAuditEnvelope {
            audit_id: audit_uuid,
            sequence_index: chrono::Utc::now().timestamp_millis() as u64,
            transaction_timestamp: current_unix_time,
            central_bank_feed,
            revenue_authority_feed,
            final_verdict: final_verdict.clone(),
            administrative_lock_signature: match final_verdict {
                ComplianceVerdict::SovereignBlocked => {
                    Some("SIG_GOV_THRESHOLD_BREACH_FREEZE_DISPATCHED".to_string())
                }
                _ => None,
            },
        };

        let _ = self.compliance_broadcast_bus.send(audit_envelope);

        if final_verdict == ComplianceVerdict::SovereignBlocked {
            return Err(format!(
                "CRITICAL ABORT: Transaction blocked by compliance gateway rules. Corridor boundary cap exceeded for target currency pairing: {source_iso}"
            ));
        }

        Ok(final_verdict)
    }
}

impl AppState {
    pub fn sovereign_compliance_hub(self: &Arc<Self>) -> SovereignComplianceHub {
        SovereignComplianceHub::new(self.clone(), self.compliance_broadcast.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HubConfig;
    use crate::workers::background::ExpiringLockManager;
    use mesh_core::AssetRegistryHub;
    use mesh_core::MeshPubkeyRegistry;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::atomic::AtomicU16;
    use tokio::sync::{broadcast, mpsc, RwLock};

    fn sample_currency_config(limit: f64) -> CurrencyAssetConfig {
        CurrencyAssetConfig {
            iso_code: "TZS".to_string(),
            country_name: "Tanzania".to_string(),
            atomic_decimals: 8,
            udt_code_hash: "0xabc".to_string(),
            udt_args: "0x01".to_string(),
            macro_velocity_limit_24h: limit,
        }
    }

    fn sample_swap_order() -> IntentSwapOrder {
        IntentSwapOrder {
            swap_id: Uuid::new_v4(),
            infrastructure_channel_id: Uuid::new_v4(),
            counterparty_pubkey: "03abc".to_string(),
            target_asset_symbol: "RUSD".to_string(),
            expiration_locktime: 1_700_000_100,
        }
    }

    fn mock_state(compliance_tx: broadcast::Sender<ComplianceAuditEnvelope>) -> Arc<AppState> {
        let (plugin_registry, module_store, plugin_hot_reloader) =
            crate::test_support::test_plugin_stack(10_000_000);
        let (tx, _) = mpsc::channel(8);
        Arc::new(AppState {
            tx_queue: tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(crate::graph::CompleteMeshGraph::new())),
            ui_broadcast: broadcast::channel(4).0,
            compliance_broadcast: compliance_tx,
            compliance_tickets: crate::auth::EphemeralTicketRegistry::new(
                crate::auth::EPHEMERAL_TICKET_TTL_SECS,
            ),
            alert_dedupe: RwLock::new(HashSet::new()),
            alert_order: RwLock::new(VecDeque::new()),
            active_funding_locks: RwLock::new(ExpiringLockManager::new(60)),
            hub_config: HubConfig {
                rpc_url: "http://127.0.0.1:8227".to_string(),
                funding_allocation_shannons: 10_000_000,
            },
            agent_ws_token: "test".to_string(),
            api_token: "test-api-token-123456".to_string(),
            agent_fnn_pubkeys: RwLock::new(HashMap::new()),
            agent_peer_addresses: RwLock::new(HashMap::new()),
            mesh_pubkey_registry: MeshPubkeyRegistry::from_map(HashMap::new()),
            payment_waiters: Arc::new(RwLock::new(HashMap::new())),
            payment_engine: crate::payment::PaymentEngineState::new(),
            simulation_edge_nodes: AtomicU16::new(16),
            ws_allowed_origins: vec![],
            agent_liquidity_snap: RwLock::new(HashMap::new()),
            liquidity_copilot: RwLock::new(crate::workers::background::LiquidityCopilot::new()),
            multi_hub_registry: RwLock::new(crate::hub::MultiHubRegistry::new()),
            asset_registry: AssetRegistryHub::new(),
            papss_gateway: None,
            enterprise_clearinghouse: crate::clearinghouse::mock_enterprise_clearinghouse(),
            regional_clearing: Arc::new(crate::clearing::RegionalClearinghouseEngine::new()),
            edge_hardware_profiles: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry,
            module_store,
            plugin_hot_reloader,
        })
    }

    #[tokio::test]
    async fn small_transaction_clears_clean() {
        let (tx, mut rx) = broadcast::channel(4);
        let hub = SovereignComplianceHub::new(mock_state(tx.clone()), tx);
        let verdict = hub
            .process_and_enforce_compliance(
                44,
                "CASH_OUT",
                "TZS",
                "KES",
                10_000.0,
                50.0,
                &sample_currency_config(100_000_000.0),
                &sample_swap_order(),
            )
            .await
            .expect("cleared");

        assert_eq!(verdict, ComplianceVerdict::ClearedClean);
        assert!(rx.recv().await.is_ok());
    }

    #[tokio::test]
    async fn sovereign_blocked_when_rolling_total_exceeds_cap() {
        let (tx, _) = broadcast::channel(4);
        let hub = SovereignComplianceHub::new(mock_state(tx.clone()), tx);
        let err = hub
            .process_and_enforce_compliance(
                44,
                "CASH_OUT",
                "TZS",
                "KES",
                30_000_000.0,
                50.0,
                &sample_currency_config(80_000_000.0),
                &sample_swap_order(),
            )
            .await
            .expect_err("blocked");

        assert!(err.contains("CRITICAL ABORT"));
    }
}
