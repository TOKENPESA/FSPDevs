//! Mesh multi-hop HTLC payments dispatched by MFA over the agent WebSocket.

use serde::{Deserialize, Serialize};

use crate::fnn_client::{
    FiberNodeRpc, LiveFnnClient, SendHtlcPaymentArgs, DEFAULT_CLTV_EXPIRY_DELTA_MS,
};
use crate::storage::EdgeTxRecord;
use crate::{AgentDb, ConfigUpdatePayload, PaymentResultPayload};
use mesh_core::error::MeshError;

#[derive(Serialize)]
struct FnnMultiHopPaymentArgs {
    target_pubkey: String,
    amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment_hash: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    trampoline_hops: Vec<String>,
    final_tlc_expiry_delta: u64,
    keysend: bool,
}

#[derive(Deserialize)]
struct FnnPaymentResponse {
    payment_hash: Option<String>,
    payment_preimage: Option<String>,
    status: String,
    #[serde(default)]
    failed_error: Option<String>,
    failure_reason: Option<String>,
}

fn normalize_payment_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        trimmed.to_string()
    } else {
        format!("0x{trimmed}")
    }
}

fn trampoline_hops_from_route(route_hops: &[String], target_pubkey: &str) -> Vec<String> {
    route_hops
        .iter()
        .map(|hop| hop.trim().to_string())
        .filter(|hop| !hop.is_empty() && hop != target_pubkey)
        .collect()
}

/// Forwards a compiled HTLC path to native Fiber `send_payment`.
pub async fn execute_fiber_multihop_payment(
    fnn_client: &LiveFnnClient,
    target_pubkey: &str,
    amount: u64,
    payment_hash: &str,
    route_manifest: Vec<String>,
) -> Result<String, MeshError> {
    let payment_hash = normalize_payment_hash(payment_hash);
    if payment_hash.is_empty() {
        return Err(MeshError::PaymentError(
            "payment_hash required for multi-hop HTLC settlement".into(),
        ));
    }

    let trampoline_hops = trampoline_hops_from_route(&route_manifest, target_pubkey);
    let payload = FnnMultiHopPaymentArgs {
        target_pubkey: target_pubkey.to_string(),
        amount: crate::mesh::shannons_to_hex(amount),
        payment_hash: Some(payment_hash.clone()),
        trampoline_hops,
        final_tlc_expiry_delta: DEFAULT_CLTV_EXPIRY_DELTA_MS,
        keysend: false,
    };

    let params = serde_json::to_value(payload)
        .map_err(|e| MeshError::PaymentError(format!("invalid multi-hop payload: {e}")))?;

    let response: FnnPaymentResponse = fnn_client
        .call_rpc("send_payment", serde_json::json!([params]))
        .await
        .map_err(|e| MeshError::PaymentError(format!("FNN RPC Node Failed: {e}")))
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|e| MeshError::PaymentError(format!("FNN RPC decode failed: {e}")))
        })?;

    let status_ok = response.status.eq_ignore_ascii_case("SUCCESS")
        || response.status.eq_ignore_ascii_case("Success")
        || response.status.eq_ignore_ascii_case("Settled")
        || response.status.eq_ignore_ascii_case("Created")
        || response.status.eq_ignore_ascii_case("InFlight");

    if status_ok {
        Ok(response
            .payment_preimage
            .or(response.payment_hash)
            .unwrap_or(payment_hash))
    } else {
        Err(MeshError::PaymentError(format!(
            "HTLC Routing Failed: {}",
            response
                .failure_reason
                .or(response.failed_error)
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
    let route_hops = cmd.route_hops.clone().unwrap_or_default();
    let payment_hash = cmd
        .payment_hash
        .as_deref()
        .map(normalize_payment_hash)
        .filter(|hash| !hash.is_empty());
    let cltv_expiry_delta = cmd
        .cltv_expiry_delta
        .unwrap_or(DEFAULT_CLTV_EXPIRY_DELTA_MS);

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

    let args = SendHtlcPaymentArgs {
        target_pubkey: target_fnn_pubkey,
        amount_shannons: amount,
        payment_hash,
        route_hops,
        cltv_expiry_delta,
    };

    let backend = fnn.lock().await;
    match backend.send_htlc_payment(args).await {
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
                status: if result.status.eq_ignore_ascii_case("Success")
                    || result.status.eq_ignore_ascii_case("Settled")
                {
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
            eprintln!("❌ [PAYMENT] FA-{agent_id} → FA-{destination_agent} failed: {e}");
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
