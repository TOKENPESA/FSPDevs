import { EDGE_NODES_STORAGE_KEY } from "./config.js";
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
import { connectMonitor } from "./events/monitor.js";
import { tickPaymentTransfer } from "./events/payment.js";
import { updateHubPanel } from "./events/liquidity.js";
import { logEvent, markDirty, state } from "./state.js";
import { buildMeshEdges } from "./topology.js";

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
 */
export function initFiberDashboard({ autoConnect = true } = {}) {
  const metricHover = document.getElementById("metric-hover");
  const speedInput = document.getElementById("speed");
  const speedLabel = document.getElementById("speed-label");
  const btnPlay = document.getElementById("btn-play");
  const btnPause = document.getElementById("btn-pause");
  const sizeInput = document.getElementById("network-size");
  const edgeCountInput = document.getElementById("edge-node-count");
  const edgePresets = document.getElementById("edge-presets");

  if (!canvas || !metricHover || !speedInput || !btnPlay || !btnPause) {
    throw new Error(
      "initFiberDashboard: required dashboard DOM nodes are missing (expected #grid, controls, metrics)",
    );
  }

  function syncPlaybackControls() {
    btnPlay.classList.toggle("playback-active", state.playing);
    btnPause.classList.toggle("playback-active", !state.playing);
    btnPlay.setAttribute("aria-pressed", String(state.playing));
    btnPause.setAttribute("aria-pressed", String(!state.playing));
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
  btnPlay.addEventListener("click", () => {
    state.playing = true;
    syncPlaybackControls();
    markDirty();
    logEvent("Animation playing");
  });
  btnPause.addEventListener("click", () => {
    state.playing = false;
    syncPlaybackControls();
    markDirty();
    logEvent("Animation paused");
  });
  speedInput.addEventListener("input", () => {
    state.speed = Number(speedInput.value);
    speedLabel.textContent = `${state.speed}×`;
    markDirty();
  });

  sizeInput?.addEventListener("input", () => {
    applyEdgeNodeCount(sizeInput.value);
  });

  edgeCountInput?.addEventListener("change", () => {
    applyEdgeNodeCount(edgeCountInput.value);
  });

  edgePresets?.addEventListener("click", (ev) => {
    const btn = ev.target.closest("button[data-n]");
    if (!btn) return;
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

if (typeof document !== "undefined" && document.getElementById("grid")) {
  initFiberDashboard();
}
