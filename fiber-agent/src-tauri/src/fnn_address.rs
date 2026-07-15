use bech32::{self, ToBase32, Variant};
use fiber_agent::fnn_client::LiveFnnClient;
use fiber_agent::mesh_ports::resolve_fnn_rpc_url;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::commands::{require_host_arc, OptionalSidecarHost};

/// Testnet SECP256K1_BLAKE160 code hash (CKB system script).
const TESTNET_SECP256K1_BLAKE160_CODE_HASH: &str =
    "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8";

/// Encode a CKB full address (`ckt1…` on testnet) from a lock script.
fn encode_ckb_full_address(
    hrp: &str,
    code_hash_hex: &str,
    hash_type: &str,
    args_hex: &str,
) -> Result<String, String> {
    let code_hash = hex::decode(code_hash_hex.trim_start_matches("0x"))
        .map_err(|err| format!("invalid code_hash hex: {err}"))?;
    if code_hash.len() != 32 {
        return Err(format!(
            "code_hash must be 32 bytes, got {}",
            code_hash.len()
        ));
    }
    let args = hex::decode(args_hex.trim_start_matches("0x"))
        .map_err(|err| format!("invalid args hex: {err}"))?;
    let hash_type_byte = match hash_type {
        "data" => 0u8,
        "type" => 1u8,
        "data1" => 2u8,
        "data2" => 4u8,
        other => return Err(format!("unsupported hash_type: {other}")),
    };

    // CKB full address payload: format(0x00) | hash_type | code_hash | args → Bech32m
    let mut payload = Vec::with_capacity(2 + code_hash.len() + args.len());
    payload.push(0x00);
    payload.push(hash_type_byte);
    payload.extend_from_slice(&code_hash);
    payload.extend_from_slice(&args);

    bech32::encode(hrp, payload.to_base32(), Variant::Bech32m)
        .map_err(|err| format!("bech32 encode failed: {err}"))
}

fn fallback_args_from_compressed_pubkey(pubkey_hex: &str) -> Result<String, String> {
    // Simulate-mode display only. Live path uses FNN `default_funding_lock_script`.
    let bytes = hex::decode(pubkey_hex.trim_start_matches("0x"))
        .map_err(|err| format!("invalid pubkey hex: {err}"))?;
    if bytes.len() != 33 {
        return Err(format!(
            "expected compressed secp256k1 pubkey (33 bytes), got {}",
            bytes.len()
        ));
    }
    let digest = Sha256::digest(&bytes);
    Ok(format!("0x{}", hex::encode(&digest[..20])))
}

fn script_from_node_info(info: &serde_json::Value) -> Result<(String, String, String), String> {
    let script = info
        .get("default_funding_lock_script")
        .or_else(|| info.get("funding_lock_script"))
        .ok_or_else(|| {
            "node_info missing default_funding_lock_script — start FNN (FNN_MODE=testnet) first"
                .to_string()
        })?;

    let code_hash = script
        .get("code_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "funding lock script missing code_hash".to_string())?
        .to_string();
    let hash_type = script
        .get("hash_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "funding lock script missing hash_type".to_string())?
        .to_string();
    let args = script
        .get("args")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "funding lock script missing args".to_string())?
        .to_string();
    Ok((code_hash, hash_type, args))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FnnAddressSnapshot {
    pub address: String,
    pub pubkey: String,
    pub network: String,
    pub fnn_rpc_url: String,
    pub funding_lock_script: serde_json::Value,
    pub source: String,
}

#[tauri::command]
pub async fn get_fnn_address(
    host: State<'_, OptionalSidecarHost>,
) -> Result<FnnAddressSnapshot, String> {
    let host = require_host_arc(host.inner())?;
    let agent_id = host.lock().await.agent_id;
    let fnn_mode = std::env::var("FNN_MODE").unwrap_or_else(|_| "testnet".to_string());
    let is_live =
        fnn_mode.eq_ignore_ascii_case("testnet") || fnn_mode.eq_ignore_ascii_case("live");
    let fnn_rpc_url =
        std::env::var("FNN_RPC_URL").unwrap_or_else(|_| resolve_fnn_rpc_url(agent_id));

    if is_live {
        let client = LiveFnnClient::new(fnn_rpc_url.clone());
        let envelope = client
            .call_fnn_rpc(json!({
                "id": 1,
                "jsonrpc": "2.0",
                "method": "node_info",
                "params": []
            }))
            .await
            .map_err(|err| format!("FNN node_info failed: {err}"))?;

        if let Some(err) = envelope.get("error") {
            return Err(format!("FNN node_info error: {err}"));
        }
        let info = envelope
            .get("result")
            .cloned()
            .ok_or_else(|| format!("FNN node_info missing result: {envelope}"))?;

        let pubkey = info
            .get("pubkey")
            .or_else(|| info.get("node_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let (code_hash, hash_type, args) = script_from_node_info(&info)?;
        let address = encode_ckb_full_address("ckt", &code_hash, &hash_type, &args)?;
        if !address.starts_with("ckt1") {
            return Err(format!("expected testnet ckt1 address, got {address}"));
        }

        return Ok(FnnAddressSnapshot {
            address,
            pubkey,
            network: "testnet".to_string(),
            fnn_rpc_url,
            funding_lock_script: json!({
                "code_hash": code_hash,
                "hash_type": hash_type,
                "args": args,
            }),
            source: "fnn_node_info".to_string(),
        });
    }

    let pubkey = host
        .lock()
        .await
        .fnn_client
        .node_pubkey()
        .await
        .unwrap_or_else(|_| fiber_agent::mesh::agent_fnn_pubkey(agent_id));
    let args = fallback_args_from_compressed_pubkey(&pubkey)?;
    let address = encode_ckb_full_address(
        "ckt",
        TESTNET_SECP256K1_BLAKE160_CODE_HASH,
        "type",
        &args,
    )?;

    Ok(FnnAddressSnapshot {
        address,
        pubkey,
        network: "testnet".to_string(),
        fnn_rpc_url,
        funding_lock_script: json!({
            "code_hash": TESTNET_SECP256K1_BLAKE160_CODE_HASH,
            "hash_type": "type",
            "args": args,
        }),
        source: "simulated".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::encode_ckb_full_address;

    #[test]
    fn encodes_ckt1_prefix_for_testnet_script() {
        let address = encode_ckb_full_address(
            "ckt",
            "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8",
            "type",
            "0x1234567890abcdef1234567890abcdef12345678",
        )
        .expect("encode");
        assert!(address.starts_with("ckt1"), "got {address}");
    }
}
