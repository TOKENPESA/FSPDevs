//! Mesh keysend and multi-hop HTLC payments dispatched by MFA over the agent WebSocket.

use serde::{Deserialize, Serialize};

use crate::fnn_client::{FiberNodeRpc, LiveFnnClient};
use crate::storage::EdgeTxRecord;
use crate::{AgentDb, ConfigUpdatePayload, PaymentResultPayload};
use mesh_core::error::MeshError;

#[derive(Serialize)]
struct FnnMultiHopPaymentArgs {
    peer_pubkey: String,
    amount_shannons: u64,
    payment_hash: String,
    route_hops: Vec<String>,
    cltv_expiry_delta: u32,
}

#[derive(Deserialize)]
struct FnnPaymentResponse {
    payment_preimage: Option<String>,
    status: String,
    failure_reason: Option<String>,
}

/// Forwards a compiled HTLC path to the native FNN upstream execution socket.
pub async fn execute_fiber_multihop_payment(
    fnn_client: &LiveFnnClient,
    target_pubkey: &str,
    amount: u64,
    payment_hash: &str,
    route_manifest: Vec<String>,
) -> Result<String, MeshError> {
    let payload = FnnMultiHopPaymentArgs {
        peer_pubkey: target_pubkey.to_string(),
        amount_shannons: amount,
        payment_hash: payment_hash.to_string(),
        route_hops: route_manifest,
        cltv_expiry_delta: 40,
    };

    let params = serde_json::to_value(payload)
        .map_err(|e| MeshError::PaymentError(format!("invalid multi-hop payload: {e}")))?;

    let response: FnnPaymentResponse = fnn_client
        .call_rpc("send_multi_hop_payment", params)
        .await
        .map_err(|e| MeshError::PaymentError(format!("FNN RPC Node Failed: {e}")))
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|e| MeshError::PaymentError(format!("FNN RPC decode failed: {e}")))
        })?;

    if response.status == "SUCCESS" {
        Ok(response.payment_preimage.unwrap_or_default())
    } else {
        Err(MeshError::PaymentError(format!(
            "HTLC Routing Failed: {}",
            response
                .failure_reason
                .unwrap_or_else(|| "Unknown Execution Error".into())
        )))
    }
}

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
