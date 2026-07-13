import {
  EDGE_NODES_STORAGE_KEY,
  MFA_HEALTH_URL,
  MFA_ROUTE_URL,
  MFA_SIMULATION_URL,
  RING_MAX,
  mfaAuthHeaders,
} from "../config.js";
import { fetchWithTimeout } from "../fetch-timeout.js";
import { formatGridDim, formatShannons } from "../format.js";
import { layoutNodes } from "../canvas/layout.js";
import { hideTooltip } from "../canvas/tooltip.js";
import {
  settlePaymentTransfer,
  startPaymentTransfer,
} from "../events/payment.js";
import { updateHubPanel } from "../events/liquidity.js";
import { $, $input, requireInput, setText } from "../dom.js";
import { errorMessage } from "../../packages/fsp-ui-types/errors.js";
import {
  logEvent,
  markDirty,
  setNodeArrays,
  state,
  touchCommEdge,
  touchCommNode,
} from "../state.js";
import { buildMeshEdges } from "../topology.js";

const sizeLabel = $("size-label");
const edgeCountInput = $input("edge-node-count");
const sizeInput = $input("network-size");
const gridDimLabel = $("grid-dim-label");
const meshSubtitle = $("mesh-subtitle");
const fleetHint = $("fleet-hint");
const routeMaxLabels = document.querySelectorAll("[id^='route-max-label']");
const routeSourceInput = $input("route-source");
const routeDestInput = $input("route-dest");
const routeAmountInput = $input("route-amount");
const metricRoute = $("metric-route");
const metricLive = $("metric-live");

/** @param {Record<string, unknown>} payload @returns {boolean} */
export function eventWithinSimulation(payload) {
  const n = state.networkSize;
  const node = Number(payload.node);
  const source = Number(payload.source);
  const destination = Number(payload.destination);
  const removed = Number(payload.removed);
  const added = Number(payload.added);
  if (payload.node != null && node > n) return false;
  if (payload.source != null && source > n) return false;
  if (payload.destination != null && destination > n) return false;
  if (payload.removed != null && removed > n) return false;
  if (payload.added != null && added > n) return false;
  return true;
}

/** @param {number} n */
export function updateEdgeNodeUi(n) {
  setText(sizeLabel, String(n));
  if (edgeCountInput) edgeCountInput.value = String(n);
  if (sizeInput) sizeInput.value = String(n);
  setText(gridDimLabel, formatGridDim(n));
  setText(
    meshSubtitle,
    `${n}-node lattice mesh (${formatGridDim(n)} grid) · live MFA stream`,
  );
  if (fleetHint) {
    fleetHint.textContent =
      n >= RING_MAX
        ? "Full ring — fleet: .\\spawn-mesh-fleet.ps1"
        : `Only FA-1…${n} on graph & routing — fleet: .\\spawn-mesh-fleet.ps1 -To ${n}`;
  }
  if (routeSourceInput) routeSourceInput.max = String(n);
  if (routeDestInput) routeDestInput.max = String(n);
  routeMaxLabels.forEach((el) => {
    el.textContent = String(n);
  });
  if (routeSourceInput && Number(routeSourceInput.value) > n) routeSourceInput.value = "1";
  if (routeDestInput && Number(routeDestInput.value) > n) {
    routeDestInput.value = String(Math.min(n, Math.max(2, Math.floor(n / 2))));
  }
}

/** @param {number} newSize */
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
  if (state.hoveredNode != null && state.hoveredNode > newSize) {
    state.hoveredNode = null;
    hideTooltip();
  }
  if (state.paymentTransfer?.clearTimer) {
    clearTimeout(state.paymentTransfer.clearTimer);
  }
  state.paymentTransfer = null;
}

/**
 * @param {unknown} raw
 * @param {{ skipSync?: boolean, skipStorage?: boolean }} [opts]
 */
export function applyEdgeNodeCount(raw, { skipSync = false, skipStorage = false } = {}) {
  const newSize = Math.max(1, Math.min(RING_MAX, Math.round(Number(raw) || 1)));
  state.networkSize = newSize;
  updateEdgeNodeUi(newSize);
  pruneSimulationState(newSize);

  setNodeArrays(new Float32Array(newSize + 1), new Float32Array(newSize + 1));
  layoutNodes();
  buildMeshEdges();
  setText(metricLive, String(state.comm.nodes.size));
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

/** @param {number} n */
export async function syncSimulationToMfa(n) {
  try {
    const res = await fetchWithTimeout(MFA_SIMULATION_URL, {
      method: "POST",
      headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
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
    const res = await fetchWithTimeout(MFA_SIMULATION_URL, { mode: "cors" });
    if (!res.ok) return;
    const data = await res.json();
    if (data.edge_nodes) {
      applyEdgeNodeCount(data.edge_nodes, { skipSync: true });
    }
  } catch {
    /* MFA may be busy */
  }
}

/** @param {number} [timeoutMs] @returns {Promise<boolean>} */
export async function fetchHubHealth(timeoutMs = 4000) {
  try {
    const res = await fetchWithTimeout(MFA_HEALTH_URL, { mode: "cors" }, timeoutMs);
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
    state.liquidity.lastEvent = `Hub health check failed: ${errorMessage(err)}`;
    updateHubPanel();
    return false;
  }
}

/** @param {HTMLInputElement} input @returns {number} */
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
    const sourceInput = routeSourceInput ?? requireInput("route-source");
    const destInput = routeDestInput ?? requireInput("route-dest");
    const amountInput = routeAmountInput ?? requireInput("route-amount");
    source = readRouteNode(sourceInput);
    destination = readRouteNode(destInput);
    amount = Number.parseInt(amountInput.value, 10);
    if (!Number.isFinite(amount) || amount < 1) {
      throw new Error("Amount must be a positive integer");
    }
  } catch (err) {
    logEvent(errorMessage(err), "warn");
    return;
  }

  logEvent(`Routing & paying FA-${source} → FA-${destination} (${amount} shannons)…`);

  try {
    const res = await fetchWithTimeout(MFA_ROUTE_URL, {
      method: "POST",
      headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
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
      const pathLabel = data.path.map((/** @type {number} */ id) => `FA-${id}`).join(" → ");
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
      setText(metricRoute, "—");
      logEvent(`Route failed: ${data.status || res.status}`, "warn");
      markDirty();
    }
  } catch (err) {
    if (state.paymentTransfer) {
      settlePaymentTransfer(false);
    }
    state.activeRoute = [];
    setText(metricRoute, "—");
    logEvent(`Route request failed — is MFA running on :1025? (${errorMessage(err)})`, "warn");
    markDirty();
  }
}
