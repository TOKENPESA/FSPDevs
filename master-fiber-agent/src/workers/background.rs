use crate::ui_events::send_ui_event;
use crate::config::{
    liquidity_copilot_cooldown_secs, liquidity_copilot_depletion_horizon_secs,
    liquidity_copilot_low_watermark_shannons, mesh_liquidity_copilot_enabled,
    HEARTBEAT_UI_MIN_INTERVAL_MS,
};
use crate::hub::{trigger_hub_liquidity_provisioning, DEFAULT_HUB_ASSET};
use crate::state::{AppState, PeerRegistry, SharedGraph};
use crate::telemetry::record_alert_dedupe;
use crate::types::MeshPulsePayload;
use axum::extract::ws::Message as AxumMessage;
use mesh_core::is_live_fiber_pubkey;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

/// Time-bounded per-agent locks to prevent duplicate hub funding RPCs.
pub struct FundingLockManager {
    locks: HashMap<u16, Instant>,
    lock_timeout: Duration,
}

impl FundingLockManager {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            locks: HashMap::new(),
            lock_timeout: Duration::from_secs(timeout_seconds),
        }
    }

    pub fn acquire_lock(&mut self, agent_id: u16) -> bool {
        let now = Instant::now();
        if let Some(acquired_at) = self.locks.get(&agent_id) {
            if now.duration_since(*acquired_at) < self.lock_timeout {
                return false;
            }
        }
        self.locks.insert(agent_id, now);
        true
    }

    pub fn release_lock(&mut self, agent_id: u16) {
        self.locks.remove(&agent_id);
    }
}

#[derive(Debug, Clone)]
struct AgentLiquiditySample {
    last_outbound_shannons: u64,
    sampled_at: Instant,
    drain_velocity_shannons_per_sec: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiquidityPrediction {
    pub prefund_recommended: bool,
    pub outbound_shannons: u64,
    pub min_graph_capacity: Option<u64>,
    pub drain_velocity_shannons_per_sec: f64,
    pub seconds_to_depletion: Option<f64>,
}

/// Tracks outbound drain velocity and predicts depletion before hard alerts fire.
pub struct LiquidityCopilot {
    samples: HashMap<u16, AgentLiquiditySample>,
    prefund_cooldown: HashMap<u16, Instant>,
    low_watermark_shannons: u64,
    depletion_horizon_secs: f64,
    cooldown: Duration,
}

impl LiquidityCopilot {
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
            prefund_cooldown: HashMap::new(),
            low_watermark_shannons: liquidity_copilot_low_watermark_shannons(),
            depletion_horizon_secs: liquidity_copilot_depletion_horizon_secs(),
            cooldown: Duration::from_secs(liquidity_copilot_cooldown_secs()),
        }
    }

    pub fn evaluate(
        &mut self,
        agent_id: u16,
        outbound_shannons: u64,
        min_graph_capacity: Option<u64>,
    ) -> LiquidityPrediction {
        let now = Instant::now();
        let sample = self
            .samples
            .entry(agent_id)
            .or_insert(AgentLiquiditySample {
                last_outbound_shannons: outbound_shannons,
                sampled_at: now,
                drain_velocity_shannons_per_sec: 0.0,
            });

        let elapsed = sample.sampled_at.elapsed().as_secs_f64().max(0.001);
        if outbound_shannons < sample.last_outbound_shannons && elapsed >= 0.05 {
            let instant_drain =
                (sample.last_outbound_shannons - outbound_shannons) as f64 / elapsed;
            sample.drain_velocity_shannons_per_sec =
                sample.drain_velocity_shannons_per_sec * 0.7 + instant_drain * 0.3;
        }

        sample.last_outbound_shannons = outbound_shannons;
        sample.sampled_at = now;

        let routing_floor = min_graph_capacity
            .unwrap_or(outbound_shannons)
            .min(outbound_shannons);

        let seconds_to_depletion = if sample.drain_velocity_shannons_per_sec > 0.0 {
            Some(routing_floor as f64 / sample.drain_velocity_shannons_per_sec)
        } else {
            None
        };

        let below_watermark = outbound_shannons <= self.low_watermark_shannons;
        let horizon_breach =
            seconds_to_depletion.is_some_and(|s| s <= self.depletion_horizon_secs);
        let graph_layer_tight =
            min_graph_capacity.is_some_and(|c| c <= self.low_watermark_shannons);

        let mut prefund_recommended = below_watermark || horizon_breach || graph_layer_tight;

        if prefund_recommended {
            if let Some(last) = self.prefund_cooldown.get(&agent_id) {
                if last.elapsed() < self.cooldown {
                    prefund_recommended = false;
                }
            }
        }

        if prefund_recommended {
            self.prefund_cooldown.insert(agent_id, now);
        }

        LiquidityPrediction {
            prefund_recommended,
            outbound_shannons,
            min_graph_capacity,
            drain_velocity_shannons_per_sec: sample.drain_velocity_shannons_per_sec,
            seconds_to_depletion,
        }
    }
}

async fn maybe_trigger_preemptive_liquidity(
    state: Arc<AppState>,
    agent_id: u16,
    target_pubkey: String,
    prediction: LiquidityPrediction,
) {
    let mut locks = state.active_funding_locks.write().await;
    if !locks.acquire_lock(agent_id) {
        return;
    }
    drop(locks);

    println!(
        "🔮 [LIQUIDITY COPILOT] Pre-emptive hub funding for FA-{agent_id} \
         (outbound {} shannons, ETA depletion {:?}s)",
        prediction.outbound_shannons, prediction.seconds_to_depletion
    );

    send_ui_event(
        &state.ui_broadcast,
        serde_json::json!({
            "event": "LIQUIDITY_COPILOT_ENGAGED",
            "node": agent_id,
            "outbound_shannons": prediction.outbound_shannons,
            "min_graph_capacity": prediction.min_graph_capacity,
            "drain_velocity_shannons_per_sec": prediction.drain_velocity_shannons_per_sec,
            "seconds_to_depletion": prediction.seconds_to_depletion,
        })
        .to_string(),
    );

    tokio::spawn(trigger_hub_liquidity_provisioning(
        agent_id,
        target_pubkey,
        state,
        DEFAULT_HUB_ASSET,
    ));
}

pub async fn run_liquidity_copilot_cycle(state: Arc<AppState>) {
    let edge_limit = state.simulation_edge_nodes.load(Ordering::Relaxed);
    let pubkeys = state.agent_fnn_pubkeys.read().await.clone();
    let outbound_snap = state.agent_liquidity_snap.read().await.clone();

    for agent_id in 1..=edge_limit {
        let Some(outbound) = outbound_snap.get(&agent_id).copied() else {
            continue;
        };
        let Some(pk) = pubkeys.get(&agent_id).cloned() else {
            continue;
        };
        if !is_live_fiber_pubkey(&pk) {
            continue;
        }

        let min_graph_capacity = state
            .graph
            .read()
            .await
            .min_active_outbound_capacity(agent_id);

        let prediction = {
            let mut copilot = state.liquidity_copilot.write().await;
            copilot.evaluate(agent_id, outbound, min_graph_capacity)
        };

        if prediction.prefund_recommended {
            maybe_trigger_preemptive_liquidity(
                state.clone(),
                agent_id,
                pk,
                prediction,
            )
            .await;
        }
    }
}

/// Predictive liquidity supervisor — evaluates mesh trends on a fixed interval.
pub async fn start_liquidity_copilot_worker(state: Arc<AppState>, interval_ms: u64) {
    if !mesh_liquidity_copilot_enabled() {
        println!("ℹ️ [LIQUIDITY COPILOT] Disabled (set MESH_LIQUIDITY_COPILOT=true to enable)");
        return;
    }

    println!(
        "🔮 [LIQUIDITY COPILOT] Supervisor online — evaluating mesh every {interval_ms}ms"
    );

    let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms.max(500)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        run_liquidity_copilot_cycle(state.clone()).await;
    }
}

pub async fn background_processor_worker(
    mut rx: mpsc::Receiver<MeshPulsePayload>,
    peers: PeerRegistry,
    graph: SharedGraph,
    ui_broadcast: broadcast::Sender<String>,
    state: Arc<AppState>,
) {
    let mut last_heartbeat_ui = Instant::now();
    let mut heartbeat_log_counter: u64 = 0;

    while let Some(telemetry) = rx.recv().await {
        let edge_limit = state.simulation_edge_nodes.load(Ordering::Relaxed);
        match telemetry.status.as_str() {
            "MESH_HEARTBEAT" => {
                if telemetry.agent > edge_limit {
                    continue;
                }

                {
                    let mut graph_write = graph.write().await;
                    graph_write.apply_heartbeat_liveness(
                        telemetry.agent,
                        &telemetry.active_mesh_neighbors,
                    );
                    if let Some(outbound) = telemetry.outbound_shannons {
                        graph_write.apply_heartbeat_liquidity(telemetry.agent, outbound);
                        state
                            .agent_liquidity_snap
                            .write()
                            .await
                            .insert(telemetry.agent, outbound);
                    }
                }

                if let Some(ref pk) = telemetry.fnn_pubkey_hex {
                    state
                        .agent_fnn_pubkeys
                        .write()
                        .await
                        .insert(telemetry.agent, pk.clone());
                }

                let agent_count = peers.read().await.len();
                let ui_due = last_heartbeat_ui.elapsed()
                    >= Duration::from_millis(HEARTBEAT_UI_MIN_INTERVAL_MS);
                if ui_due || agent_count <= 32 {
                    last_heartbeat_ui = Instant::now();
                    let ui_event = serde_json::json!({
                        "event": "MESH_HEARTBEAT",
                        "node": telemetry.agent,
                        "channels": telemetry.active_mesh_neighbors.len(),
                        "neighbors": telemetry.active_mesh_neighbors,
                        "outbound_shannons": telemetry.outbound_shannons,
                        "inbound_shannons": telemetry.inbound_shannons,
                    });
                    send_ui_event(&ui_broadcast, ui_event.to_string());
                }

                heartbeat_log_counter += 1;
                if agent_count <= 32 || heartbeat_log_counter.is_multiple_of(256) {
                    println!(
                        "💚 [MFA TELEMETRY] FA-{} heartbeat · {} channel(s) · {} agent WS(s) connected",
                        telemetry.agent,
                        telemetry.active_mesh_neighbors.len(),
                        agent_count
                    );
                }
            }
            "ALERT_MFA_NODE_DROPPED" => {
                if telemetry.agent > edge_limit || telemetry.report_target > edge_limit {
                    continue;
                }
                let dedupe_key = (telemetry.agent, telemetry.report_target);
                if !record_alert_dedupe(&state, dedupe_key).await {
                    continue;
                }

                let fallback_target = {
                    let mut graph_write = graph.write().await;
                    graph_write.isolate_faulty_node(telemetry.report_target);
                    graph_write.pick_healing_peer(telemetry.agent, telemetry.report_target)
                };

                println!(
                    "🧠 [MFA MESH BRAIN] Isolated FA-{}. Healing FA-{} → FA-{}",
                    telemetry.report_target, telemetry.agent, fallback_target
                );

                {
                    let registry = peers.read().await;
                    if let Some((agent_tx, _)) = registry.get(&telemetry.agent) {
                        let swap_cmd = serde_json::json!({
                            "command": "MESH_CHANNEL_HOT_SWAP",
                            "target_peer_id": telemetry.report_target,
                            "alternative_peer_id": fallback_target,
                        });
                        if agent_tx
                            .try_send(AxumMessage::Text(swap_cmd.to_string()))
                            .is_err()
                        {
                            eprintln!(
                                "❌ [HEALING FAILURE] Failed to route instruction to Node FA-{}.",
                                telemetry.agent
                            );
                        }
                    } else {
                        eprintln!(
                            "⚠️ [HEALING VOID] No active WebSocket for FA-{} — dashboard notified, hot-swap not delivered.",
                            telemetry.agent
                        );
                    }
                }

                let ui_event = serde_json::json!({
                    "event": "MESH_HEAL",
                    "node": telemetry.agent,
                    "removed": telemetry.report_target,
                    "added": fallback_target,
                });
                send_ui_event(&ui_broadcast, ui_event.to_string());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration as StdDuration;

    #[test]
    fn funding_lock_blocks_duplicate_acquire_within_timeout() {
        let mut mgr = FundingLockManager::new(60);
        assert!(mgr.acquire_lock(44));
        assert!(!mgr.acquire_lock(44));
    }

    #[test]
    fn funding_lock_releases_explicitly() {
        let mut mgr = FundingLockManager::new(60);
        assert!(mgr.acquire_lock(7));
        mgr.release_lock(7);
        assert!(mgr.acquire_lock(7));
    }

    #[test]
    fn funding_lock_expires_after_timeout() {
        let mut mgr = FundingLockManager::new(1);
        assert!(mgr.acquire_lock(99));
        thread::sleep(StdDuration::from_millis(1100));
        assert!(mgr.acquire_lock(99));
    }

    #[test]
    fn liquidity_copilot_predicts_depletion_from_drain_velocity() {
        let mut copilot = LiquidityCopilot {
            samples: HashMap::new(),
            prefund_cooldown: HashMap::new(),
            low_watermark_shannons: 10_000_000_000,
            depletion_horizon_secs: 120.0,
            cooldown: Duration::from_secs(300),
        };

        let first = copilot.evaluate(44, 20_000_000_000, Some(20_000_000_000));
        assert!(!first.prefund_recommended);

        thread::sleep(StdDuration::from_millis(50));
        let second = copilot.evaluate(44, 19_000_000_000, Some(19_000_000_000));
        assert!(second.drain_velocity_shannons_per_sec > 0.0);

        let mut fast_drain = LiquidityCopilot {
            samples: HashMap::new(),
            prefund_cooldown: HashMap::new(),
            low_watermark_shannons: 10_000_000_000,
            depletion_horizon_secs: 10_000.0,
            cooldown: Duration::from_secs(0),
        };
        fast_drain.evaluate(7, 100_000_000, Some(100_000_000));
        thread::sleep(StdDuration::from_millis(50));
        let urgent = fast_drain.evaluate(7, 50_000_000, Some(50_000_000));
        assert!(urgent.prefund_recommended);
        assert!(urgent.seconds_to_depletion.is_some());
    }

    #[test]
    fn liquidity_copilot_triggers_on_low_watermark() {
        let mut copilot = LiquidityCopilot {
            samples: HashMap::new(),
            prefund_cooldown: HashMap::new(),
            low_watermark_shannons: 5_000_000_000,
            depletion_horizon_secs: 120.0,
            cooldown: Duration::from_secs(0),
        };

        let prediction = copilot.evaluate(12, 4_000_000_000, Some(4_000_000_000));
        assert!(prediction.prefund_recommended);
    }
}
