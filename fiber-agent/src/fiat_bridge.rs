//! Mobile money float bridge — cash-in/out against local FNN loopback RPC.

use std::sync::Arc;

use mesh_core::types::{
    EdgeTransaction, EdgeTxType, FeeCalculationBreakdown, FeeLayersConfig, FiatProvider,
    FloatExhaustionTelemetry, L2Asset, SingleCapacityParams,
};
use serde_json::Value;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use crate::clearing_client::post_float_crisis_to_mfa;
use crate::fees::FeeCalculationEngine;
use crate::fnn_client::FiberNodeRpc;
use crate::sanitize_storage_error;
use crate::storage::AgentDb;

pub struct MobileMoneyFloatBridge {
    db: Arc<AgentDb>,
    fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    agent_id: u16,
    provider: FiatProvider,
    agent_account: String,
    critical_fiat_floor: f64,
}

impl MobileMoneyFloatBridge {
    pub fn new(
        db: Arc<AgentDb>,
        fnn: Arc<dyn FiberNodeRpc + Send + Sync>,
        agent_id: u16,
        provider: FiatProvider,
        account: String,
        floor: f64,
    ) -> Self {
        Self {
            db,
            fnn_client: fnn,
            agent_id,
            provider,
            agent_account: account,
            critical_fiat_floor: floor,
        }
    }

    pub fn agent_id(&self) -> u16 {
        self.agent_id
    }

    pub fn provider(&self) -> FiatProvider {
        self.provider
    }

    pub fn agent_account(&self) -> &str {
        &self.agent_account
    }

    /// Customer deposits paper cash and receives L2 RUSD tokens on-channel.
    pub async fn process_cash_in(
        &self,
        customer_pubkey: &str,
        amount_shannons: u64,
        fiat_received: f64,
    ) -> Result<EdgeTransaction, String> {
        println!(
            "📥 [CASH-IN EXECUTION] Routing {amount_shannons} RUSD tokens to counterparty {customer_pubkey}"
        );

        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "send_payment",
            "params": {
                "target_pubkey": customer_pubkey,
                "amount": amount_shannons,
                "asset_type": "RUSD"
            },
            "id": 1
        });

        let rpc_response = self
            .fnn_client
            .call_fnn_rpc(rpc_payload)
            .await
            .map_err(|_| "FNN node communications timed out".to_string())?;

        if rpc_response
            .get("error")
            .is_some_and(|err| !err.is_null())
        {
            return Err(format!(
                "L2 Network execution rejected: {}",
                rpc_response["error"]["message"]
            ));
        }

        let result = rpc_response
            .get("result")
            .ok_or_else(|| "FNN RPC missing result field".to_string())?;

        let preimage = result
            .get("preimage")
            .and_then(Value::as_str)
            .map(str::to_string);
        let payment_hash = result
            .get("payment_hash")
            .and_then(Value::as_str)
            .map(str::to_string);

        let tx = EdgeTransaction::single_capacity(SingleCapacityParams {
            tx_id: Uuid::new_v4(),
            agent_id: self.agent_id,
            tx_type: EdgeTxType::CashIn,
            asset: L2Asset::RusdStablecoin,
            amount_atomic: amount_shannons,
            fiat_amount: fiat_received,
            counterparty_pubkey: customer_pubkey.to_string(),
            payment_hash,
            preimage,
            timestamp: chrono::Utc::now().timestamp(),
            is_synchronized: false,
        });

        self.db
            .insert_fiat_edge_transaction(&tx)
            .map_err(|e| sanitize_storage_error("cache fiat edge transaction", e))?;

        Ok(tx)
    }

    /// Evaluates telco float reserves and emits proactive exhaustion telemetry.
    pub async fn evaluate_fiat_float_drain_velocity(
        &self,
        current_fiat_reserve: f64,
        calculated_drain_rate: f64,
        digital_l2_balance_shannons: u64,
        telemetry_tx: &Sender<String>,
    ) {
        if current_fiat_reserve > self.critical_fiat_floor {
            return;
        }

        println!(
            "⚠️ [BRIDGE CRISIS] Float boundary breach! Reserves: {} (floor {}). Triggers pre-emptive clearing swap.",
            current_fiat_reserve, self.critical_fiat_floor
        );

        let alert = FloatExhaustionTelemetry {
            agent_id: self.agent_id,
            provider: self.provider,
            current_fiat_balance: current_fiat_reserve,
            critical_fiat_floor: self.critical_fiat_floor,
            digital_l2_balance_shannons,
            drain_velocity_per_sec: calculated_drain_rate,
        };

        if let Ok(serialized) = serde_json::to_string(&alert) {
            let _ = telemetry_tx.send(serialized).await;
        }
    }

    /// Builds float exhaustion telemetry and POSTs it to MFA `/clearing/float-crisis`.
    pub async fn dispatch_float_crisis_clearing(
        &self,
        current_fiat_reserve: f64,
        calculated_drain_rate: f64,
        digital_l2_balance_shannons: u64,
    ) -> Result<Option<(FloatExhaustionTelemetry, serde_json::Value)>, String> {
        if current_fiat_reserve > self.critical_fiat_floor {
            return Ok(None);
        }

        println!(
            "⚠️ [BRIDGE CRISIS] Float boundary breach! Reserves: {} (floor {}). Dispatching MFA clearing intake.",
            current_fiat_reserve, self.critical_fiat_floor
        );

        let alert = FloatExhaustionTelemetry {
            agent_id: self.agent_id,
            provider: self.provider,
            current_fiat_balance: current_fiat_reserve,
            critical_fiat_floor: self.critical_fiat_floor,
            digital_l2_balance_shannons,
            drain_velocity_per_sec: calculated_drain_rate,
        };

        let client = reqwest::Client::new();
        match post_float_crisis_to_mfa(&client, &alert, None).await {
            Ok(mfa_response) => Ok(Some((alert, mfa_response))),
            Err(err) => {
                eprintln!("⚠️ [CLEARING DISPATCH] MFA unreachable — queueing float-crisis telemetry: {err}");
                let payload = serde_json::to_string(&alert)
                    .map_err(|_| "Float-crisis telemetry encoding failed".to_string())?;
                let queued = self
                    .db
                    .enqueue_telemetry_raw("FLOAT_CRISIS", &payload)
                    .is_ok();
                Ok(Some((
                    alert,
                    serde_json::json!({
                        "status": "MFA_OFFLINE",
                        "queued": queued,
                        "reason": err,
                    }),
                )))
            }
        }
    }

    /// Generates a validated multi-tier invoice payload for the user interface dashboard.
    pub async fn draft_cash_out_invoice_breakdown(
        &self,
        target_withdrawal_fiat: f64,
        active_rules: &FeeLayersConfig,
    ) -> Result<FeeCalculationBreakdown, String> {
        println!(
            "🧮 [FEE CALCULATION ENGINE] Running multi-layer ledger audit for TZS {target_withdrawal_fiat}"
        );

        let mock_conversion_rate = std::env::var("MFA_FIAT_SHANNONS_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(38.0);
        let estimated_route_hops = 3;

        let breakdown = FeeCalculationEngine::compute_cash_out_breakdown(
            target_withdrawal_fiat,
            active_rules,
            estimated_route_hops,
            mock_conversion_rate,
        );

        println!(
            "📋 [FEE SPLIT] Principal: {} | Net: {} | Agent: {} | Gov Tax: {}",
            breakdown.principal_fiat_amount,
            breakdown.layer1_l2_routing_fee_fiat,
            breakdown.layer2_kiosk_commission_fiat,
            breakdown.layer3_sovereign_levy_fiat
        );

        Ok(breakdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnn_client::LiveFnnClient;
    use mesh_core::types::FiatProvider;

    #[test]
    fn bridge_stores_provider_and_floor() {
        let db = Arc::new(AgentDb::open(99).expect("open test db"));
        let bridge = MobileMoneyFloatBridge::new(
            db,
            Arc::new(LiveFnnClient::new("http://127.0.0.1:18299".to_string())),
            99,
            FiatProvider::Mpesa,
            "255700000000".to_string(),
            50_000.0,
        );
        assert_eq!(bridge.provider(), FiatProvider::Mpesa);
        assert_eq!(bridge.agent_account(), "255700000000");
    }

    #[tokio::test]
    async fn draft_cash_out_invoice_breakdown_returns_fee_layers() {
        let db = Arc::new(AgentDb::open(98).expect("open test db"));
        let bridge = MobileMoneyFloatBridge::new(
            db,
            Arc::new(LiveFnnClient::new("http://127.0.0.1:18298".to_string())),
            98,
            FiatProvider::Mpesa,
            "255700000000".to_string(),
            50_000.0,
        );
        let rules = FeeLayersConfig {
            kiosk_flat_commission: 500.0,
            kiosk_proportional_ppm: 10_000,
            sovereign_levy_rate: 0.001,
        };

        let breakdown = bridge
            .draft_cash_out_invoice_breakdown(10_000.0, &rules)
            .await
            .expect("invoice breakdown");

        assert_eq!(breakdown.principal_fiat_amount, 10_000.0);
        assert_eq!(breakdown.layer1_l2_routing_fee_fiat, 1.5);
        assert_eq!(breakdown.layer2_kiosk_commission_fiat, 600.0);
        assert_eq!(breakdown.layer3_sovereign_levy_fiat, 10.0);
    }
}
