//! LUME — institutional yielding protocol with RGB++ order-book matching.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use mesh_core::network::PeerModulePacket;
use mesh_core::types::{AssetCapacity, L2Asset};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use crate::identity::resolve_agent_secret_key;
use crate::module_system::SidecarModule;
use crate::peer_packet::sign_peer_module_packet;

#[derive(Debug, Clone)]
struct RgbOrder {
    agent_id: u16,
    asset: L2Asset,
    amount_atomic: u64,
    price_bps: u64,
    htlc_timeout_secs: u64,
}

#[derive(Debug, Default)]
struct OrderBook {
    bids: BTreeMap<(u64, u16), RgbOrder>,
    asks: BTreeMap<(u64, u16), RgbOrder>,
}

pub struct LumeYieldingModule {
    agent_id: u16,
    book: Arc<Mutex<OrderBook>>,
    outbound_tx: Option<mpsc::Sender<PeerModulePacket>>,
}

impl LumeYieldingModule {
    pub fn new(agent_id: u16) -> Self {
        Self {
            agent_id,
            book: Arc::new(Mutex::new(OrderBook::default())),
            outbound_tx: None,
        }
    }

    fn parse_rgb_asset(payload: &Value) -> Result<L2Asset, String> {
        let kind = payload
            .get("asset_kind")
            .and_then(Value::as_str)
            .ok_or("asset_kind required")?;
        let id = payload
            .get("asset_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind.to_ascii_uppercase().as_str() {
            "CKB" => Ok(L2Asset::CkbNative),
            "RUSD" => Ok(L2Asset::RusdStablecoin),
            "RGB++" | "RGBPP" => {
                if id.is_empty() {
                    return Err("RGB++ requires asset_id (cell type hash)".to_string());
                }
                Ok(L2Asset::RgbPlusPlus(id.to_string()))
            }
            "UDT" => {
                if id.is_empty() {
                    return Err("UDT requires asset_id (script hash)".to_string());
                }
                Ok(L2Asset::UDT(id.to_string()))
            }
            other => Err(format!("unsupported asset_kind '{other}'")),
        }
    }

    async fn try_match_orders(&self) -> Result<(), String> {
        let mut book = self.book.lock().await;
        let Some(((_, _), best_bid)) = book.bids.iter().next_back().map(|(k, v)| (*k, v.clone())) else {
            return Ok(());
        };
        let Some(((_, _), best_ask)) = book.asks.iter().next().map(|(k, v)| (*k, v.clone())) else {
            return Ok(());
        };

        if best_bid.price_bps < best_ask.price_bps {
            return Ok(());
        }

        if best_bid.amount_atomic == 0 || best_ask.amount_atomic == 0 {
            return Ok(());
        }

        let match_amount = best_bid.amount_atomic.min(best_ask.amount_atomic);
        let lock_secs = best_bid.htlc_timeout_secs.max(best_ask.htlc_timeout_secs);

        book.bids.remove(&(best_bid.price_bps, best_bid.agent_id));
        book.asks.remove(&(best_ask.price_bps, best_ask.agent_id));

        let outbound = self.outbound_tx.clone().ok_or("outbound channel not wired")?;
        drop(book);

        let capacities = serde_json::to_value([
            AssetCapacity::new(best_bid.asset.clone(), match_amount),
            AssetCapacity::new(best_ask.asset.clone(), match_amount),
        ])
        .map_err(|err| format!("LUME capacity serialization failed: {err}"))?;

        let packet = PeerModulePacket {
            source_agent_id: self.agent_id,
            target_agent_id: best_ask.agent_id,
            target_module: "lume_yielding".to_string(),
            method: "dictate_htlc_lock".to_string(),
            payload: json!({
                "counterparty_agent": best_bid.agent_id,
                "capacities": capacities,
                "htlc_timeout_secs": lock_secs,
                "spread_bps": best_ask.price_bps.saturating_sub(best_bid.price_bps),
            }),
            signature: None,
        };

        let secret = resolve_agent_secret_key(self.agent_id)?;
        let signed = sign_peer_module_packet(packet, &secret)?;
        outbound
            .send(signed)
            .await
            .map_err(|err| format!("LUME outbound HTLC dispatch failed: {err}"))?;

        log::info!(
            "📈 [LUME] Matched RGB++ bid FA-{} vs ask FA-{} for {match_amount} atoms (HTLC {lock_secs}s)",
            best_bid.agent_id,
            best_ask.agent_id
        );
        Ok(())
    }
}

#[async_trait]
impl SidecarModule for LumeYieldingModule {
    fn module_name(&self) -> &'static str {
        "lume_yielding"
    }

    fn local_agent_id(&self) -> u16 {
        self.agent_id
    }

    fn set_outbound_channel(&mut self, tx: mpsc::Sender<PeerModulePacket>) {
        self.outbound_tx = Some(tx);
    }

    async fn initialize(&mut self) -> Result<(), String> {
        log::info!("🏦 [LUME] Institutional yielding order book online.");
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String> {
        match method {
            "get_order_book_depth" => {
                let book = self.book.lock().await;
                Ok(json!({
                    "bids": book.bids.len(),
                    "asks": book.asks.len(),
                }))
            }
            "submit_local_bid" | "submit_local_ask" => {
                let side = if method == "submit_local_bid" { "bid" } else { "ask" };
                let price_bps = payload
                    .get("price_bps")
                    .and_then(Value::as_u64)
                    .ok_or("price_bps required")?;
                let amount_atomic = payload
                    .get("amount_atomic")
                    .and_then(Value::as_u64)
                    .ok_or("amount_atomic required")?;
                let asset = Self::parse_rgb_asset(&payload)?;
                let order = RgbOrder {
                    agent_id: self.agent_id,
                    asset,
                    amount_atomic,
                    price_bps,
                    htlc_timeout_secs: payload
                        .get("htlc_timeout_secs")
                        .and_then(Value::as_u64)
                        .unwrap_or(300),
                };
                let mut book = self.book.lock().await;
                if side == "bid" {
                    book.bids.insert((price_bps, self.agent_id), order);
                } else {
                    book.asks.insert((price_bps, self.agent_id), order);
                }
                drop(book);
                self.try_match_orders().await?;
                Ok(json!({ "status": "accepted", "side": side }))
            }
            _ => Err(format!("Method '{method}' unsupported on lume_yielding module.")),
        }
    }

    async fn handle_peer_message(
        &self,
        source_agent_id: u16,
        method: &str,
        payload: Value,
    ) -> Result<(), String> {
        match method {
            "telemetry_stream" | "submit_rgb_bid" => {
                let price_bps = payload
                    .get("price_bps")
                    .and_then(Value::as_u64)
                    .ok_or("price_bps required")?;
                let amount_atomic = payload
                    .get("amount_atomic")
                    .and_then(Value::as_u64)
                    .ok_or("amount_atomic required")?;
                let asset = Self::parse_rgb_asset(&payload)?;
                let order = RgbOrder {
                    agent_id: source_agent_id,
                    asset,
                    amount_atomic,
                    price_bps,
                    htlc_timeout_secs: payload
                        .get("htlc_timeout_secs")
                        .and_then(Value::as_u64)
                        .unwrap_or(300),
                };
                self.book
                    .lock()
                    .await
                    .bids
                    .insert((price_bps, source_agent_id), order);
                self.try_match_orders().await
            }
            "submit_rgb_ask" => {
                let price_bps = payload
                    .get("price_bps")
                    .and_then(Value::as_u64)
                    .ok_or("price_bps required")?;
                let amount_atomic = payload
                    .get("amount_atomic")
                    .and_then(Value::as_u64)
                    .ok_or("amount_atomic required")?;
                let asset = Self::parse_rgb_asset(&payload)?;
                let order = RgbOrder {
                    agent_id: source_agent_id,
                    asset,
                    amount_atomic,
                    price_bps,
                    htlc_timeout_secs: payload
                        .get("htlc_timeout_secs")
                        .and_then(Value::as_u64)
                        .unwrap_or(300),
                };
                self.book
                    .lock()
                    .await
                    .asks
                    .insert((price_bps, source_agent_id), order);
                self.try_match_orders().await
            }
            _ => Err(format!(
                "Peer method '{method}' unsupported on lume_yielding module."
            )),
        }
    }
}
