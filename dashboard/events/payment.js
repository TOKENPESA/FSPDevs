import {
  PAYMENT_SETTLE_DISPLAY_MS,
  PAYMENT_TRAVEL_CAP,
} from "../config.js";
import { formatShannons } from "../format.js";
import {
  logEvent,
  markDirty,
  nodeX,
  nodeY,
  resolveNodeBalances,
  setNodeLedger,
  state,
  touchCommEdge,
} from "../state.js";
import { nodeStatus, updateTooltip } from "../canvas/tooltip.js";

const metricRoute = document.getElementById("metric-route");
const metricHover = document.getElementById("metric-hover");

export function paymentTravelDurationMs(pathLen) {
  return 1200 + Math.max(0, pathLen - 1) * 450;
}

export function pathPointAtProgress(path, progress) {
  if (!path || path.length < 2) return null;
  const totalSegs = path.length - 1;
  const travel = Math.min(1, Math.max(0, progress)) * totalSegs;
  const segIdx = Math.min(Math.floor(travel), totalSegs - 1);
  const segT = travel - segIdx;
  const from = path[segIdx];
  const to = path[segIdx + 1];
  return {
    x: nodeX[from] + (nodeX[to] - nodeX[from]) * segT,
    y: nodeY[from] + (nodeY[to] - nodeY[from]) * segT,
    from,
    to,
    atDestination: progress >= 1,
  };
}

function applyDashboardPaymentBalances(source, dest, amount, fee = 0) {
  const srcPrev = resolveNodeBalances(source);
  const dstPrev = resolveNodeBalances(dest);
  const srcOut = srcPrev?.outbound ?? amount * 3;
  const dstIn = dstPrev?.inbound ?? 0;
  const srcIn = srcPrev?.inbound ?? 0;
  const dstOut = dstPrev?.outbound ?? 0;
  const totalDebit = amount + fee;
  const newSrcOut = Math.max(0, srcOut - totalDebit);

  setNodeLedger(source, newSrcOut, srcIn);
  setNodeLedger(dest, dstOut, dstIn + amount);
  const now = Date.now();
  state.comm.received.set(dest, { amount, at: now, from: source });
  state.comm.sent.set(source, { amount, fee, totalDebit, at: now, to: dest });
  touchCommEdge(source, dest, "mesh");

  logEvent(
    `Ledger: FA-${source} out ${formatShannons(srcOut)} → ${formatShannons(newSrcOut)} (−${formatShannons(totalDebit)})`,
    "heal",
  );
}

export function startPaymentTransfer(path, source, destination, amount) {
  if (state.paymentTransfer?.clearTimer) {
    clearTimeout(state.paymentTransfer.clearTimer);
  }
  state.paymentTransfer = {
    path: [...path],
    source,
    destination,
    amount,
    progress: 0,
    phase: "traveling",
    startedAt: performance.now(),
    settledAt: null,
    clearTimer: null,
  };
  state.activeRoute = [...path];
  metricRoute.textContent = "in flight…";
  markDirty();
}

export function settlePaymentTransfer(success, fee = 0) {
  const pt = state.paymentTransfer;
  if (!pt || pt.phase === "settled" || pt.phase === "failed") return;

  const finish = () => {
    const current = state.paymentTransfer;
    if (!current || current !== pt) return;
    current.phase = success ? "settled" : "failed";
    current.settledAt = performance.now();
    if (success) {
      current.progress = 1;
      applyDashboardPaymentBalances(current.source, current.destination, current.amount, fee);
      metricRoute.textContent = `delivered → FA-${current.destination}`;
      logEvent(
        `Funds arrived at FA-${current.destination} · +${formatShannons(current.amount)}`,
        "heal",
      );
    } else {
      metricRoute.textContent = "payment failed";
    }

    current.clearTimer = setTimeout(() => {
      if (state.paymentTransfer === current) {
        state.paymentTransfer = null;
        state.activeRoute = [];
        metricRoute.textContent = "—";
        markDirty();
      }
    }, PAYMENT_SETTLE_DISPLAY_MS);

    markDirty();

    if (state.hoveredNode === current.source || state.hoveredNode === current.destination) {
      const ptr = state.lastPointer;
      if (ptr && state.hoveredNode) {
        updateTooltip(state.hoveredNode, ptr.clientX, ptr.clientY);
        const st = nodeStatus(state.hoveredNode);
        metricHover.textContent = `FA-${state.hoveredNode} · ${st.label}`;
      }
    }
  };

  const elapsed = performance.now() - pt.startedAt;
  const minTravelMs = 750;
  if (success && elapsed < minTravelMs) {
    setTimeout(finish, minTravelMs - elapsed);
    return;
  }
  finish();
}

export function tickPaymentTransfer(now) {
  const pt = state.paymentTransfer;
  if (!pt || pt.phase !== "traveling") return;

  const elapsed = now - pt.startedAt;
  const duration = paymentTravelDurationMs(pt.path.length);
  pt.progress = Math.min(PAYMENT_TRAVEL_CAP, elapsed / duration);
  markDirty();
}

export function handlePaymentEvent(payload) {
  if (payload.event === "PAYMENT_STARTED") {
    if (Array.isArray(payload.path) && payload.path.length >= 2) {
      startPaymentTransfer(
        payload.path,
        payload.source,
        payload.destination,
        payload.amount_shannons ?? 0,
      );
    }
    logEvent(
      `PAYMENT started: FA-${payload.source} → FA-${payload.destination} · ${formatShannons(payload.amount_shannons)}`,
      "heal",
    );
    return true;
  }
  if (payload.event === "PAYMENT_EXECUTED") {
    const fee = payload.fee_shannons != null ? ` · fee ${formatShannons(payload.fee_shannons)}` : "";
    logEvent(
      `PAYMENT settled: FA-${payload.source} → FA-${payload.destination} · ${formatShannons(payload.amount_shannons)}${fee}`,
      "heal",
    );
    if (!state.paymentTransfer && Array.isArray(payload.path) && payload.path.length >= 2) {
      startPaymentTransfer(
        payload.path,
        payload.source,
        payload.destination,
        payload.amount_shannons ?? 0,
      );
    }
    settlePaymentTransfer(true, payload.fee_shannons ?? 0);
    return true;
  }
  if (payload.event === "PAYMENT_FAILED") {
    if (state.paymentTransfer) {
      settlePaymentTransfer(false);
    }
    logEvent(
      `PAYMENT failed: FA-${payload.source} → FA-${payload.destination} — ${payload.reason || "unknown"}`,
      "warn",
    );
    return true;
  }
  return false;
}
