use std::time::Duration;

use bech32::{self, ToBase32, Variant};
use fiber_agent::fnn_client::LiveFnnClient;
use fiber_agent::mesh_ports::resolve_fnn_rpc_url;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::commands::{require_host_arc, OptionalSidecarHost};

/// Testnet SECP256K1_BLAKE160 code hash (CKB system script).
const TESTNET_SECP256K1_BLAKE160_CODE_HASH: &str =
    "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8";
const DEFAULT_CKB_RPC: &str = "http://134.122.120.65:8114";
const FALLBACK_CKB_RPC: &str = "https://testnet.ckbapp.dev/";
const SHANNONS_PER_CKB: u64 = 100_000_000;

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

    // RFC 0021 full payload: 0x00 | code_hash | hash_type | args → Bech32m
    let mut payload = Vec::with_capacity(2 + code_hash.len() + args.len());
    payload.push(0x00);
    payload.extend_from_slice(&code_hash);
    payload.push(hash_type_byte);
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

fn ckb_rpc_candidates() -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(url) = std::env::var("CKB_NODE_RPC_URL") {
        let trimmed = url.trim().to_string();
        if !trimmed.is_empty() {
            urls.push(trimmed);
        }
    }
    for url in [DEFAULT_CKB_RPC, FALLBACK_CKB_RPC] {
        if !urls.iter().any(|u| u == url) {
            urls.push(url.to_string());
        }
    }
    urls
}

fn parse_hex_u64(raw: &str) -> Option<u64> {
    let trimmed = raw.trim().trim_start_matches("0x");
    u64::from_str_radix(trimmed, 16).ok()
}

fn format_ckb_from_shannons(shannons: u64) -> String {
    let whole = shannons / SHANNONS_PER_CKB;
    let frac = shannons % SHANNONS_PER_CKB;
    if frac == 0 {
        format!("{whole}")
    } else {
        let frac_str = format!("{frac:08}");
        let frac_trim = frac_str.trim_end_matches('0');
        format!("{whole}.{frac_trim}")
    }
}

async fn ckb_rpc_call(rpc_url: &str, method: &str, params: Value) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| format!("CKB HTTP client: {err}"))?;
    let body = json!({
        "id": 1,
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    let response = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("CKB RPC unreachable ({rpc_url}): {err}"))?;
    if !response.status().is_success() {
        return Err(format!("CKB RPC HTTP {}", response.status()));
    }
    let envelope: Value = response
        .json()
        .await
        .map_err(|err| format!("CKB RPC JSON: {err}"))?;
    if let Some(err) = envelope.get("error") {
        return Err(format!("CKB RPC error: {err}"));
    }
    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| format!("CKB RPC missing result: {envelope}"))
}

/// Extra CKB (in shannons) kept for fees / change when funding a Fiber channel on L1.
const CHANNEL_OPEN_FEE_RESERVE_SHANNONS: u64 = 100_000_000; // 1 CKB

pub(crate) async fn fetch_l1_capacity_shannons(
    code_hash: &str,
    hash_type: &str,
    args: &str,
) -> Result<u64, String> {
    let search = json!({
        "script": {
            "code_hash": code_hash,
            "hash_type": hash_type,
            "args": args,
        },
        "script_type": "lock",
        "script_search_mode": "exact",
    });

    let mut errors = Vec::new();
    for rpc_url in ckb_rpc_candidates() {
        // Prefer indexer aggregate when available.
        match ckb_rpc_call(&rpc_url, "get_cells_capacity", json!([search])).await {
            Ok(result) => {
                if let Some(cap) = result
                    .get("capacity")
                    .and_then(|v| v.as_str())
                    .and_then(parse_hex_u64)
                {
                    return Ok(cap);
                }
                errors.push(format!(
                    "{rpc_url}: get_cells_capacity missing capacity field"
                ));
            }
            Err(err) => errors.push(format!("{rpc_url}: {err}")),
        }

        // Fallback: page live cells and sum capacities.
        match ckb_rpc_call(
            &rpc_url,
            "get_cells",
            json!([
                {
                    "script": {
                        "code_hash": code_hash,
                        "hash_type": hash_type,
                        "args": args,
                    },
                    "script_type": "lock",
                    "script_search_mode": "exact",
                    "with_data": false,
                },
                "asc",
                "0x64"
            ]),
        )
        .await
        {
            Ok(result) => {
                let objects = result
                    .get("objects")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut total = 0u64;
                for obj in objects {
                    let cap = obj
                        .pointer("/output/capacity")
                        .and_then(|v| v.as_str())
                        .and_then(parse_hex_u64)
                        .unwrap_or(0);
                    total = total.saturating_add(cap);
                }
                return Ok(total);
            }
            Err(err) => errors.push(format!("{rpc_url}: {err}")),
        }
    }
    Err(if errors.is_empty() {
        "no CKB RPC candidates".to_string()
    } else {
        errors.join(" | ")
    })
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
    /// On-chain CKB capacity locked by the FNN funding lock (shannons).
    pub l1_balance_shannons: u64,
    /// Human CKB amount for the funding lock (e.g. `"123.45"`).
    pub l1_balance_ckb: String,
    /// Where L1 capacity was queried from (or error note).
    pub l1_balance_source: String,
}

/// Ensure the FNN funding lock has enough on-chain testnet CKB to fund `amount_shannons`.
/// Without this, Fiber briefly enters `NegotiatingFunding` then drops the channel — Refresh shows 0.
pub async fn require_l1_for_channel_open(
    fnn_rpc_url: &str,
    amount_shannons: u64,
) -> Result<(String, u64), String> {
    let client = LiveFnnClient::new(fnn_rpc_url.to_string());
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

    let chain_hash = info
        .get("chain_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Nervos CKB testnet genesis hash (Fiber `chain: testnet`).
    const TESTNET_CHAIN_HASH: &str =
        "0x10639e0895502b5688a6be8cf69460d76541bfa4821629d86d62ba0aae3f9606";
    if !chain_hash.is_empty() && !chain_hash.eq_ignore_ascii_case(TESTNET_CHAIN_HASH) {
        return Err(format!(
            "FNN is not on CKB testnet (chain_hash={chain_hash}). \
             Use the bundled testnet config (fiber.chain: testnet)."
        ));
    }

    let (code_hash, hash_type, args) = script_from_node_info(&info)?;
    let address = encode_ckb_full_address("ckt", &code_hash, &hash_type, &args)?;
    let balance = fetch_l1_capacity_shannons(&code_hash, &hash_type, &args)
        .await
        .map_err(|err| format!("Could not read L1 funding balance: {err}"))?;
    let need = amount_shannons.saturating_add(CHANNEL_OPEN_FEE_RESERVE_SHANNONS);
    if balance < need {
        return Err(format!(
            "L1 funding lock has only {} CKB on CKB testnet, but opening needs ~{} CKB \
             (plus ~1 CKB fee reserve). Fund this address via the Funding tab / faucet.nervos.org, \
             wait for confirmation, then retry: {address}",
            format_ckb_from_shannons(balance),
            format_ckb_from_shannons(amount_shannons),
        ));
    }
    Ok((address, balance))
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

        let (l1_balance_shannons, l1_balance_source) =
            match fetch_l1_capacity_shannons(&code_hash, &hash_type, &args).await {
                Ok(shannons) => (shannons, "ckb_get_cells_capacity".to_string()),
                Err(err) => {
                    log::warn!("[funding] L1 capacity query failed: {err}");
                    (0, format!("unavailable: {err}"))
                }
            };

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
            l1_balance_shannons,
            l1_balance_ckb: format_ckb_from_shannons(l1_balance_shannons),
            l1_balance_source,
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
        l1_balance_shannons: 0,
        l1_balance_ckb: "0".to_string(),
        l1_balance_source: "simulate".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::encode_ckb_full_address;

    #[test]
    fn encodes_rfc0021_full_address_example() {
        // https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0021-ckb-address-format/0021-ckb-address-format.md
        let address = encode_ckb_full_address(
            "ckb",
            "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8",
            "type",
            "0xb39bbc0b3673c7d36450bc14cfcdad2d559c6c64",
        )
        .expect("encode");
        assert_eq!(
            address,
            "ckb1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqdnnw7qkdnnclfkg59uzn8umtfd2kwxceqxwquc4"
        );
    }

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
