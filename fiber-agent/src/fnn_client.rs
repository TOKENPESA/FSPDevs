use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::mesh::{mesh_neighbor_ids, shannons_to_hex, DEFAULT_OPEN_CHANNEL_SHANNONS};
use crate::{agent_fnn_pubkey, peer_id_from_agent_pubkey, MeshChannelState};

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
pub trait FiberNodeRpc {
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
}

pub struct LiveFnnClient {
    rpc_url: String,
    http_client: Client,
    local_pubkey: RwLock<Option<String>>,
}

pub struct SimulatedFnnClient {
    agent_id: u16,
    channels: Arc<RwLock<Vec<MeshChannelState>>>,
}

impl LiveFnnClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            http_client: Client::new(),
            local_pubkey: RwLock::new(None),
        }
    }

    async fn call_rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let payload = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: method.to_string(),
            params,
        };

        let response = self
            .http_client
            .post(&self.rpc_url)
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

    pub(crate) fn decode_list_channels(result: Value) -> Result<Vec<MeshChannelState>, String> {
        let parsed: ListChannelsRpcResult = serde_json::from_value(result)
            .map_err(|e| format!("Failed to decode list_channels result: {e}"))?;

        Ok(parsed
            .channels
            .into_iter()
            .map(|ch| {
                let peer_id = Self::pubkey_to_peer_stub(&ch.pubkey);
                MeshChannelState {
                    peer_id,
                    nonce: 1,
                    consecutive_failures: 0,
                    is_active: ch.enabled && Self::channel_is_active(&ch.state),
                    peer_pubkey: Some(ch.pubkey),
                    channel_id: ch.channel_id,
                    local_balance_shannons: Self::parse_balance_shannons(ch.local_balance.as_ref()),
                    remote_balance_shannons: Self::parse_balance_shannons(ch.remote_balance.as_ref()),
                }
            })
            .collect())
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
                }
            })
            .collect();

        Self {
            agent_id,
            channels: Arc::new(RwLock::new(channels)),
        }
    }
}

#[async_trait]
impl FiberNodeRpc for SimulatedFnnClient {
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

        let payment_hash = format!(
            "sim-pay-{}-{}-{}",
            self.agent_id,
            peer_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        Ok(PaymentResult {
            payment_hash,
            status: "Success".to_string(),
            fee_shannons,
        })
    }
}

#[async_trait]
impl FiberNodeRpc for LiveFnnClient {
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
        let result = self
            .call_rpc(
                "send_payment",
                serde_json::json!([{
                    "target_pubkey": target_pubkey,
                    "amount": shannons_to_hex(amount),
                    "keysend": true
                }]),
            )
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

        let mut final_status = status;
        let mut final_fee = fee_shannons;
        for _ in 0..20 {
            if final_status.eq_ignore_ascii_case("Success")
                || final_status.eq_ignore_ascii_case("Failed")
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
            payment_hash,
            status: final_status,
            fee_shannons: final_fee,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::chord_peer;
    use serde_json::json;

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
