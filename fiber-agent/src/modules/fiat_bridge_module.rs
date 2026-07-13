use std::sync::Arc;

use async_trait::async_trait;
use mesh_core::types::FeeLayersConfig;
use serde_json::Value;

use crate::fiat_bridge::MobileMoneyFloatBridge;
use crate::fnn_client::FiberNodeRpc;
use crate::module_system::SidecarModule;
use crate::storage::AgentDb;
use mesh_core::types::FiatProvider;

pub struct FiatBridgeModule {
    bridge: MobileMoneyFloatBridge,
}

impl FiatBridgeModule {
    pub fn new(
        db: Arc<AgentDb>,
        fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
        agent_id: u16,
    ) -> Self {
        Self::with_config(
            db,
            fnn_client,
            agent_id,
            FiatProvider::Mpesa,
            "255700000000".to_string(),
            50_000.0,
        )
    }

    pub fn with_config(
        db: Arc<AgentDb>,
        fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
        agent_id: u16,
        provider: FiatProvider,
        msisdn: String,
        critical_fiat_floor: f64,
    ) -> Self {
        Self {
            bridge: MobileMoneyFloatBridge::new(
                db,
                fnn_client,
                agent_id,
                provider,
                msisdn,
                critical_fiat_floor,
            ),
        }
    }
}

#[async_trait]
impl SidecarModule for FiatBridgeModule {
    fn module_name(&self) -> &'static str {
        "fiat_bridge"
    }

    fn local_agent_id(&self) -> u16 {
        self.bridge.agent_id()
    }

    async fn initialize(&mut self) -> Result<(), String> {
        log::info!("Fiat Bridge Module Ready: Mobile money float engine online.");
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String> {
        match method {
            "calculate_invoice_preview" => {
                let target_fiat = payload["target_fiat"].as_f64().unwrap_or(0.0);
                let rules = FeeLayersConfig {
                    kiosk_flat_commission: payload["flat_commission"].as_f64().unwrap_or(0.0),
                    kiosk_proportional_ppm: payload["proportional_ppm"]
                        .as_u64()
                        .unwrap_or(0) as u32,
                    sovereign_levy_rate: payload["sovereign_levy"].as_f64().unwrap_or(0.0),
                };
                let breakdown = self
                    .bridge
                    .draft_cash_out_invoice_breakdown(target_fiat, &rules)
                    .await?;
                serde_json::to_value(breakdown)
                    .map_err(|err| format!("fee breakdown serialization failed: {err}"))
            }
            "process_cash_in" => {
                let customer_pubkey = payload["customer_pubkey"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let amount_shannons = payload["amount_shannons"].as_u64().unwrap_or(0);
                let fiat_received = payload["fiat_received"].as_f64().unwrap_or(0.0);
                let tx = self
                    .bridge
                    .process_cash_in(&customer_pubkey, amount_shannons, fiat_received)
                    .await?;
                serde_json::to_value(tx).map_err(|err| format!("cash-in serialization failed: {err}"))
            }
            "dispatch_float_crisis_clearing" => {
                let current_fiat = payload["current_fiat"].as_f64().unwrap_or(0.0);
                let drain_rate = payload["drain_rate"].as_f64().unwrap_or(450.0);
                let digital_l2 = payload["digital_l2_balance_shannons"]
                    .as_u64()
                    .unwrap_or(6_200_000);
                match self
                    .bridge
                    .dispatch_float_crisis_clearing(current_fiat, drain_rate, digital_l2)
                    .await?
                {
                    Some((telemetry, mfa_response)) => Ok(serde_json::json!({
                        "status": "breached",
                        "telemetry": telemetry,
                        "mfa_response": mfa_response,
                    })),
                    None => Ok(serde_json::json!({ "status": "safe" })),
                }
            }
            _ => Err(format!("Method '{method}' not supported by the fiat bridge module")),
        }
    }
}
