/** @typedef {import('../types.js').PathPoint} PathPoint */

import {
  PAYMENT_SETTLE_DISPLAY_MS,
  PAYMENT_TRAVEL_CAP,
} from "../config.js";
import { safeUserMessage } from "../dom-security.js";
import { formatShannons } from "../format.js";
import { createLogger } from "../logger.js";
import {
  logEvent,
  markDirty,
  markRouteBlacklist,
  nodeX,
  nodeY,
  resolveNodeBalances,
  setNodeLedger,
  state,
  touchCommEdge,
} from "../state.js";
import { nodeStatus, updateTooltip } from "../canvas/tooltip.js";

const log = createLogger("payment-events");
const metricRoute = document.getElementById("metric-route");
const metricHover = document.getElementById("metric-hover");

/** @param {number} pathLen @returns {number} */
export function paymentTravelDurationMs(pathLen) {
  return 1200 + Math.max(0, pathLen - 1) * 450;
}

/** @param {number[]} path @param {number} progress @returns {PathPoint | null} */
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

/** @param {number} source @param {number} dest @param {number} amount @param {number} [fee] */
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

/** @param {number[]} path @param {number} source @param {number} destination @param {number} amount */
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
    bottleneck: null,
    failReason: null,
  };
  state.activeRoute = [...path];
  if (metricRoute) metricRoute.textContent = "in flight…";
  markDirty();
}

/**
 * Detect pathfind / TemporaryNodeFailure feedback and extract blacklist hops.
 * @param {Record<string, unknown>} payload
 * @returns {{ hops: Array<{ a: number, b: number, bottleneck: number }>, reason: string } | null}
 */
export function extractPathfindBlacklist(payload) {
  const reason = safeUserMessage(payload.reason ?? payload.error ?? payload.payment_error, "");
  if (!reason) return null;

  const isPathFail = /TemporaryNodeFailure|no path|PathFind|max_fee_amount is too low|Failed to build route/i
    .test(reason);
  if (!isPathFail) return null;

  /** @type {number[]} */
  const path = Array.isArray(payload.path)
    ? payload.path.map(Number).filter((n) => Number.isFinite(n) && n > 0)
    : [];
  const source = Number(payload.source);
  const destination = Number(payload.destination);

  /** @type {number[]} */
  const mentioned = [];
  for (const match of reason.matchAll(/FA-(\d+)/gi)) {
    const id = Number(match[1]);
    if (Number.isFinite(id) && id > 0) mentioned.push(id);
  }

  /** @type {Array<{ a: number, b: number, bottleneck: number }>} */
  const hops = [];
  if (path.length >= 2) {
    for (let i = 0; i < path.length - 1; i++) {
      hops.push({
        a: path[i],
        b: path[i + 1],
        bottleneck: path[i + 1],
      });
    }
  } else if (Number.isFinite(source) && Number.isFinite(destination) && source > 0 && destination > 0) {
    hops.push({ a: source, b: destination, bottleneck: destination });
  }

  if (mentioned.length > 0 && hops.length > 0) {
    const bottleneck = mentioned[mentioned.length - 1];
    for (const hop of hops) {
      if (hop.a === bottleneck || hop.b === bottleneck) {
        hop.bottleneck = bottleneck;
      }
    }
  }

  // Prefer the first intermediate as bottleneck for TemporaryNodeFailure.
  if (/TemporaryNodeFailure/i.test(reason) && path.length >= 2) {
    const idx = Math.min(1, path.length - 1);
    const bottleneck = path[idx];
    for (const hop of hops) {
      if (hop.a === path[0] && hop.b === path[1]) {
        hop.bottleneck = bottleneck;
      }
    }
  }

  return hops.length > 0 ? { hops, reason } : { hops: [], reason };
}

/** @param {boolean} success @param {number} [fee] @param {{ bottleneck?: number | null, failReason?: string | null }} [failMeta] */
export function settlePaymentTransfer(success, fee = 0, failMeta = {}) {
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
      if (metricRoute) metricRoute.textContent = `delivered → FA-${current.destination}`;
      logEvent(
        `Funds arrived at FA-${current.destination} · +${formatShannons(current.amount)}`,
        "heal",
      );
    } else {
      current.bottleneck = failMeta.bottleneck ?? current.bottleneck ?? current.path[1] ?? current.destination;
      current.failReason = failMeta.failReason ?? current.failReason;
      if (metricRoute) {
        metricRoute.textContent = current.bottleneck
          ? `blocked @ FA-${current.bottleneck}`
          : "payment failed";
      }
    }

    current.clearTimer = setTimeout(() => {
      if (state.paymentTransfer === current) {
        state.paymentTransfer = null;
        state.activeRoute = [];
        if (metricRoute) metricRoute.textContent = "—";
        markDirty();
      }
    }, PAYMENT_SETTLE_DISPLAY_MS);

    markDirty();

    if (state.hoveredNode === current.source || state.hoveredNode === current.destination) {
      const ptr = state.lastPointer;
      if (ptr && state.hoveredNode) {
        updateTooltip(state.hoveredNode, ptr.clientX, ptr.clientY);
        const st = nodeStatus(state.hoveredNode);
        if (metricHover) metricHover.textContent = `FA-${state.hoveredNode} · ${st.label}`;
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

/** @param {number} now */
export function tickPaymentTransfer(now) {
  const pt = state.paymentTransfer;
  if (!pt || pt.phase !== "traveling") return;

  const elapsed = now - pt.startedAt;
  const duration = paymentTravelDurationMs(pt.path.length);
  pt.progress = Math.min(PAYMENT_TRAVEL_CAP, elapsed / duration);
  markDirty();
}

/** @param {Record<string, unknown>} payload @returns {boolean} */
export function handlePaymentEvent(payload) {
  if (payload.event === "PAYMENT_STARTED") {
    if (Array.isArray(payload.path) && payload.path.length >= 2) {
      startPaymentTransfer(
        payload.path.map(Number),
        Number(payload.source),
        Number(payload.destination),
        Number(payload.amount_shannons ?? 0),
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
        payload.path.map(Number),
        Number(payload.source),
        Number(payload.destination),
        Number(payload.amount_shannons ?? 0),
      );
    }
    settlePaymentTransfer(true, Number(payload.fee_shannons ?? 0));
    return true;
  }
  if (payload.event === "PAYMENT_FAILED" || payload.event === "payment_failed") {
    const blacklist = extractPathfindBlacklist(payload);
    const safeReason = safeUserMessage(
      payload.reason ?? payload.error ?? "unknown",
      "payment failed",
    );

    if (blacklist) {
      for (const hop of blacklist.hops) {
        markRouteBlacklist(hop.a, hop.b, hop.bottleneck, blacklist.reason);
      }
      if (blacklist.hops.length > 0) {
        const nodes = [...new Set(blacklist.hops.flatMap((h) => [h.a, h.b, h.bottleneck]))];
        log.info("pathfind blacklist applied", { nodes, reason: blacklist.reason });
      }
    }

    if (!state.paymentTransfer && Array.isArray(payload.path) && payload.path.length >= 2) {
      startPaymentTransfer(
        payload.path.map(Number),
        Number(payload.source),
        Number(payload.destination),
        Number(payload.amount_shannons ?? 0),
      );
    }

    const bottleneck = blacklist?.hops[0]?.bottleneck
      ?? (Array.isArray(payload.path) && payload.path.length > 1
        ? Number(payload.path[1])
        : Number(payload.destination))
      ?? null;

    if (state.paymentTransfer) {
      settlePaymentTransfer(false, 0, { bottleneck, failReason: safeReason });
    }

    logEvent(
      `PAYMENT failed: FA-${payload.source} → FA-${payload.destination} — ${safeReason}`,
      "warn",
    );
    return true;
  }
  return false;
}
