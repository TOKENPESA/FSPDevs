import {
  COMM_STYLE,
  COMM_TTL_MS,
  MFA_HUB,
  PATH_STYLES,
  PAYMENT_SETTLE_DISPLAY_MS,
} from "../config.js";
import { pathPointAtProgress } from "../events/payment.js";
import {
  isCommLive,
  meshEdges,
  nodeX,
  nodeY,
  pruneComm,
  pruneNodeVisualStates,
  state,
} from "../state.js";
import { meshPeerLinks } from "../topology.js";
import { canvas } from "./layout.js";

/** @typedef {import('../types.js').PathStyle} PathStyle */

const _ctx = canvas.getContext("2d");
if (!_ctx) throw new Error("2d canvas context unavailable");
/** @type {CanvasRenderingContext2D} */
const ctx = _ctx;

const metricTick = document.getElementById("metric-tick");
const metricLive = document.getElementById("metric-live");
const metricDead = document.getElementById("metric-dead");
const metricHeals = document.getElementById("metric-heals");

/** @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2 @param {PathStyle} style @param {number} now @param {boolean} [dead] */
function drawFlashLine(x1, y1, x2, y2, style, now, dead = false) {
  const pulse = dead ? 0.25 : 0.65 + 0.35 * Math.sin(now * 0.009 * state.speed);
  const offset = (now * style.speed * state.speed * 80) % 40;

  ctx.save();
  ctx.lineWidth = style.width;
  ctx.strokeStyle = dead ? "#666" : style.color;
  ctx.globalAlpha = pulse;
  ctx.setLineDash(style.dash);
  ctx.lineDashOffset = -offset;
  ctx.shadowColor = dead ? "transparent" : style.color;
  ctx.shadowBlur = dead ? 0 : 8;
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();

  if (!dead) {
    const t = (now * 0.0018 * state.speed) % 1;
    ctx.setLineDash([]);
    ctx.shadowBlur = 12;
    ctx.globalAlpha = 1;
    ctx.fillStyle = style.color;
    ctx.beginPath();
    ctx.arc(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, 3.2, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.restore();
}

/** @param {number} now */
function drawHoveredPaths(now) {
  const id = state.hoveredNode;
  if (!id) return;

  for (const { peer, kind } of meshPeerLinks(id)) {
    const dead = state.dead.has(id) || state.dead.has(peer);
    const pathKind = /** @type {keyof typeof PATH_STYLES} */ (kind);
    drawFlashLine(
      nodeX[id], nodeY[id],
      nodeX[peer], nodeY[peer],
      PATH_STYLES[pathKind], now, dead,
    );
  }

  drawFlashLine(
    nodeX[id], nodeY[id],
    MFA_HUB.x, MFA_HUB.y,
    PATH_STYLES.mfa, now, state.dead.has(id),
  );

  ctx.save();
  ctx.globalAlpha = 0.9;
  ctx.fillStyle = "#ffb347";
  ctx.shadowColor = "#ffb347";
  ctx.shadowBlur = 10;
  ctx.beginPath();
  ctx.arc(MFA_HUB.x, MFA_HUB.y, 5, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.font = "9px ui-monospace, monospace";
  ctx.shadowBlur = 0;
  ctx.fillText("MFA", MFA_HUB.x + 8, MFA_HUB.y + 3);
  ctx.restore();
}

/** @param {number} a @param {number} b */
function linkActive(a, b) {
  return !state.dead.has(a) && !state.dead.has(b);
}

/** @param {Array<[number, number]>} edges @param {{ color: string, width: number, alpha?: number, hover?: string }} style @param {number} now @param {Set<number> | null} [highlightSet] */
function drawMeshLayer(edges, style, now, highlightSet) {
  ctx.lineWidth = style.width;
  ctx.strokeStyle = style.color;
  ctx.globalAlpha = style.alpha ?? 1;
  ctx.beginPath();
  for (const [a, b] of edges) {
    if (!linkActive(a, b)) continue;
    if (highlightSet && (highlightSet.has(a) || highlightSet.has(b))) continue;
    ctx.moveTo(nodeX[a], nodeY[a]);
    ctx.lineTo(nodeX[b], nodeY[b]);
  }
  ctx.stroke();

  if (highlightSet) {
    ctx.strokeStyle = style.hover ?? style.color;
    ctx.globalAlpha = 0.95;
    ctx.beginPath();
    for (const [a, b] of edges) {
      if (!linkActive(a, b)) continue;
      if (!highlightSet.has(a) && !highlightSet.has(b)) continue;
      ctx.moveTo(nodeX[a], nodeY[a]);
      ctx.lineTo(nodeX[b], nodeY[b]);
    }
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}

/** @param {number} now */
export function drawCommLinks(now) {
  const t = Date.now();
  pruneComm(t);

  for (const meta of state.comm.edges.values()) {
    if (t - meta.at > COMM_TTL_MS) continue;
    const { a, b } = meta;
    if (state.dead.has(a) || state.dead.has(b)) continue;
    const style = (COMM_STYLE[/** @type {keyof typeof COMM_STYLE} */ (meta.kind)] ?? COMM_STYLE.mesh);
    const fade = 1 - (t - meta.at) / COMM_TTL_MS;
    ctx.save();
    ctx.globalAlpha = 0.35 + 0.65 * fade;
    drawFlashLine(nodeX[a], nodeY[a], nodeX[b], nodeY[b], style, now, false);
    ctx.restore();
  }

  const liveMfa = [...state.comm.mfaLinks.entries()].filter(([, m]) => t - m.at <= COMM_TTL_MS);
  if (liveMfa.length > 0) {
    ctx.save();
    ctx.globalAlpha = 0.85;
    ctx.fillStyle = "#ffb347";
    ctx.shadowColor = "#ffb347";
    ctx.shadowBlur = 12;
    ctx.beginPath();
    ctx.arc(MFA_HUB.x, MFA_HUB.y, 6, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#fff";
    ctx.font = "9px ui-monospace, monospace";
    ctx.shadowBlur = 0;
    ctx.fillText("MFA", MFA_HUB.x + 9, MFA_HUB.y + 3);
    ctx.restore();

    for (const [nodeId] of liveMfa.slice(0, 48)) {
      if (state.dead.has(nodeId)) continue;
      const fade = 1 - (t - (state.comm.mfaLinks.get(nodeId)?.at ?? t)) / COMM_TTL_MS;
      ctx.save();
      ctx.globalAlpha = 0.25 + 0.55 * fade;
      drawFlashLine(nodeX[nodeId], nodeY[nodeId], MFA_HUB.x, MFA_HUB.y, COMM_STYLE.mfa, now, false);
      ctx.restore();
    }
  }
}

/** @param {number} x @param {number} y @param {number} radius @param {string} color @param {number} alpha @param {number} [glow] */
function drawGlowDot(x, y, radius, color, alpha, glow = 8) {
  ctx.globalAlpha = alpha * 0.3;
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, radius + glow, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalAlpha = alpha;
  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fill();
}

/** @param {number} now */
export function drawActiveRoute(now) {
  const pt = state.paymentTransfer;
  const path = pt?.path ?? state.activeRoute;
  if (path.length < 2) return;

  const progress = pt ? pt.progress : 1;
  const settled = pt?.phase === "settled";
  const failed = pt?.phase === "failed";
  const traveling = pt?.phase === "traveling";

  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  ctx.strokeStyle = failed ? "rgba(231, 76, 60, 0.5)" : "rgba(0, 212, 255, 0.25)";
  ctx.lineWidth = 2;
  ctx.globalAlpha = 0.6;
  ctx.beginPath();
  ctx.moveTo(nodeX[path[0]], nodeY[path[0]]);
  for (let i = 1; i < path.length; i++) {
    ctx.lineTo(nodeX[path[i]], nodeY[path[i]]);
  }
  ctx.stroke();

  if (traveling || settled) {
    const dot = pathPointAtProgress(path, progress);
    ctx.strokeStyle = settled ? "#69f0ae" : "#00ffff";
    ctx.lineWidth = settled ? 4 : 3.5;
    ctx.globalAlpha = settled ? 0.95 : 0.85 + 0.15 * Math.sin(now * 0.012);
    ctx.shadowColor = settled ? "#69f0ae" : "#00d4ff";
    ctx.shadowBlur = settled ? 20 : 16;
    ctx.beginPath();
    ctx.moveTo(nodeX[path[0]], nodeY[path[0]]);
    if (dot) {
      ctx.lineTo(dot.x, dot.y);
    }
    ctx.stroke();

    if (dot) {
      const r = settled ? 7 + 2 * Math.sin(now * 0.008) : 5;
      ctx.globalAlpha = 1;
      ctx.fillStyle = settled ? "#b9ffcc" : "#e8ffff";
      ctx.shadowBlur = settled ? 28 : 22;
      ctx.beginPath();
      ctx.arc(dot.x, dot.y, r, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  if (settled && pt) {
    const dest = pt.destination;
    const burst = 0.5 + 0.5 * Math.sin(now * 0.006);
    ctx.globalAlpha = 0.35 + 0.35 * burst;
    ctx.fillStyle = "#69f0ae";
    ctx.shadowColor = "#69f0ae";
    ctx.shadowBlur = 24;
    ctx.beginPath();
    ctx.arc(nodeX[dest], nodeY[dest], 10 + 4 * burst, 0, Math.PI * 2);
    ctx.fill();
  }

  ctx.restore();
}

/** @param {number} now */
export function drawConstellation(now) {
  ctx.fillStyle = "#060a12";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  if (state.playing) {
    for (let i = 0; i < 60; i++) {
      const sx = (Math.sin(i * 127.1) * 0.5 + 0.5) * canvas.width;
      const sy = (Math.cos(i * 311.7) * 0.5 + 0.5) * canvas.height;
      ctx.globalAlpha = (0.3 + 0.7 * Math.abs(Math.sin(now * 0.001 + i))) * 0.35;
      ctx.fillStyle = "#8ba3cc";
      ctx.fillRect(sx, sy, 1.2, 1.2);
    }
  }

  const hoverMesh = new Set();
  if (state.hoveredNode && state.hoveredNode <= state.networkSize) {
    hoverMesh.add(state.hoveredNode);
    for (const p of meshPeerLinks(state.hoveredNode, state.networkSize).map((l) => l.peer)) {
      if (p <= state.networkSize) hoverMesh.add(p);
    }
  }

  drawMeshLayer(meshEdges.chord, {
    color: "rgba(155, 89, 182, 0.22)",
    hover: "rgba(192, 120, 255, 0.75)",
    width: 0.8,
    alpha: 0.22,
  }, now, hoverMesh);
  drawMeshLayer(meshEdges.skip, {
    color: "rgba(61, 139, 253, 0.28)",
    hover: "rgba(100, 180, 255, 0.85)",
    width: 0.9,
    alpha: 0.28,
  }, now, hoverMesh);
  drawMeshLayer(meshEdges.ring, {
    color: "rgba(46, 204, 113, 0.35)",
    hover: "rgba(80, 255, 160, 0.9)",
    width: 1,
    alpha: 0.35,
  }, now, hoverMesh);

  drawCommLinks(now);

  if (state.hoveredNode && state.hoveredNode <= state.networkSize) {
    drawHoveredPaths(now);
  }

  drawActiveRoute(now);

  for (const link of state.healLinks) {
    if (link.from > state.networkSize || link.to > state.networkSize) continue;
    ctx.strokeStyle = "rgba(0, 212, 255, 0.85)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(nodeX[link.from], nodeY[link.from]);
    ctx.lineTo(nodeX[link.to], nodeY[link.to]);
    ctx.stroke();
  }

  const pulse = state.playing ? 0.55 + 0.45 * Math.sin(now * 0.004 * state.speed) : 0.75;
  const nowMs = Date.now();
  pruneNodeVisualStates(nowMs);
  for (let id = 1; id <= state.networkSize; id++) {
    const isDead = state.dead.has(id);
    const isHovered = state.hoveredNode === id;
    const isLive = isCommLive(id, nowMs);
    const recv = state.comm.received.get(id);
    const recentlyReceived = recv && nowMs - recv.at < PAYMENT_SETTLE_DISPLAY_MS;
    const isPaySource = state.paymentTransfer?.source === id;
    const isPayDest = state.paymentTransfer?.destination === id;
    const paySettled = state.paymentTransfer?.phase === "settled";

    let color = isLive ? "#00e5ff" : "#2a3a4a";
    let radius = isLive ? 2.4 : 1.2;
    let alpha = isLive ? (pulse * 0.85 + 0.15) : 0.45;

    if (recentlyReceived || (isPayDest && paySettled)) {
      color = "#69f0ae";
      radius = 3.2;
      alpha = 0.85 + 0.15 * Math.sin(now * 0.01);
    } else if (isPaySource && state.paymentTransfer?.phase === "traveling") {
      color = "#ffb347";
      radius = 2.8;
    } else if (isPayDest && state.paymentTransfer?.phase === "traveling") {
      color = "#7ec8ff";
      radius = 2.6;
      alpha = 0.7 + 0.3 * Math.sin(now * 0.014);
    }
    const liquidity = state.liquidity.byNode.get(id);
    if (liquidity && nowMs - liquidity.at < 120_000) {
      if (liquidity.status === "funded") {
        color = "#f1c40f";
        radius = 2.6;
      } else if (liquidity.status === "faucet") {
        color = "#e67e22";
        radius = 2.4;
      } else if (liquidity.status === "failed") {
        color = "#c0392b";
        radius = 2.4;
      } else if (liquidity.status === "engaged" || liquidity.status === "started") {
        color = "#f39c12";
        radius = 2.3;
        alpha = state.playing ? 0.7 + 0.3 * Math.sin(now * 0.012) : 0.9;
      }
    }
    const visual = state.nodeVisual.get(id);
    if (visual && nowMs - visual.at < 120_000) {
      if (visual.status === "WARN_DRAIN") {
        color = "#ff7043";
        radius = 2.9;
        alpha = state.playing ? 0.75 + 0.25 * Math.sin(now * 0.018) : 0.95;
      } else if (visual.status === "INJECTING") {
        color = "#ffd54f";
        radius = 3;
        alpha = 0.9;
      }
    }
    if (isDead) {
      color = "#e74c3c";
      radius = 2.4;
      alpha = state.playing ? 0.85 + 0.15 * Math.sin(now * 0.006) : 0.95;
    } else if (state.healed.has(id)) {
      color = "#00d4ff";
      radius = 2.2;
    }
    if (isHovered) {
      radius += 1.2;
      alpha = 1;
    }
    drawGlowDot(nodeX[id], nodeY[id], radius, color, alpha, isHovered ? 12 : 6);
  }

  if (metricTick) metricTick.textContent = String(state.tick);
  if (metricLive) metricLive.textContent = String(state.comm.nodes.size);
  if (metricDead) metricDead.textContent = String(state.dead.size);
  if (metricHeals) metricHeals.textContent = String(state.healCount);
  state.dirty = false;
}

export { canvas, ctx };
