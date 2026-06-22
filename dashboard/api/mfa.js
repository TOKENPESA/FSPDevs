import {
  EDGE_NODES_STORAGE_KEY,
  MFA_HEALTH_URL,
  MFA_ROUTE_URL,
  MFA_SIMULATION_URL,
  RING_MAX,
} from "../config.js";
import { formatGridDim, formatShannons } from "../format.js";
import { layoutNodes } from "../canvas/layout.js";
import { hideTooltip } from "../canvas/tooltip.js";
import {
  settlePaymentTransfer,
  startPaymentTransfer,
} from "../events/payment.js";
import { updateHubPanel } from "../events/liquidity.js";
import {
  logEvent,
  markDirty,
  setNodeArrays,
  state,
  touchCommEdge,
  touchCommNode,
} from "../state.js";
import { buildMeshEdges } from "../topology.js";

const sizeLabel = document.getElementById("size-label");
const edgeCountInput = document.getElementById("edge-node-count");
const sizeInput = document.getElementById("network-size");
const gridDimLabel = document.getElementById("grid-dim-label");
const meshSubtitle = document.getElementById("mesh-subtitle");
const fleetHint = document.getElementById("fleet-hint");
const routeMaxLabels = document.querySelectorAll("[id^='route-max-label']");
const routeSourceInput = document.getElementById("route-source");
const routeDestInput = document.getElementById("route-dest");
const routeAmountInput = document.getElementById("route-amount");
const metricRoute = document.getElementById("metric-route");
const metricLive = document.getElementById("metric-live");

export function eventWithinSimulation(payload) {
  const n = state.networkSize;
  if (payload.node != null && payload.node > n) return false;
  if (payload.source != null && payload.source > n) return false;
  if (payload.destination != null && payload.destination > n) return false;
  if (payload.removed != null && payload.removed > n) return false;
  if (payload.added != null && payload.added > n) return false;
  return true;
}

export function updateEdgeNodeUi(n) {
  sizeLabel.textContent = String(n);
  edgeCountInput.value = String(n);
  sizeInput.value = String(n);
  gridDimLabel.textContent = formatGridDim(n);
  meshSubtitle.textContent = `${n}-node lattice mesh (${formatGridDim(n)} grid) · live MFA stream`;
  fleetHint.innerHTML =
    n >= RING_MAX
      ? "Full ring — fleet: <code>.\\spawn-mesh-fleet.ps1</code>"
      : `Only FA-1…${n} on graph & routing — fleet: <code>.\\spawn-mesh-fleet.ps1 -To ${n}</code>`;
  routeSourceInput.max = String(n);
  routeDestInput.max = String(n);
  routeMaxLabels.forEach((el) => {
    el.textContent = String(n);
  });
  if (Number(routeSourceInput.value) > n) routeSourceInput.value = "1";
  if (Number(routeDestInput.value) > n) routeDestInput.value = String(Math.min(n, Math.max(2, Math.floor(n / 2))));
}

function pruneSimulationState(newSize) {
  state.dead.forEach((id) => {
    if (id > newSize) state.dead.delete(id);
  });
  state.healed.forEach((id) => {
    if (id > newSize) state.healed.delete(id);
  });
  state.healLinks = state.healLinks.filter((l) => l.from <= newSize && l.to <= newSize);
  state.activeRoute = state.activeRoute.filter((id) => id <= newSize);
  for (const id of [...state.comm.nodes.keys()]) {
    if (id > newSize) state.comm.nodes.delete(id);
  }
  for (const id of [...state.comm.balances.keys()]) {
    if (id > newSize) state.comm.balances.delete(id);
  }
  for (const id of [...state.comm.mfaLinks.keys()]) {
    if (id > newSize) state.comm.mfaLinks.delete(id);
  }
  for (const [key, meta] of [...state.comm.edges.entries()]) {
    if (meta.a > newSize || meta.b > newSize) state.comm.edges.delete(key);
  }
  if (state.hoveredNode > newSize) {
    state.hoveredNode = null;
    hideTooltip();
  }
  if (state.paymentTransfer?.clearTimer) {
    clearTimeout(state.paymentTransfer.clearTimer);
  }
  state.paymentTransfer = null;
}

export function applyEdgeNodeCount(raw, { skipSync = false, skipStorage = false } = {}) {
  const newSize = Math.max(1, Math.min(RING_MAX, Math.round(Number(raw) || 1)));
  state.networkSize = newSize;
  updateEdgeNodeUi(newSize);
  pruneSimulationState(newSize);

  setNodeArrays(new Float32Array(newSize + 1), new Float32Array(newSize + 1));
  layoutNodes();
  buildMeshEdges();
  metricLive.textContent = String(state.comm.nodes.size);
  markDirty();

  if (!skipStorage) {
    try {
      localStorage.setItem(EDGE_NODES_STORAGE_KEY, String(newSize));
    } catch {
      /* ignore */
    }
  }
  if (!skipSync) {
    syncSimulationToMfa(newSize);
  }
}

export async function syncSimulationToMfa(n) {
  try {
    const res = await fetch(MFA_SIMULATION_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ edge_nodes: n }),
    });
    if (res.ok) {
      logEvent(`Simulation size → ${n} edge node(s) (synced to MFA)`, "heal");
    }
  } catch {
    logEvent(`Set ${n} edge nodes locally (MFA offline — sync on Connect)`, "warn");
  }
}

export async function loadSimulationFromMfa() {
  try {
    const res = await fetch(MFA_SIMULATION_URL, { mode: "cors" });
    if (!res.ok) return;
    const data = await res.json();
    if (data.edge_nodes) {
      applyEdgeNodeCount(data.edge_nodes, { skipSync: true });
    }
  } catch {
    /* MFA may be busy */
  }
}

export async function fetchHubHealth(timeoutMs = 4000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(MFA_HEALTH_URL, { mode: "cors", signal: controller.signal });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    if (data.hub) {
      state.hub.rpcUrl = data.hub.rpc_url ?? "—";
      const shannons = data.hub.funding_allocation_shannons;
      state.hub.fundingShannons = shannons != null
        ? `${Number(shannons).toLocaleString()} shannons / channel`
        : "—";
      state.hub.sidecarAlerts = data.hub.sidecar_balance_alerts ?? "see env FIBER_AGENT_HUB_CHANNEL_FUNDING";
    }
    if (data.simulation_edge_nodes) {
      applyEdgeNodeCount(data.simulation_edge_nodes, { skipSync: true });
    }
    updateHubPanel();
    logEvent(`Hub linked: ${state.hub.rpcUrl} · ${state.hub.fundingShannons}`, "heal");
    return true;
  } catch (err) {
    state.liquidity.lastEvent = `Hub health check failed: ${err.message}`;
    updateHubPanel();
    return false;
  } finally {
    clearTimeout(timer);
  }
}

function readRouteNode(input) {
  const n = Number.parseInt(input.value, 10);
  if (!Number.isInteger(n) || n < 1 || n > state.networkSize) {
    throw new Error(`Node must be an integer between 1 and ${state.networkSize}`);
  }
  return n;
}

export async function routeTransaction() {
  let source;
  let destination;
  let amount;

  try {
    source = readRouteNode(routeSourceInput);
    destination = readRouteNode(routeDestInput);
    amount = Number.parseInt(routeAmountInput.value, 10);
    if (!Number.isFinite(amount) || amount < 1) {
      throw new Error("Amount must be a positive integer");
    }
  } catch (err) {
    logEvent(err.message, "warn");
    return;
  }

  logEvent(`Routing & paying FA-${source} → FA-${destination} (${amount} shannons)…`);

  try {
    const res = await fetch(MFA_ROUTE_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source,
        destination,
        amount_shannons: amount,
        active_network_limit: state.networkSize,
        execute: true,
      }),
    });

    const data = await res.json();

    if (data.status === "ROUTE_FOUND" && Array.isArray(data.path) && data.path.length >= 2) {
      startPaymentTransfer(data.path, source, destination, amount);
      for (let i = 0; i < data.path.length - 1; i++) {
        touchCommEdge(data.path[i], data.path[i + 1], "mesh");
      }
      touchCommNode(data.path[0], [data.path[1]], 1);
      touchCommEdge(source, destination, "mesh");
      const pathLabel = data.path.map((id) => `FA-${id}`).join(" → ");
      logEvent(`ROUTE_FOUND (${data.execution_latency_ms}ms): ${pathLabel}`, "heal");

      if (data.payment_status === "SUCCESS") {
        const fee = data.payment_fee_shannons ?? 0;
        settlePaymentTransfer(true, fee);
        const hash = data.payment_hash ? ` · ${data.payment_hash.slice(0, 18)}…` : "";
        logEvent(
          `PAYMENT OK: FA-${source} → FA-${destination} · ${formatShannons(amount)} · fee ${formatShannons(fee)}${hash}`,
          "heal",
        );
      } else if (data.payment_status === "SKIPPED_NO_SIDECAR") {
        settlePaymentTransfer(false);
        logEvent(
          `Route only — start sidecar: AGENT_ID=${source} (payment not sent)`,
          "warn",
        );
      } else if (data.payment_status === "FAILED" || data.payment_status === "TIMEOUT") {
        settlePaymentTransfer(false);
        logEvent(
          `Payment ${data.payment_status}: ${data.payment_error || "see MFA logs"}`,
          "warn",
        );
      } else if (data.payment_status && data.payment_status !== "SKIPPED") {
        logEvent(`Payment pending: ${data.payment_status}`, "heal");
      }
      markDirty();
    } else {
      if (state.paymentTransfer) {
        settlePaymentTransfer(false);
      }
      state.activeRoute = [];
      metricRoute.textContent = "—";
      logEvent(`Route failed: ${data.status || res.status}`, "warn");
      markDirty();
    }
  } catch (err) {
    if (state.paymentTransfer) {
      settlePaymentTransfer(false);
    }
    state.activeRoute = [];
    metricRoute.textContent = "—";
    logEvent(`Route request failed — is MFA running on :1025? (${err.message})`, "warn");
    markDirty();
  }
}
