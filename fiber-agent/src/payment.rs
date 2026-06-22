//! Mesh keysend payments dispatched by MFA over the agent WebSocket.

use crate::fnn_client::FiberNodeRpc;
use crate::storage::EdgeTxRecord;
use crate::{AgentDb, ConfigUpdatePayload, PaymentResultPayload};

pub async fn execute_mesh_payment(
    fnn: &tokio::sync::Mutex<Box<dyn FiberNodeRpc + Send + Sync>>,
    agent_id: u16,
    cmd: &ConfigUpdatePayload,
    db: Option<&AgentDb>,
) -> PaymentResultPayload {
    let payment_id = cmd
        .payment_id
        .clone()
        .unwrap_or_else(|| format!("pay-{agent_id}-{}", cmd.destination_agent.unwrap_or(0)));
    let destination_agent = cmd.destination_agent.unwrap_or(0);
    let target_fnn_pubkey = cmd.target_fnn_pubkey.clone().unwrap_or_default();
    let amount = cmd.amount_shannons.unwrap_or(0);

    if amount == 0 || target_fnn_pubkey.is_empty() {
        return PaymentResultPayload {
            command: "PAYMENT_RESULT".to_string(),
            payment_id,
            agent: agent_id,
            destination_agent,
            status: "FAILED".to_string(),
            payment_hash: None,
            fee_shannons: None,
            error: Some("invalid payment command (amount or target pubkey missing)".into()),
        };
    }

    let backend = fnn.lock().await;
    match backend
        .send_keysend_payment(&target_fnn_pubkey, amount)
        .await
    {
        Ok(result) => {
            if let Some(db_ref) = db {
                let tx = EdgeTxRecord {
                    tx_hash: result.payment_hash.clone(),
                    direction: "OUTBOUND".to_string(),
                    amount_shannons: amount,
                    fee_earned_shannons: result.fee_shannons,
                    status: result.status.clone(),
                    settled_at: Some(chrono::Utc::now()),
                    created_at: chrono::Utc::now(),
                };
                if let Err(e) = db_ref.insert_edge_transaction(&tx) {
                    eprintln!("⚠️ [STORAGE] payment ledger insert failed: {e}");
                }
            }

            println!(
                "💸 [PAYMENT] FA-{agent_id} → FA-{destination_agent} · {amount} shannons · {} · fee {}",
                result.status, result.fee_shannons
            );

            PaymentResultPayload {
                command: "PAYMENT_RESULT".to_string(),
                payment_id,
                agent: agent_id,
                destination_agent,
                status: if result.status.eq_ignore_ascii_case("Success") {
                    "SUCCESS".to_string()
                } else {
                    result.status.to_uppercase()
                },
                payment_hash: Some(result.payment_hash),
                fee_shannons: Some(result.fee_shannons),
                error: None,
            }
        }
        Err(e) => {
            eprintln!(
                "❌ [PAYMENT] FA-{agent_id} → FA-{destination_agent} failed: {e}"
            );
            PaymentResultPayload {
                command: "PAYMENT_RESULT".to_string(),
                payment_id,
                agent: agent_id,
                destination_agent,
                status: "FAILED".to_string(),
                payment_hash: None,
                fee_shannons: None,
                error: Some(e),
            }
        }
    }
}
