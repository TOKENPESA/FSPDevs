import { COMM_TTL_MS } from "../config.js";
import { formatShannons } from "../format.js";
import {
  appendLogEvent,
  logEvent,
  markDirty,
  resolveNodeBalances,
  setNodeLedger,
  state,
  touchCommEdge,
  touchCommNode,
  updateNodeVisualState,
} from "../state.js";
import { handlePaymentEvent } from "./payment.js";
import { handleLiquidityEvent } from "./liquidity.js";
import {
  eventWithinSimulation,
  fetchHubHealth,
  loadSimulationFromMfa,
} from "../api/mfa.js";

const connStatus = document.getElementById("conn-status");
const connDot = document.getElementById("conn-dot");
const metricTick = document.getElementById("metric-tick");
const metricLive = document.getElementById("metric-live");
const metricDead = document.getElementById("metric-dead");
const metricHeals = document.getElementById("metric-heals");
const mfaWsInput = document.getElementById("mfa-ws");

let monitorReconnectTimer = null;

function bumpMonitorMetrics() {
  state.tick += 1;
  if (metricTick) metricTick.textContent = String(state.tick);
  if (metricLive) metricLive.textContent = String(state.comm.nodes.size);
  if (metricDead) metricDead.textContent = String(state.dead.size);
  if (metricHeals) metricHeals.textContent = String(state.healCount);
  if (state.playing) {
    markDirty();
  }
}

export function scheduleMonitorReconnect() {
  if (monitorReconnectTimer) return;
  monitorReconnectTimer = setTimeout(() => {
    monitorReconnectTimer = null;
    if (state.ws?.readyState === WebSocket.OPEN) return;
    connectMonitor();
  }, 5000);
}

/** Handles schema-versioned monitor envelopes from MFA (`MonitorEnvelope`). */
export function handleVersionedMonitorEnvelope(envelope) {
  if (!envelope.schema_version || !envelope.event) {
    console.warn(
      "⚠️ [SKIPPED FRAME] Received unversioned or non-standard payload structure.",
    );
    return false;
  }

  const { event: eventType, payload = {} } = envelope;

  switch (eventType) {
    case "COPILOT_PREDICTION_ALERT":
      appendLogEvent(
        `📈 [COPILOT] Node FA-${payload.node} running low! Exhaustion expected in ${Math.round(payload.seconds_remaining ?? 0)}s`,
        "warn",
      );
      updateNodeVisualState(payload.node, "WARN_DRAIN");
      break;

    case "REQUITY_INJECTION":
      appendLogEvent(
        `💰 [LIQUIDITY] Core Hub injected capacity into FA-${payload.node} via [${payload.vault ?? "hub"}]`,
        "liquidity",
      );
      updateNodeVisualState(payload.node, "INJECTING");
      handleLiquidityEvent({
        event: "LIQUIDITY_INJECTION",
        node: payload.node,
        amount_shannons: payload.amount_shannons ?? 0,
      });
      break;

    case "TOPOLOGY_SYNC":
      appendLogEvent(
        `🔗 [TOPOLOGY] Graph v${payload.version ?? "?"} · ${payload.updated_channels_count ?? 0} channel update(s)`,
        "heal",
      );
      break;

    case "INTENT_SWAP_SUCCESS":
      appendLogEvent(
        `🔄 [SWAP] Atomic off-chain swap accomplished: ${formatShannons(payload.amount ?? 0)}`,
        "liquidity",
      );
      break;

    default:
      console.log(`ℹ️ [MONITOR] Processing versioned channel event: ${eventType}`);
  }

  bumpMonitorMetrics();
  markDirty();
  return true;
}

/**
 * Lightweight WebSocket bootstrap for embedders (schema-versioned frames only).
 * The full demo shell should use `connectMonitor()` for MFA health checks and legacy events.
 */
export function initializeMonitorSocket(wsUrl) {
  const socket = new WebSocket(wsUrl);
  state.ws = socket;

  socket.onmessage = (event) => {
    try {
      const envelope = JSON.parse(event.data);
      if (!handleVersionedMonitorEnvelope(envelope)) {
        handleMonitorMessage(event.data);
      }
    } catch (err) {
      console.error(
        "❌ [PARSING FAULT] Failed to deconstruct websocket event payload:",
        err,
      );
    }
  };

  socket.onclose = () => {
    console.log("🔌 [MONITOR DISCONNECTED] Retrying socket handshake loop...");
    setTimeout(() => initializeMonitorSocket(wsUrl), 3000);
  };

  return socket;
}

export function handleMonitorMessage(raw) {
  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    logEvent(`Ignored: ${raw}`);
    return;
  }

  if (payload.schema_version && payload.event) {
    handleVersionedMonitorEnvelope(payload);
    return;
  }

  if (payload.event !== "SYS_LAG" && !eventWithinSimulation(payload)) {
    return;
  }
  bumpMonitorMetrics();
  if (payload.event === "MESH_HEAL") {
    state.healCount += 1;
    state.dead.add(payload.removed);
    state.healed.add(payload.added);
    state.healLinks.push({ from: payload.node, to: payload.added });
    if (state.healLinks.length > 8) state.healLinks.shift();
    touchCommNode(payload.node, [payload.added], 1);
    touchCommEdge(payload.node, payload.added, "heal");
    logEvent(
      `MESH_HEAL: FA-${payload.node} swapped FA-${payload.removed} → FA-${payload.added}`,
      "heal",
    );
    markDirty();
  } else if (payload.event === "MESH_HEARTBEAT") {
    state.dead.delete(payload.node);
    const neighbors = payload.neighbors ?? [];
    const channelCount = payload.channels ?? neighbors.length ?? 0;
    const balances = {
      outbound: payload.outbound_shannons ?? null,
      inbound: payload.inbound_shannons ?? null,
    };
    touchCommNode(payload.node, neighbors, channelCount, balances);
    if (balances.outbound != null || balances.inbound != null) {
      setNodeLedger(
        payload.node,
        balances.outbound ?? resolveNodeBalances(payload.node)?.outbound ?? 0,
        balances.inbound ?? resolveNodeBalances(payload.node)?.inbound ?? 0,
      );
    }
    logEvent(
      `HEARTBEAT: FA-${payload.node} · ${channelCount} ch · out ${formatShannons(balances.outbound)} · in ${formatShannons(balances.inbound)}`,
      "heal",
    );
    markDirty();
  } else if (payload.event === "SYS_LAG") {
    logEvent(
      `⚠️ System Congestion: Skipped ${payload.skipped} frames due to processing lag.`,
      "warn",
    );
  } else if (!handlePaymentEvent(payload) && !handleLiquidityEvent(payload)) {
    logEvent(JSON.stringify(payload));
  }
}

export async function connectMonitor() {
  if (state.ws) {
    state.ws.close();
    state.ws = null;
  }
  if (window.location.protocol === "file:") {
    logEvent(
      "Open the dashboard via http://127.0.0.1:8088 (npm run serve:dashboard), not as a local file",
      "warn",
    );
    if (connStatus) connStatus.textContent = "Wrong URL";
    return;
  }
  const url = mfaWsInput?.value.trim() ?? "ws://127.0.0.1:1025/ws/monitor";
  if (!url.startsWith("ws://127.0.0.1") && !url.startsWith("ws://localhost")) {
    logEvent("Monitor URL must be ws://127.0.0.1 or ws://localhost", "warn");
    return;
  }

  if (connStatus) connStatus.textContent = "Connecting…";
  connDot?.classList.remove("connected");

  const hubOk = await fetchHubHealth(10000);
  if (!hubOk) {
    logEvent(
      "MFA not responding on :1025 — start MFA first (cd fnn-testnet; .\\start-live-mfa.ps1), then Connect",
      "warn",
    );
    if (connStatus) connStatus.textContent = "MFA offline";
    scheduleMonitorReconnect();
    return;
  }

  const ws = new WebSocket(url);
  state.ws = ws;
  ws.onopen = () => {
    if (connStatus) connStatus.textContent = "Connected";
    connDot?.classList.add("connected");
    logEvent(`Monitor connected: ${url}`, "heal");
    loadSimulationFromMfa();
  };
  ws.onclose = (ev) => {
    if (connStatus) connStatus.textContent = "Disconnected";
    connDot?.classList.remove("connected");
    const hint =
      ev.code === 1006
        ? " — is MFA running? Use http://127.0.0.1:8088 for the dashboard"
        : "";
    logEvent(`Monitor disconnected (code ${ev.code})${hint}`, "warn");
    scheduleMonitorReconnect();
  };
  ws.onerror = () => {
    if (connStatus) connStatus.textContent = "WS error";
    logEvent("WebSocket error — confirm MFA is on :1025 and dashboard is served on :8088", "warn");
  };
  ws.onmessage = (ev) => handleMonitorMessage(ev.data);
}
