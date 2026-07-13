//! Autonomous AI market maker — headless liquidity provision loop.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::fnn_client::FiberNodeRpc;
use crate::module_system::SidecarModule;
use crate::storage::AgentDb;

const MM_POLL_INTERVAL_SECS: u64 = 30;
const MM_MIN_SPREAD_BPS: u64 = 25;

pub struct AutonomousMarketMakerModule {
    agent_id: u16,
    db: Arc<AgentDb>,
    fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    shutdown_tx: Option<watch::Sender<bool>>,
    worker: Option<JoinHandle<()>>,
}

impl AutonomousMarketMakerModule {
    pub fn new(
        agent_id: u16,
        db: Arc<AgentDb>,
        fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    ) -> Self {
        Self {
            agent_id,
            db,
            fnn_client,
            shutdown_tx: None,
            worker: None,
        }
    }

    async fn market_maker_loop(
        agent_id: u16,
        db: Arc<AgentDb>,
        fnn: Arc<dyn FiberNodeRpc + Send + Sync>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(MM_POLL_INTERVAL_SECS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                _ = interval.tick() => {
                    if let Err(err) = Self::rebalance_limit_orders(agent_id, &db, fnn.as_ref()).await {
                        log::warn!("🤖 [AMM] FA-{agent_id} rebalance skipped: {err}");
                    }
                }
            }
        }
        log::info!("🤖 [AMM] FA-{agent_id} headless market maker stopped.");
    }

    async fn rebalance_limit_orders(
        agent_id: u16,
        db: &AgentDb,
        fnn: &dyn FiberNodeRpc,
    ) -> Result<(), String> {
        let channels = db.list_cached_channels()?;
        if channels.is_empty() {
            return Err("no cached channel state — poll list_channels first".to_string());
        }

        for channel in channels.iter().filter(|ch| ch.is_ready) {
            let mid = channel
                .local_balance_shannons
                .saturating_add(channel.remote_balance_shannons)
                / 2;
            if mid < 100_000 {
                continue;
            }

            let bid_amount = mid.saturating_mul(100 - MM_MIN_SPREAD_BPS) / 100;
            let ask_amount = mid.saturating_mul(100 + MM_MIN_SPREAD_BPS) / 100;

            let rpc_payload = json!({
                "jsonrpc": "2.0",
                "method": "place_limit_order",
                "params": {
                    "channel_id": channel.channel_id,
                    "peer_pubkey": channel.peer_pubkey,
                    "bid_amount_shannons": bid_amount,
                    "ask_amount_shannons": ask_amount,
                    "spread_bps": MM_MIN_SPREAD_BPS,
                    "agent_id": agent_id,
                },
                "id": 1
            });

            fnn.call_fnn_rpc(rpc_payload).await?;
            log::info!(
                "🤖 [AMM] FA-{agent_id} posted limit orders on {} (bid {bid_amount} / ask {ask_amount})",
                channel.channel_id
            );
        }
        Ok(())
    }
}

#[async_trait]
impl SidecarModule for AutonomousMarketMakerModule {
    fn module_name(&self) -> &'static str {
        "fiber_agent_swarm"
    }

    fn local_agent_id(&self) -> u16 {
        self.agent_id
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let agent_id = self.agent_id;
        let db = Arc::clone(&self.db);
        let fnn = Arc::clone(&self.fnn_client);

        let worker = tokio::spawn(async move {
            Self::market_maker_loop(agent_id, db, fnn, shutdown_rx).await;
        });

        self.shutdown_tx = Some(shutdown_tx);
        self.worker = Some(worker);
        log::info!("🤖 [AMM] Autonomous market maker swarm spawned for FA-{}.", self.agent_id);
        Ok(())
    }

    async fn handle_rpc_command(&self, method: &str, _payload: Value) -> Result<Value, String> {
        match method {
            "get_swarm_status" => Ok(json!({
                "agent_id": self.agent_id,
                "poll_interval_secs": MM_POLL_INTERVAL_SECS,
                "spread_bps": MM_MIN_SPREAD_BPS,
                "worker_active": self.worker.is_some(),
            })),
            "force_rebalance" => {
                Self::rebalance_limit_orders(self.agent_id, &self.db, self.fnn_client.as_ref())
                    .await?;
                Ok(json!({ "status": "rebalanced" }))
            }
            _ => Err(format!(
                "Method '{method}' unsupported on fiber_agent_swarm module."
            )),
        }
    }
}

impl Drop for AutonomousMarketMakerModule {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }
}
