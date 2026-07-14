import { applyEdgeNodeCount, fetchHubHealth } from "../../../../dashboard/api/mfa.js";
import { connectMonitor } from "../../../../dashboard/events/monitor.js";
import { state } from "../../../../dashboard/state.js";
import { paintMeshCanvasHints, tryAutoConnectMonitor, monitorStatusLabel } from "../../monitor-bridge.js";
import { attachMeshCanvas } from "../../mesh-canvas.js";
import { metricSection, metricCell } from "../../stats-ui.js";

/** @typedef {import('../../types.js').MfaUiHostContext} MfaUiHostContext */

export const meshPanel = {
  id: "mfa-mesh",
  title: "Mesh Network",
  navLabel: "Visualizer",
  navIcon: "mesh",
  badge: "topology",
  navDescription: "1024-node lattice with live MFA telemetry and simulation sizing.",
  render() {
    return `
      <div class="workspace-card">
        <div class="workspace-card-head">
          <h2>Lattice visualizer</h2>
          <p class="panel-hint">Bright nodes = live telemetry · click to toggle offline</p>
        </div>
        <div class="mesh-controls" style="margin-bottom:0.85rem">
          <div class="mesh-control-row">
            <button type="button" class="panel-btn panel-btn-primary" data-action="mesh-connect">Connect monitor</button>
            <button type="button" class="panel-btn" data-action="mesh-play">Play</button>
            <button type="button" class="panel-btn" data-action="mesh-pause">Pause</button>
          </div>
          <div class="workspace-field">
            <label for="mesh-edge-count">Edge nodes (FA count)</label>
            <input id="mesh-edge-count" type="number" min="1" max="1024" value="${state.networkSize}">
          </div>
          <div class="mesh-preset-row" data-mesh-presets>
            ${[16, 32, 64, 128, 256, 512, 1024]
              .map((n) => `<button type="button" data-n="${n}">${n}</button>`)
              .join("")}
          </div>
        </div>
        <div class="mesh-canvas-wrap" data-mesh-canvas-host>
          <div class="mesh-canvas-hint" data-mesh-canvas-hint>Connect monitor for live heartbeats</div>
        </div>
      </div>`;
  },
  renderAside() {
    return metricSection(
      "Mesh telemetry",
      [
        metricCell("Tick", String(state.tick), "Monitor frames"),
        metricCell("Live", String(state.comm.nodes.size), "Nodes with heartbeat"),
        metricCell("Offline", String(state.dead.size), "Marked dead"),
        metricCell("Heals", String(state.healCount), "Recovery events"),
      ].join(""),
      { hint: "Updates while monitor is connected" },
    );
  },
  /**
   * @param {HTMLElement} root
   * @param {MfaUiHostContext} ctx
   */
  async mount(root, ctx) {
    const canvasHost = root.querySelector("[data-mesh-canvas-host]");
    await attachMeshCanvas(canvasHost);

    ctx.connectMonitor = connectMonitor;

    root.querySelector('[data-action="mesh-connect"]')?.addEventListener("click", () => {
      void connectMonitor().then(() => paintMeshCanvasHints());
    });
    root.querySelector('[data-action="mesh-play"]')?.addEventListener("click", () => {
      state.playing = true;
      document.getElementById("btn-play")?.click();
    });
    root.querySelector('[data-action="mesh-pause"]')?.addEventListener("click", () => {
      state.playing = false;
      document.getElementById("btn-pause")?.click();
    });

    const edgeInput = root.querySelector("#mesh-edge-count");
    edgeInput?.addEventListener("change", () => {
      if (edgeInput instanceof HTMLInputElement) {
        applyEdgeNodeCount(edgeInput.value);
      }
    });
    root.querySelector("[data-mesh-presets]")?.addEventListener("click", (ev) => {
      const target = ev.target;
      if (!(target instanceof Element)) return;
      const btn = target.closest("button[data-n]");
      if (!(btn instanceof HTMLButtonElement)) return;
      applyEdgeNodeCount(btn.dataset.n ?? "");
      if (edgeInput instanceof HTMLInputElement && btn.dataset.n) {
        edgeInput.value = btn.dataset.n;
      }
    });

    void fetchHubHealth().then(() => paintMeshCanvasHints());
    paintMeshCanvasHints();
    if (monitorStatusLabel() !== "connected") {
      void tryAutoConnectMonitor();
    }
  },
};
