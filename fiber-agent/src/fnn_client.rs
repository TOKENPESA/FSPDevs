use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::mesh::{mesh_neighbor_ids, shannons_to_hex, DEFAULT_OPEN_CHANNEL_SHANNONS};
use mesh_core::types::{AssetCapacity, L2Asset};
use crate::{agent_fnn_pubkey, peer_id_from_agent_pubkey, MeshChannelState};

/// Default Fiber final TLC expiry delta (4 hours), matching testnet channel defaults.
pub const DEFAULT_CLTV_EXPIRY_DELTA_MS: u64 = 14_400_000;

/// Loads `FNN_BISCUIT_TOKEN` for Fiber RPC Bearer auth (required in production custody).
pub fn resolve_fnn_biscuit_token() -> Option<String> {
    match std::env::var("FNN_BISCUIT_TOKEN") {
        Ok(token) => {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    }
}

fn bearer_headers(biscuit_token: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(token) = biscuit_token {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}

/// Arguments for native Fiber `send_payment` multi-hop HTLC dispatch.
#[derive(Debug, Clone)]
pub struct SendHtlcPaymentArgs {
    pub target_pubkey: String,
    pub amount_shannons: u64,
    pub payment_hash: Option<String>,
    pub route_hops: Vec<String>,
    pub cltv_expiry_delta: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RgbCellCapacity {
    pub cell_type_hash: String,
    pub amount_atomic: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fractional_nanos: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BitcoinUtxoBinding {
    pub txid: String,
    pub vout: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_cell_id: Option<String>,
}

#[derive(Serialize)]
struct RpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Deserialize)]
struct ListChannelsRpcResult {
    channels: Vec<FnnChannel>,
}

#[derive(Deserialize)]
struct FnnAssetBalance {
    #[serde(default, alias = "asset_type")]
    asset: Option<String>,
    #[serde(default)]
    amount: Option<Value>,
    #[serde(default)]
    fractional_nanos: Option<u64>,
    #[serde(default, alias = "cell_type_hash")]
    rgb_cell_type_hash: Option<String>,
}

#[derive(Deserialize)]
struct FnnChannel {
    #[serde(default, alias = "channel_id")]
    channel_id: Option<String>,
    pubkey: String,
    #[serde(default)]
    enabled: bool,
    state: Value,
    #[serde(default)]
    local_balance: Option<Value>,
    #[serde(default)]
    remote_balance: Option<Value>,
    #[serde(default)]
    local_asset_balances: Option<Vec<FnnAssetBalance>>,
    #[serde(default)]
    remote_asset_balances: Option<Vec<FnnAssetBalance>>,
    /// Legacy combined multi-asset balance blob from older FNN builds.
    #[serde(default)]
    #[allow(dead_code)]
    asset_balances: Option<Value>,
}

#[derive(Deserialize)]
struct NodeInfoResult {
    pubkey: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentResult {
    pub payment_hash: String,
    pub status: String,
    pub fee_shannons: u64,
}

#[async_trait]
pub trait FiberNodeRpc: Send + Sync {
    /// Dispatches a raw JSON-RPC payload to the underlying Fiber node.
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String>;

    /// Verifies if a specific HTLC payment hash has successfully settled on-chain.
    async fn payment_is_success(&self, payment_hash: &str) -> Result<bool, String>;

    async fn list_channels(&self) -> Result<Vec<MeshChannelState>, String>;
    async fn node_pubkey(&self) -> Result<String, String>;
    async fn connect_peer(&self, peer_public_key: &str) -> Result<(), String>;
    async fn open_channel(&self, peer_public_key: &str, amount: u64) -> Result<(), String>;
    async fn close_channel(&self, peer_public_key: &str, force: bool) -> Result<(), String>;
    async fn send_keysend_payment(
        &self,
        target_pubkey: &str,
        amount: u64,
    ) -> Result<PaymentResult, String>;

    /// Dispatches a multi-hop HTLC via Fiber `send_payment` (trampoline hops + CLTV).
    async fn send_htlc_payment(&self, args: SendHtlcPaymentArgs) -> Result<PaymentResult, String>;

    /// Queries RGB++ isomorphic cell capacity for a given type hash on CKB.
    async fn get_rgb_cell_capacity(&self, cell_type_hash: &str) -> Result<RgbCellCapacity, String>;

    /// Maps a Bitcoin UTXO to its bound CKB cell via RGB++ isomorphic binding.
    async fn map_bitcoin_utxo(&self, txid: &str, vout: u32) -> Result<BitcoinUtxoBinding, String>;
}

/// Bridges an `Arc<dyn FiberNodeRpc>` into the mutex-backed backend used by MFA control WS.
pub struct ArcFnnBackend(pub Arc<dyn FiberNodeRpc + Send + Sync>);

#[async_trait]
impl FiberNodeRpc for ArcFnnBackend {
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        self.0.call_fnn_rpc(payload).await
    }

    async fn payment_is_success(&self, payment_hash: &str) -> Result<bool, String> {
        self.0.payment_is_success(payment_hash).await
    }

    async fn list_channels(&self) -> Result<Vec<MeshChannelState>, String> {
        self.0.list_channels().await
    }

    async fn node_pubkey(&self) -> Result<String, String> {
        self.0.node_pubkey().await
    }

    async fn connect_peer(&self, peer_public_key: &str) -> Result<(), String> {
        self.0.connect_peer(peer_public_key).await
    }

    async fn open_channel(&self, peer_public_key: &str, amount: u64) -> Result<(), String> {
        self.0.open_channel(peer_public_key, amount).await
    }

    async fn close_channel(&self, peer_public_key: &str, force: bool) -> Result<(), String> {
        self.0.close_channel(peer_public_key, force).await
    }

    async fn send_keysend_payment(
        &self,
        target_pubkey: &str,
        amount: u64,
    ) -> Result<PaymentResult, String> {
        self.0.send_keysend_payment(target_pubkey, amount).await
    }

    async fn send_htlc_payment(&self, args: SendHtlcPaymentArgs) -> Result<PaymentResult, String> {
        self.0.send_htlc_payment(args).await
    }

    async fn get_rgb_cell_capacity(&self, cell_type_hash: &str) -> Result<RgbCellCapacity, String> {
        self.0.get_rgb_cell_capacity(cell_type_hash).await
    }

    async fn map_bitcoin_utxo(&self, txid: &str, vout: u32) -> Result<BitcoinUtxoBinding, String> {
        self.0.map_bitcoin_utxo(txid, vout).await
    }
}

#[derive(Clone)]
pub struct LiveFnnClient {
    rpc_url: String,
    http_client: Client,
    biscuit_token: Option<String>,
    local_pubkey: Arc<RwLock<Option<String>>>,
}

#[derive(Clone)]
pub struct SimulatedFnnClient {
    agent_id: u16,
    channels: Arc<RwLock<Vec<MeshChannelState>>>,
}

impl LiveFnnClient {
    pub fn new(rpc_url: String) -> Self {
        Self::with_biscuit_token(rpc_url, resolve_fnn_biscuit_token())
    }

    pub fn with_biscuit_token(rpc_url: String, biscuit_token: Option<String>) -> Self {
        Self {
            rpc_url,
            http_client: Client::builder()
                .timeout(Duration::from_secs(10))
                .default_headers(bearer_headers(biscuit_token.as_deref()))
                .build()
                .expect("Failed to build FNN HTTP client"),
            biscuit_token,
            local_pubkey: Arc::new(RwLock::new(None)),
        }
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.biscuit_token.as_deref() {
            Some(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
            None => request,
        }
    }

    pub(crate) fn build_send_payment_params(args: &SendHtlcPaymentArgs) -> Value {
        let trampoline_hops: Vec<String> = args
            .route_hops
            .iter()
            .map(|hop| hop.trim().to_string())
            .filter(|hop| !hop.is_empty() && hop != &args.target_pubkey)
            .collect();

        let cltv = if args.cltv_expiry_delta == 0 {
            DEFAULT_CLTV_EXPIRY_DELTA_MS
        } else {
            args.cltv_expiry_delta
        };

        let mut params = serde_json::json!({
            "target_pubkey": args.target_pubkey,
            "amount": shannons_to_hex(args.amount_shannons),
            "final_tlc_expiry_delta": cltv,
        });

        if let Some(hash) = args.payment_hash.as_deref().map(str::trim).filter(|h| !h.is_empty())
        {
            let normalized = if hash.starts_with("0x") || hash.starts_with("0X") {
                hash.to_string()
            } else {
                format!("0x{hash}")
            };
            params["payment_hash"] = Value::String(normalized);
            params["keysend"] = Value::Bool(false);
        } else {
            params["keysend"] = Value::Bool(true);
        }

        if !trampoline_hops.is_empty() {
            params["trampoline_hops"] = Value::Array(
                trampoline_hops
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            );
        }

        params
    }

    pub(crate) async fn call_rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let payload = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: method.to_string(),
            params,
        };

        let response = self
            .authorize(self.http_client.post(&self.rpc_url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("FNN RPC unreachable: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("FNN RPC HTTP {}", response.status()));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse FNN RPC response: {e}"))?;

        if let Some(err) = body.get("error") {
            return Err(format!("FNN RPC error: {err}"));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| format!("FNN RPC missing result field: {body}"))
    }

    /// Posts a raw JSON-RPC envelope (used by the mobile-money float bridge).
    pub async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        let response = self
            .authorize(self.http_client.post(&self.rpc_url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("FNN Network Timeout: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "FNN Node rejected connection: HTTP {}",
                response.status()
            ));
        }

        response
            .json::<Value>()
            .await
            .map_err(|e| format!("FNN JSON parse error: {e}"))
    }

    async fn poll_payment_result(
        &self,
        payment_hash: &str,
        initial_status: String,
        initial_fee: u64,
    ) -> Result<PaymentResult, String> {
        let mut final_status = initial_status;
        let mut final_fee = initial_fee;
        for _ in 0..20 {
            if final_status.eq_ignore_ascii_case("Success")
                || final_status.eq_ignore_ascii_case("Failed")
                || final_status.eq_ignore_ascii_case("Settled")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let payment = self
                .call_rpc(
                    "get_payment",
                    serde_json::json!([{ "payment_hash": payment_hash }]),
                )
                .await?;
            final_status = payment
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or(&final_status)
                .to_string();
            final_fee = Self::parse_balance_shannons(payment.get("fee"));
        }

        Ok(PaymentResult {
            payment_hash: payment_hash.to_string(),
            status: final_status,
            fee_shannons: final_fee,
        })
    }

    /// Returns true when FNN reports the payment hash as successfully settled.
    pub async fn payment_is_success(&self, payment_hash: &str) -> Result<bool, String> {
        let trimmed = payment_hash.trim();
        if trimmed.is_empty() {
            return Ok(false);
        }

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_payment",
            "params": { "payment_hash": trimmed },
            "id": 1
        });

        let res = self.call_fnn_rpc(payload).await?;
        if let Some(status) = res
            .get("result")
            .and_then(|r| r.get("status"))
            .and_then(Value::as_str)
        {
            return Ok(status.eq_ignore_ascii_case("Success")
                || status.eq_ignore_ascii_case("Settled"));
        }
        Ok(false)
    }

    pub(crate) fn channel_is_active(state: &Value) -> bool {
        if state
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("CHANNELREADY"))
        {
            return true;
        }
        if state.get("ChannelReady").is_some() {
            return true;
        }
        state
            .get("state_name")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("ChannelReady"))
    }

    pub(crate) fn pubkey_to_peer_stub(pubkey: &str) -> u16 {
        pubkey
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or_else(|_| {
                pubkey
                    .bytes()
                    .fold(0u32, |acc, b| acc.wrapping_add(b as u32))
                    .rem_euclid(1024) as u16
                    + 1
            })
    }

    pub(crate) fn parse_balance_shannons(value: Option<&Value>) -> u64 {
        let Some(value) = value else {
            return 0;
        };

        if let Some(text) = value.as_str() {
            let hex = text.strip_prefix("0x").unwrap_or(text);
            return u128::from_str_radix(hex, 16)
                .unwrap_or(0)
                .min(u64::MAX as u128) as u64;
        }

        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
            .unwrap_or(0)
    }

    fn parse_asset_capacity(entry: &FnnAssetBalance) -> Option<AssetCapacity> {
        let asset_label = entry.asset.as_deref()?.trim();
        if asset_label.is_empty() {
            return None;
        }
        let asset = L2Asset::from_ledger_label(asset_label).ok()?;
        let amount_atomic = Self::parse_balance_shannons(entry.amount.as_ref());
        let mut cap = AssetCapacity::new(asset, amount_atomic);
        cap.rwa_fraction_nanos = entry.fractional_nanos;
        cap.rgb_cell_type_hash = entry.rgb_cell_type_hash.clone();
        Some(cap)
    }

    fn parse_asset_balance_list(entries: Option<Vec<FnnAssetBalance>>) -> Vec<AssetCapacity> {
        entries
            .unwrap_or_default()
            .iter()
            .filter_map(Self::parse_asset_capacity)
            .collect()
    }

    fn merge_native_capacity(
        mut capacities: Vec<AssetCapacity>,
        native_amount: u64,
        side: &str,
    ) -> Vec<AssetCapacity> {
        let _ = side;
        if native_amount == 0 {
            return capacities;
        }
        if capacities
            .iter()
            .any(|cap| cap.asset == L2Asset::CkbNative)
        {
            return capacities;
        }
        capacities.insert(0, AssetCapacity::new(L2Asset::CkbNative, native_amount));
        capacities
    }

    pub(crate) fn decode_list_channels(result: Value) -> Result<Vec<MeshChannelState>, String> {
        let parsed: ListChannelsRpcResult = serde_json::from_value(result)
            .map_err(|e| format!("Failed to decode list_channels result: {e}"))?;

        Ok(parsed
            .channels
            .into_iter()
            .map(|ch| {
                let peer_id = Self::pubkey_to_peer_stub(&ch.pubkey);
                let local_native = Self::parse_balance_shannons(ch.local_balance.as_ref());
                let remote_native = Self::parse_balance_shannons(ch.remote_balance.as_ref());
                let mut local_capacities = Self::parse_asset_balance_list(ch.local_asset_balances);
                let mut remote_capacities = Self::parse_asset_balance_list(ch.remote_asset_balances);
                local_capacities =
                    Self::merge_native_capacity(local_capacities, local_native, "local");
                remote_capacities =
                    Self::merge_native_capacity(remote_capacities, remote_native, "remote");

                MeshChannelState {
                    peer_id,
                    nonce: 1,
                    consecutive_failures: 0,
                    is_active: ch.enabled && Self::channel_is_active(&ch.state),
                    peer_pubkey: Some(ch.pubkey),
                    channel_id: ch.channel_id,
                    local_balance_shannons: local_native,
                    remote_balance_shannons: remote_native,
                    local_capacities,
                    remote_capacities,
                }
            })
            .collect())
    }

    async fn query_rgb_cell_capacity(
        &self,
        cell_type_hash: &str,
    ) -> Result<RgbCellCapacity, String> {
        let hash = cell_type_hash.trim();
        if hash.is_empty() || hash.len() > 128 {
            return Err("cell_type_hash must be 1..=128 characters".to_string());
        }
        let result = self
            .call_rpc(
                "get_rgb_cell_capacity",
                serde_json::json!([{ "cell_type_hash": hash }]),
            )
            .await?;
        serde_json::from_value(result)
            .map_err(|e| format!("decode get_rgb_cell_capacity: {e}"))
    }

    async fn query_bitcoin_utxo_binding(
        &self,
        txid: &str,
        vout: u32,
    ) -> Result<BitcoinUtxoBinding, String> {
        let txid = txid.trim();
        if txid.is_empty() || txid.len() > 128 {
            return Err("txid must be 1..=128 characters".to_string());
        }
        let result = self
            .call_rpc(
                "map_bitcoin_utxo",
                serde_json::json!([{ "txid": txid, "vout": vout }]),
            )
            .await?;
        serde_json::from_value(result)
            .map_err(|e| format!("decode map_bitcoin_utxo: {e}"))
    }
}

fn simulated_channel_balances(agent_id: u16, peer_id: u16) -> (u64, u64) {
    let per_channel = DEFAULT_OPEN_CHANNEL_SHANNONS / 3;
    let skew = ((agent_id as u64).wrapping_mul(17) + peer_id as u64).rem_euclid(5_000_000_000);
    let local = per_channel.saturating_mul(2).saturating_add(skew);
    let remote = per_channel.saturating_add(per_channel / 2).saturating_sub(skew / 2);
    (local, remote)
}

impl SimulatedFnnClient {
    pub fn new(agent_id: u16) -> Self {
        let channels = mesh_neighbor_ids(agent_id, 1024)
            .into_iter()
            .map(|peer_id| {
                let (local_balance_shannons, remote_balance_shannons) =
                    simulated_channel_balances(agent_id, peer_id);
                MeshChannelState {
                    peer_id,
                    nonce: 1,
                    consecutive_failures: 0,
                    is_active: true,
                    peer_pubkey: Some(agent_fnn_pubkey(peer_id)),
                    channel_id: Some(format!("sim-channel-{agent_id}-{peer_id}")),
                    local_balance_shannons,
                    remote_balance_shannons,
                    local_capacities: vec![AssetCapacity::new(L2Asset::CkbNative, local_balance_shannons)],
                    remote_capacities: vec![AssetCapacity::new(L2Asset::CkbNative, remote_balance_shannons)],
                }
            })
            .collect();

        Self {
            agent_id,
            channels: Arc::new(RwLock::new(channels)),
        }
    }

    async fn send_direct_simulated(
        &self,
        args: &SendHtlcPaymentArgs,
    ) -> Result<PaymentResult, String> {
        let target_pubkey = args.target_pubkey.as_str();
        let amount = args.amount_shannons;
        let peer_id = peer_id_from_agent_pubkey(target_pubkey)
            .unwrap_or_else(|| LiveFnnClient::pubkey_to_peer_stub(target_pubkey));
        let fee_shannons = 1_000u64;
        let total = amount.saturating_add(fee_shannons);

        let mut channels = self.channels.write().await;
        let channel_idx = channels
            .iter()
            .position(|c| c.is_active && c.peer_id == peer_id)
            .or_else(|| {
                channels.iter().position(|c| {
                    c.is_active && c.peer_pubkey.as_deref() == Some(target_pubkey)
                })
            })
            .ok_or_else(|| {
                format!("no active simulated channel toward FA-{peer_id} ({target_pubkey})")
            })?;

        let channel = &mut channels[channel_idx];

        if channel.local_balance_shannons < total {
            return Err(format!(
                "insufficient outbound balance: have {} need {total} shannons",
                channel.local_balance_shannons
            ));
        }

        channel.local_balance_shannons -= total;
        channel.remote_balance_shannons += amount;

        let payment_hash = args.payment_hash.clone().unwrap_or_else(|| {
            format!(
                "sim-pay-{}-{}-{}",
                self.agent_id,
                peer_id,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            )
        });

        Ok(PaymentResult {
            payment_hash,
            status: "Success".to_string(),
            fee_shannons,
        })
    }
}

#[async_trait]
impl FiberNodeRpc for SimulatedFnnClient {
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        tokio::time::sleep(Duration::from_millis(600)).await;

        let method = payload
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

        match method {
            "send_payment" => {
                log::info!(
                    "🧪 [MOCK FNN] Simulating successful payment dispatch for FA-{}",
                    self.agent_id
                );
                let mock_preimage = format!(
                    "mock-preimage-{}",
                    Uuid::new_v4().to_string().replace('-', "")
                );
                let mock_hash = format!(
                    "0xmockhash{}",
                    Uuid::new_v4().to_string().replace('-', "")
                );

                Ok(serde_json::json!({
                    "jsonrpc": "2.0",
                    "result": {
                        "payment_hash": mock_hash,
                        "preimage": mock_preimage,
                        "status": "Settled"
                    },
                    "id": payload.get("id").unwrap_or(&serde_json::json!(1))
                }))
            }
            _ => Ok(serde_json::json!({ "jsonrpc": "2.0", "result": {}, "id": 1 })),
        }
    }

    async fn payment_is_success(&self, _payment_hash: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn list_channels(&self) -> Result<Vec<MeshChannelState>, String> {
        Ok(self.channels.read().await.clone())
    }

    async fn node_pubkey(&self) -> Result<String, String> {
        Ok(agent_fnn_pubkey(self.agent_id))
    }

    async fn connect_peer(&self, _peer_public_key: &str) -> Result<(), String> {
        Ok(())
    }

    async fn open_channel(&self, peer_public_key: &str, _amount: u64) -> Result<(), String> {
        let peer_id = peer_id_from_agent_pubkey(peer_public_key)
            .unwrap_or_else(|| LiveFnnClient::pubkey_to_peer_stub(peer_public_key));

        let mut channels = self.channels.write().await;
        if let Some(channel) = channels.iter_mut().find(|c| c.peer_id == peer_id) {
            channel.is_active = true;
            channel.consecutive_failures = 0;
            channel.peer_pubkey = Some(peer_public_key.to_string());
            return Ok(());
        }

        let (local_balance_shannons, remote_balance_shannons) =
            simulated_channel_balances(self.agent_id, peer_id);
        channels.push(MeshChannelState {
            peer_id,
            nonce: 1,
            consecutive_failures: 0,
            is_active: true,
            peer_pubkey: Some(peer_public_key.to_string()),
            channel_id: Some(format!("sim-channel-{}-{peer_id}", self.agent_id)),
            local_balance_shannons,
            remote_balance_shannons,
            local_capacities: vec![AssetCapacity::new(L2Asset::CkbNative, local_balance_shannons)],
            remote_capacities: vec![AssetCapacity::new(L2Asset::CkbNative, remote_balance_shannons)],
        });
        Ok(())
    }

    async fn close_channel(&self, peer_public_key: &str, _force: bool) -> Result<(), String> {
        let peer_id = peer_id_from_agent_pubkey(peer_public_key)
            .unwrap_or_else(|| LiveFnnClient::pubkey_to_peer_stub(peer_public_key));

        let mut channels = self.channels.write().await;
        if let Some(channel) = channels.iter_mut().find(|c| c.peer_id == peer_id) {
            channel.is_active = false;
        }
        Ok(())
    }

    async fn send_keysend_payment(
        &self,
        target_pubkey: &str,
        amount: u64,
    ) -> Result<PaymentResult, String> {
        self.send_htlc_payment(SendHtlcPaymentArgs {
            target_pubkey: target_pubkey.to_string(),
            amount_shannons: amount,
            payment_hash: None,
            route_hops: Vec::new(),
            cltv_expiry_delta: DEFAULT_CLTV_EXPIRY_DELTA_MS,
        })
        .await
    }

    async fn send_htlc_payment(&self, args: SendHtlcPaymentArgs) -> Result<PaymentResult, String> {
        if args.route_hops.is_empty() {
            return SimulatedFnnClient::send_direct_simulated(self, &args).await;
        }

        // Multi-hop: settle against the first outbound hop while recording the full route.
        let first_hop = args
            .route_hops
            .iter()
            .map(|hop| hop.trim())
            .find(|hop| !hop.is_empty())
            .unwrap_or(args.target_pubkey.as_str());

        let mut result = SimulatedFnnClient::send_direct_simulated(
            self,
            &SendHtlcPaymentArgs {
                target_pubkey: first_hop.to_string(),
                amount_shannons: args.amount_shannons,
                payment_hash: args.payment_hash.clone(),
                route_hops: Vec::new(),
                cltv_expiry_delta: args.cltv_expiry_delta,
            },
        )
        .await?;

        if let Some(hash) = args.payment_hash {
            result.payment_hash = hash;
        }
        Ok(result)
    }

    async fn get_rgb_cell_capacity(&self, cell_type_hash: &str) -> Result<RgbCellCapacity, String> {
        let hash = cell_type_hash.trim();
        if hash.is_empty() {
            return Err("cell_type_hash required".to_string());
        }
        Ok(RgbCellCapacity {
            cell_type_hash: hash.to_string(),
            amount_atomic: 1_000_000,
            fractional_nanos: Some(0),
        })
    }

    async fn map_bitcoin_utxo(&self, txid: &str, vout: u32) -> Result<BitcoinUtxoBinding, String> {
        let txid = txid.trim();
        if txid.is_empty() {
            return Err("txid required".to_string());
        }
        Ok(BitcoinUtxoBinding {
            txid: txid.to_string(),
            vout,
            ckb_cell_id: Some(format!("sim-cell-{txid}-{vout}")),
        })
    }
}

#[async_trait]
impl FiberNodeRpc for LiveFnnClient {
    async fn call_fnn_rpc(&self, payload: Value) -> Result<Value, String> {
        LiveFnnClient::call_fnn_rpc(self, payload).await
    }

    async fn payment_is_success(&self, payment_hash: &str) -> Result<bool, String> {
        LiveFnnClient::payment_is_success(self, payment_hash).await
    }

    async fn list_channels(&self) -> Result<Vec<MeshChannelState>, String> {
        let result = self
            .call_rpc(
                "list_channels",
                serde_json::json!([{
                    "include_closed": false,
                    "only_pending": false
                }]),
            )
            .await?;

        Self::decode_list_channels(result)
    }

    async fn node_pubkey(&self) -> Result<String, String> {
        if let Some(cached) = self.local_pubkey.read().await.clone() {
            return Ok(cached);
        }

        let result = self.call_rpc("node_info", serde_json::json!([])).await?;
        let info: NodeInfoResult = serde_json::from_value(result)
            .map_err(|e| format!("decode node_info: {e}"))?;
        *self.local_pubkey.write().await = Some(info.pubkey.clone());
        Ok(info.pubkey)
    }

    async fn connect_peer(&self, peer_public_key: &str) -> Result<(), String> {
        self.call_rpc(
            "connect_peer",
            serde_json::json!([{
                "pubkey": peer_public_key,
                "save": true
            }]),
        )
        .await?;
        Ok(())
    }

    async fn open_channel(&self, peer_public_key: &str, amount: u64) -> Result<(), String> {
        let _ = self.connect_peer(peer_public_key).await;
        self.call_rpc(
            "open_channel",
            serde_json::json!([{
                "pubkey": peer_public_key,
                "funding_amount": shannons_to_hex(amount),
                "public": true
            }]),
        )
        .await?;
        Ok(())
    }

    async fn close_channel(&self, peer_public_key: &str, force: bool) -> Result<(), String> {
        let channels = self.list_channels().await?;
        let channel_id = channels
            .iter()
            .find(|c| c.peer_pubkey.as_deref() == Some(peer_public_key))
            .and_then(|c| c.channel_id.clone())
            .ok_or_else(|| format!("no channel_id for pubkey {peer_public_key}"))?;

        self.call_rpc(
            "shutdown_channel",
            serde_json::json!([{
                "channel_id": channel_id,
                "force": force
            }]),
        )
        .await?;
        Ok(())
    }

    async fn send_keysend_payment(
        &self,
        target_pubkey: &str,
        amount: u64,
    ) -> Result<PaymentResult, String> {
        self.send_htlc_payment(SendHtlcPaymentArgs {
            target_pubkey: target_pubkey.to_string(),
            amount_shannons: amount,
            payment_hash: None,
            route_hops: Vec::new(),
            cltv_expiry_delta: DEFAULT_CLTV_EXPIRY_DELTA_MS,
        })
        .await
    }

    async fn send_htlc_payment(&self, args: SendHtlcPaymentArgs) -> Result<PaymentResult, String> {
        let params = LiveFnnClient::build_send_payment_params(&args);
        let result = self
            .call_rpc("send_payment", serde_json::json!([params]))
            .await?;

        let payment_hash = result
            .get("payment_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let status = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Created")
            .to_string();
        let fee_shannons = Self::parse_balance_shannons(result.get("fee"));

        if payment_hash.is_empty() {
            return Err(format!("send_payment missing payment_hash: {result}"));
        }

        self.poll_payment_result(&payment_hash, status, fee_shannons)
            .await
    }

    async fn get_rgb_cell_capacity(&self, cell_type_hash: &str) -> Result<RgbCellCapacity, String> {
        LiveFnnClient::query_rgb_cell_capacity(self, cell_type_hash).await
    }

    async fn map_bitcoin_utxo(&self, txid: &str, vout: u32) -> Result<BitcoinUtxoBinding, String> {
        LiveFnnClient::query_bitcoin_utxo_binding(self, txid, vout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::chord_peer;
    use serde_json::json;

    #[test]
    fn build_send_payment_params_maps_multihop_htlc_fields() {
        let params = LiveFnnClient::build_send_payment_params(&SendHtlcPaymentArgs {
            target_pubkey: "03dest".into(),
            amount_shannons: 1_000_000,
            payment_hash: Some("aabb".into()),
            route_hops: vec!["03hop1".into(), "03dest".into()],
            cltv_expiry_delta: 14_400_000,
        });

        assert_eq!(params["target_pubkey"], "03dest");
        assert_eq!(params["payment_hash"], "0xaabb");
        assert_eq!(params["keysend"], false);
        assert_eq!(params["final_tlc_expiry_delta"], 14_400_000);
        assert_eq!(params["trampoline_hops"], json!(["03hop1"]));
    }

    #[test]
    fn pubkey_to_peer_stub_uses_digits_when_present() {
        assert_eq!(LiveFnnClient::pubkey_to_peer_stub("0xabc1234"), 1234);
    }

    #[test]
    fn pubkey_to_peer_stub_hashes_when_no_digits() {
        let id = LiveFnnClient::pubkey_to_peer_stub("ckt1qwerty");
        assert!((1..=1024).contains(&id));
    }

    #[test]
    fn channel_is_active_accepts_fnn_v08_state_name() {
        assert!(LiveFnnClient::channel_is_active(&json!({"state_name": "ChannelReady"})));
        assert!(LiveFnnClient::channel_is_active(&json!("ChannelReady")));
        assert!(!LiveFnnClient::channel_is_active(&json!({"state_name": "AwaitingTxSignatures"})));
    }

    #[test]
    fn decode_list_channels_maps_balances_from_hex() {
        let result = json!({
            "channels": [{
                "channel_id": "0xabc",
                "pubkey": "0xpeer99",
                "enabled": true,
                "state": {"state_name": "ChannelReady"},
                "local_balance": "0x2faf080",
                "remote_balance": "0xbebc200"
            }]
        });

        let channels = LiveFnnClient::decode_list_channels(result).unwrap();
        assert_eq!(channels[0].local_balance_shannons, 50_000_000);
        assert_eq!(channels[0].remote_balance_shannons, 200_000_000);
    }

    #[test]
    fn decode_list_channels_maps_channel_id_and_state_name() {
        let result = json!({
            "channels": [{
                "channel_id": "0xabc",
                "pubkey": "0xpeer99",
                "enabled": true,
                "state": {"state_name": "ChannelReady"}
            }]
        });

        let channels = LiveFnnClient::decode_list_channels(result).unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].peer_id, 99);
        assert!(channels[0].is_active);
        assert_eq!(channels[0].channel_id.as_deref(), Some("0xabc"));
    }

    #[tokio::test]
    async fn simulated_neighbors_use_correct_chord_peer() {
        let client = SimulatedFnnClient::new(1);
        let peers = mesh_neighbor_ids(1, 1024);
        assert_eq!(peers[2], chord_peer(1, 1024));
        let channels = client.list_channels().await.unwrap();
        assert_eq!(channels.len(), 3);
    }

    #[tokio::test]
    async fn simulated_keysend_moves_outbound_balance() {
        let client = SimulatedFnnClient::new(44);
        let target = agent_fnn_pubkey(45);
        let before = client
            .list_channels()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.peer_id == 45)
            .expect("ring neighbor channel")
            .local_balance_shannons;

        let result = client
            .send_keysend_payment(&target, 1_000_000)
            .await
            .expect("simulated keysend");
        assert_eq!(result.status, "Success");

        let after = client
            .list_channels()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.peer_id == 45)
            .expect("ring neighbor channel")
            .local_balance_shannons;
        assert!(after < before);
    }

    #[tokio::test]
    async fn simulated_open_channel_reactivates_broken_link() {
        let client = SimulatedFnnClient::new(44);
        let peer_id = client.list_channels().await.unwrap()[0].peer_id;
        let pubkey = crate::agent_fnn_pubkey(peer_id);

        client.close_channel(&pubkey, false).await.unwrap();
        assert!(
            client
                .list_channels()
                .await
                .unwrap()
                .iter()
                .any(|c| c.peer_id == peer_id && !c.is_active)
        );

        client.open_channel(&pubkey, 1).await.unwrap();
        assert!(
            client
                .list_channels()
                .await
                .unwrap()
                .iter()
                .any(|c| c.peer_id == peer_id && c.is_active)
        );
    }
}
