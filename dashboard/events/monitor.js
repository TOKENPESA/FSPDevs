/** @typedef {import('../types.js').MonitorEnvelope} MonitorEnvelope */

import { COMM_TTL_MS, MFA_SIMULATION_URL, mfaApiToken, mfaAuthedUrl, mfaAuthHeaders } from "../config.js";
import { connectWebSocketWithTimeout, fetchWithTimeout } from "../fetch-timeout.js";
import { createLogger } from "../logger.js";
import { formatShannons } from "../format.js";
import { $input } from "../dom.js";
import { errorMessage } from "../../packages/fsp-ui-types/errors.js";
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
const mfaWsInput = $input("mfa-ws");
const log = createLogger("monitor");

/** @type {ReturnType<typeof setTimeout> | null} */
let monitorReconnectTimer = null;

function monitorWsUrl() {
  const raw = mfaWsInput?.value?.trim() ?? "ws://127.0.0.1:1025/ws/monitor";
  return mfaAuthedUrl(raw);
}

function dashboardOriginHint() {
  const origin = window.location.origin;
  if (window.location.protocol === "file:") {
    return "Open via http://127.0.0.1:8088 (npm run serve:mfa), not as a local file.";
  }
  const host = window.location.hostname;
  if (host !== "127.0.0.1" && host !== "localhost" && host !== "[::1]") {
    return `Page origin ${origin} is not loopback — MFA blocks cross-origin monitor WS. Use http://127.0.0.1:8088`;
  }
  return "";
}

async function verifyMfaApiToken() {
  try {
    const res = await fetchWithTimeout(
      MFA_SIMULATION_URL,
      {
        method: "POST",
        headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
        body: JSON.stringify({ edge_nodes: 16 }),
      },
      5000,
    );
    if (res.status === 401) {
      return {
        ok: false,
        message:
          "API token rejected — clear localStorage key fspdevs-mfa-api-token or set MFA_API_TOKEN=fspdevs-local-api-devonly when starting MFA.",
      };
    }
    return { ok: true };
  } catch (error) {
    return { ok: false, message: errorMessage(error) };
  }
}

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
/** @param {MonitorEnvelope} envelope @returns {boolean} */
export function handleVersionedMonitorEnvelope(envelope) {
  if (!envelope.schema_version || !envelope.event) {
    log.warn("skipped unversioned monitor frame");
    return false;
  }

  const { event: eventType, payload = {} } = envelope;

  switch (eventType) {
    case "COPILOT_PREDICTION_ALERT":
      appendLogEvent(
        `📈 [COPILOT] Node FA-${payload.node} running low! Exhaustion expected in ${Math.round(Number(payload.seconds_remaining ?? 0))}s`,
        "warn",
      );
      updateNodeVisualState(Number(payload.node), "WARN_DRAIN");
      break;

    case "REQUITY_INJECTION":
      appendLogEvent(
        `💰 [LIQUIDITY] Core Hub injected capacity into FA-${payload.node} via [${payload.vault ?? "hub"}]`,
        "liquidity",
      );
      updateNodeVisualState(Number(payload.node), "INJECTING");
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
        `🔄 [ENTERPRISE CLEARING] Cross-hub intent swap: ${formatShannons(payload.amount ?? 0)} shannons`,
        "liquidity",
      );
      break;

    case "BALANCE_DEPLETED":
    case "TELEMETRY_BALANCE_DEPLETED":
      appendLogEvent(
        `🚨 [TELEMETRY] BalanceDepleted on FA-${payload.node ?? payload.agent_id ?? "?"} · enterprise refuel triggered`,
        "warn",
      );
      break;

    default:
      logEvent(`Monitor event: ${eventType}`, "heal");
  }

  bumpMonitorMetrics();
  markDirty();
  return true;
}

/**
 * Lightweight WebSocket bootstrap for embedders (schema-versioned frames only).
 * The full demo shell should use `connectMonitor()` for MFA health checks and legacy events.
 * @param {string} wsUrl
 */
export async function initializeMonitorSocket(wsUrl) {
  const socket = await connectWebSocketWithTimeout(wsUrl);
  state.ws = socket;

  socket.onmessage = (event) => {
    try {
      const envelope = JSON.parse(String(event.data));
      if (!handleVersionedMonitorEnvelope(envelope)) {
        handleMonitorMessage(event.data);
      }
    } catch (err) {
      log.error("monitor payload parse failed", err);
    }
  };

  socket.onclose = () => {
    logEvent("Monitor disconnected — retrying…", "warn");
    setTimeout(() => void initializeMonitorSocket(wsUrl), 3000);
  };

  return socket;
}

/** @param {unknown} raw */
export function handleMonitorMessage(raw) {
  let payload;
  try {
    payload = JSON.parse(String(raw));
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
    state.dead.add(Number(payload.removed));
    state.healed.add(Number(payload.added));
    state.healLinks.push({ from: Number(payload.node), to: Number(payload.added) });
    if (state.healLinks.length > 8) state.healLinks.shift();
    touchCommNode(Number(payload.node), [Number(payload.added)], 1);
    touchCommEdge(Number(payload.node), Number(payload.added), "heal");
    logEvent(
      `MESH_HEAL: FA-${payload.node} swapped FA-${payload.removed} → FA-${payload.added}`,
      "heal",
    );
    markDirty();
  } else if (payload.event === "MESH_HEARTBEAT") {
    state.dead.delete(Number(payload.node));
    const neighbors = Array.isArray(payload.neighbors) ? payload.neighbors.map(Number) : [];
    const channelCount = Number(payload.channels ?? neighbors.length ?? 0);
    const balances = {
      outbound: payload.local_capacity_shannons ?? payload.outbound_shannons ?? null,
      inbound: payload.inbound_shannons ?? null,
    };
    touchCommNode(Number(payload.node), neighbors, channelCount, balances);
    if (balances.outbound != null || balances.inbound != null) {
      setNodeLedger(
        Number(payload.node),
        Number(balances.outbound ?? resolveNodeBalances(Number(payload.node))?.outbound ?? 0),
        Number(balances.inbound ?? resolveNodeBalances(Number(payload.node))?.inbound ?? 0),
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
  } else if (payload.status === "ALERT_BALANCE_DEPLETED" || payload.event === "ALERT_BALANCE_DEPLETED") {
    const node = payload.node ?? payload.agent_id ?? "?";
    logEvent(
      `🚨 [TELEMETRY] ALERT_BALANCE_DEPLETED · FA-${node} · EnterpriseClearinghouse refuel pipeline`,
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

  const originHint = dashboardOriginHint();
  if (originHint) {
    logEvent(originHint, "warn");
    if (connStatus) connStatus.textContent = "Wrong URL";
    return;
  }

  const url = monitorWsUrl();
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

  const tokenCheck = await verifyMfaApiToken();
  if (!tokenCheck.ok) {
    logEvent(tokenCheck.message ?? "Token check failed", "warn");
    if (connStatus) connStatus.textContent = "Auth error";
    scheduleMonitorReconnect();
    return;
  }

  let ws;
  try {
    ws = await connectWebSocketWithTimeout(url);
  } catch (error) {
    log.error("monitor websocket connect failed", error);
    if (connStatus) connStatus.textContent = "WS error";
    logEvent(
      `WebSocket failed — MFA on :1025, dashboard on :8088, token ${mfaApiToken().slice(0, 6)}…`,
      "warn",
    );
    scheduleMonitorReconnect();
    return;
  }
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
    logEvent(
      "WebSocket error — use http://127.0.0.1:8088, MFA on :1025, dev token fspdevs-local-api-devonly",
      "warn",
    );
  };
  ws.onmessage = (ev) => handleMonitorMessage(ev.data);
}
