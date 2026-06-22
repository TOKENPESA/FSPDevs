import { state, markDirty, logEvent } from "../state.js";
import { formatShannons } from "../format.js";

const hubRpcEl = document.getElementById("hub-rpc");
const hubFundingEl = document.getElementById("hub-funding");
const hubAlertsEl = document.getElementById("hub-alerts");
const hubLastEventEl = document.getElementById("hub-last-event");
const metricLiquidityOk = document.getElementById("metric-liquidity-ok");
const metricLiquidityFaucet = document.getElementById("metric-liquidity-faucet");
const metricLiquidityFlight = document.getElementById("metric-liquidity-flight");
const metricLiquidityFail = document.getElementById("metric-liquidity-fail");

export function updateHubPanel() {
  hubRpcEl.textContent = state.hub.rpcUrl;
  hubFundingEl.textContent = state.hub.fundingShannons;
  hubAlertsEl.textContent = state.hub.sidecarAlerts;
  hubLastEventEl.textContent = state.liquidity.lastEvent;
  metricLiquidityOk.textContent = String(state.liquidity.injections);
  metricLiquidityFaucet.textContent = String(state.liquidity.faucetHints);
  metricLiquidityFlight.textContent = String(state.liquidity.inFlight);
  metricLiquidityFail.textContent = String(state.liquidity.failed);
}

export function markLiquidityNode(node, status) {
  if (!node) return;
  state.liquidity.byNode.set(node, { status, at: Date.now() });
  if (state.liquidity.byNode.size > 64) {
    const oldest = [...state.liquidity.byNode.entries()].sort((a, b) => a[1].at - b[1].at)[0];
    if (oldest) state.liquidity.byNode.delete(oldest[0]);
  }
}

export function handleLiquidityEvent(payload) {
  const node = payload.node;
  const ts = new Date().toLocaleTimeString();

  switch (payload.event) {
    case "LIQUIDITY_ENGAGED":
      state.liquidity.inFlight += 1;
      state.liquidity.lastEvent = `${ts} · FA-${node} funding engaged`;
      markLiquidityNode(node, "engaged");
      logEvent(`LIQUIDITY engaged for FA-${node} (hub provisioning)`, "liquidity");
      break;
    case "LIQUIDITY_STARTED":
      state.liquidity.lastEvent = `${ts} · FA-${node} connect_peer + open_channel…`;
      markLiquidityNode(node, "started");
      logEvent(`LIQUIDITY started for FA-${node}`, "liquidity");
      break;
    case "LIQUIDITY_INJECTION":
      state.liquidity.inFlight = Math.max(0, state.liquidity.inFlight - 1);
      state.liquidity.injections += 1;
      {
        const funded = formatShannons(payload.amount_shannons ?? 0);
        state.liquidity.lastEvent = `${ts} · FA-${node} funded ${funded}`;
        markLiquidityNode(node, "funded");
        logEvent(`💰 LIQUIDITY OK: FA-${node} +${funded}`, "liquidity");
      }
      break;
    case "LIQUIDITY_NEEDS_FAUCET":
      state.liquidity.inFlight = Math.max(0, state.liquidity.inFlight - 1);
      state.liquidity.faucetHints += 1;
      state.liquidity.lastEvent = `${ts} · FA-${node} needs testnet faucet (same FNN as hub)`;
      markLiquidityNode(node, "faucet");
      logEvent(
        `💧 FA-${node}: fund via get-ckb-address.ps1 → faucet.nervos.org (cannot channel to self)`,
        "warn",
      );
      break;
    case "LIQUIDITY_FAILED":
      state.liquidity.inFlight = Math.max(0, state.liquidity.inFlight - 1);
      state.liquidity.failed += 1;
      state.liquidity.lastEvent = `${ts} · FA-${node} failed: ${payload.reason ?? "unknown"}`;
      markLiquidityNode(node, "failed");
      logEvent(`❌ LIQUIDITY failed FA-${node}: ${payload.reason ?? "error"}`, "dead");
      break;
    default:
      return false;
  }
  updateHubPanel();
  markDirty();
  return true;
}
