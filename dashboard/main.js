import { EDGE_NODES_STORAGE_KEY } from "./config.js";
import { createLogger } from "./logger.js";
import { $, $button, $input, setText } from "./dom.js";
import {
  applyEdgeNodeCount,
  routeTransaction,
  updateEdgeNodeUi,
} from "./api/mfa.js";
import { canvas, layoutNodes, nodeAt } from "./canvas/layout.js";
import { drawConstellation } from "./canvas/draw.js";
import {
  hideTooltip,
  nodeStatus,
  updateTooltip,
} from "./canvas/tooltip.js";
import { connectMonitor, handleVersionedMonitorEnvelope, initializeMonitorSocket } from "./events/monitor.js";
import { tickPaymentTransfer } from "./events/payment.js";
import { updateHubPanel } from "./events/liquidity.js";
import { appendLogEvent, logEvent, markDirty, state, updateNodeVisualState } from "./state.js";
import { buildMeshEdges } from "./topology.js";
import { initSidecarConsole } from "./sidecar-console.js";

export const DASHBOARD_VERSION = "1.0.0";

export {
  EDGE_NODES_STORAGE_KEY,
  applyEdgeNodeCount,
  routeTransaction,
  updateEdgeNodeUi,
  canvas,
  layoutNodes,
  nodeAt,
  drawConstellation,
  hideTooltip,
  nodeStatus,
  updateTooltip,
  connectMonitor,
  tickPaymentTransfer,
  updateHubPanel,
  logEvent,
  appendLogEvent,
  updateNodeVisualState,
  markDirty,
  state,
  buildMeshEdges,
};

/**
 * Boots the full Fiber mesh dashboard when the standard demo shell is present.
 * Third-party embeds can import exports above and wire their own DOM instead.
 * @param {{ autoConnect?: boolean }} [opts]
 */
export function initFiberDashboard({ autoConnect = true } = {}) {
  const metricHover = $("metric-hover");
  const speedInput = $input("speed");
  const speedLabel = $("speed-label");
  const btnPlay = $button("btn-play");
  const btnPause = $button("btn-pause");
  const sizeInput = $input("network-size");
  const edgeCountInput = $input("edge-node-count");
  const edgePresets = $("edge-presets");

  if (!metricHover || !speedInput || !btnPlay || !btnPause) {
    throw new Error(
      "initFiberDashboard: required dashboard DOM nodes are missing (expected #grid, controls, metrics)",
    );
  }

  const playButton = btnPlay;
  const pauseButton = btnPause;

  function syncPlaybackControls() {
    playButton.classList.toggle("playback-active", state.playing);
    pauseButton.classList.toggle("playback-active", !state.playing);
    playButton.setAttribute("aria-pressed", String(state.playing));
    pauseButton.setAttribute("aria-pressed", String(!state.playing));
  }

  let mouseRaf = 0;
  canvas.addEventListener("mousemove", (ev) => {
    state.lastPointer = { clientX: ev.clientX, clientY: ev.clientY };
    if (mouseRaf) return;
    mouseRaf = requestAnimationFrame(() => {
      mouseRaf = 0;
      const rect = canvas.getBoundingClientRect();
      const x = (ev.clientX - rect.left) * (canvas.width / rect.width);
      const y = (ev.clientY - rect.top) * (canvas.height / rect.height);
      const id = nodeAt(x, y);
      if (id !== state.hoveredNode) {
        state.hoveredNode = id;
        if (id) {
          const st = nodeStatus(id);
          metricHover.textContent = `FA-${id} · ${st.label}`;
        } else {
          metricHover.textContent = "—";
        }
        markDirty();
      }
      updateTooltip(id, ev.clientX, ev.clientY);
    });
  });

  canvas.addEventListener("mouseleave", () => {
    state.hoveredNode = null;
    metricHover.textContent = "—";
    hideTooltip();
    markDirty();
  });

  canvas.addEventListener("click", (ev) => {
    const rect = canvas.getBoundingClientRect();
    const x = (ev.clientX - rect.left) * (canvas.width / rect.width);
    const y = (ev.clientY - rect.top) * (canvas.height / rect.height);
    const id = nodeAt(x, y);
    if (!id) return;
    if (state.dead.has(id)) {
      state.dead.delete(id);
      logEvent(`FA-${id} restored (local)`, "heal");
    } else {
      state.dead.add(id);
      logEvent(`FA-${id} marked offline (local)`, "dead");
    }
    markDirty();
    if (state.hoveredNode === id) {
      updateTooltip(id, ev.clientX, ev.clientY);
      const st = nodeStatus(id);
      metricHover.textContent = `FA-${id} · ${st.label}`;
    }
  });

  document.getElementById("btn-connect")?.addEventListener("click", connectMonitor);
  document.getElementById("btn-route")?.addEventListener("click", () => {
    routeTransaction();
  });
  playButton.addEventListener("click", () => {
    state.playing = true;
    syncPlaybackControls();
    markDirty();
    logEvent("Animation playing");
  });
  pauseButton.addEventListener("click", () => {
    state.playing = false;
    syncPlaybackControls();
    markDirty();
    logEvent("Animation paused");
  });
  speedInput.addEventListener("input", () => {
    state.speed = Number(speedInput.value);
    setText(speedLabel, `${state.speed}×`);
    markDirty();
  });

  sizeInput?.addEventListener("input", () => {
    applyEdgeNodeCount(sizeInput.value);
  });

  edgeCountInput?.addEventListener("change", () => {
    applyEdgeNodeCount(edgeCountInput.value);
  });

  edgePresets?.addEventListener("click", (ev) => {
    const target = ev.target;
    if (!(target instanceof Element)) return;
    const btn = target.closest("button[data-n]");
    if (!(btn instanceof HTMLButtonElement)) return;
    applyEdgeNodeCount(btn.dataset.n);
  });

  layoutNodes();
  buildMeshEdges();
  updateEdgeNodeUi(state.networkSize);
  try {
    const saved = localStorage.getItem(EDGE_NODES_STORAGE_KEY);
    if (saved) applyEdgeNodeCount(saved, { skipSync: true });
  } catch {
    /* ignore */
  }
  syncPlaybackControls();
  updateHubPanel();

  /** @param {number} now */
  function frame(now) {
    if (!state.animTime) state.animTime = now;
    if (state.playing) {
      state.animTime = now;
    }

    tickPaymentTransfer(now);

    const needsRedraw =
      state.dirty ||
      state.playing ||
      state.hoveredNode !== null ||
      state.activeRoute.length > 0 ||
      state.paymentTransfer != null ||
      state.comm.nodes.size > 0;

    if (needsRedraw) {
      drawConstellation(state.animTime);
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  if (autoConnect) {
    connectMonitor();
  }

  return {
    state,
    connectMonitor,
    handleVersionedMonitorEnvelope,
    initializeMonitorSocket,
    layoutNodes,
    drawConstellation,
    routeTransaction,
  };
}

const log = createLogger("fiber-dashboard");

if (typeof document !== "undefined" && document.getElementById("grid") && document.querySelector(".canvas-wrap")) {
  try {
    initFiberDashboard();
  } catch (err) {
    log.error("init failed", err);
  }
}

if (typeof document !== "undefined" && document.getElementById("console-stream")) {
  void initSidecarConsole().catch((err) => {
    log.error("sidecar console init failed", err);
  });
}
