import { createLogger } from "../../dashboard/logger.js";
import {
  mfaDisplayHost,
  mfaMonitorWsBaseUrl,
  seedMfaApiTokenFromQuery,
} from "../../dashboard/config.js";
import { parkMeshCanvas } from "./mesh-canvas.js";
import { escapeHtml, safeUserMessage } from "./dom-security.js";
import { MFA_MODULES } from "./module-registry.js";
import { navIcon } from "./icons.js";
import { MfaUiHost } from "./ui-host.js";
import { connectMonitor } from "../../dashboard/events/monitor.js";
import { startMeshHintWatcher, tryAutoConnectMonitor } from "./monitor-bridge.js";

const log = createLogger("mfa-ui");

seedMfaApiTokenFromQuery();
parkMeshCanvas();

const brandSubtitle = document.getElementById("brand-subtitle");
if (brandSubtitle) brandSubtitle.textContent = mfaDisplayHost();
const mfaWsInput = document.getElementById("mfa-ws");
if (mfaWsInput instanceof HTMLInputElement) {
  mfaWsInput.value = mfaMonitorWsBaseUrl();
}

const contentEl = document.getElementById("main-content");
const navEl = document.getElementById("sidebar-nav");
if (!(contentEl instanceof HTMLElement) || !(navEl instanceof HTMLElement)) {
  throw new Error("MFA shell markup missing #main-content or #sidebar-nav");
}

const host = new MfaUiHost(contentEl, navEl);

for (const mfaModule of MFA_MODULES) {
  host.register(mfaModule);
}

host.ctx.connectMonitor = connectMonitor;

async function bootApp() {
  try {
    if (document.getElementById("grid") && !window.__MFA_MESH_BOOTED__) {
      const { initFiberDashboard } = await import("../../dashboard/main.js");
      initFiberDashboard({ autoConnect: false });
      window.__MFA_MESH_BOOTED__ = true;
    }
    await host.boot();
    startMeshHintWatcher();
    void tryAutoConnectMonitor();
  } catch (error) {
    log.error("boot failed", error);
    const content = document.getElementById("main-content");
    const nav = document.getElementById("sidebar-nav");
    if (nav && !nav.innerHTML.trim()) {
      nav.innerHTML = `
        <button type="button" class="nav-item active" data-route-id="dashboard">
          <span class="nav-icon-box">${navIcon("dashboard", 16)}</span>
          <span class="nav-label">Dashboard</span>
        </button>`;
    }
    if (content) {
      content.innerHTML = `
        <section class="content-panel">
          <h1>MFA UI failed to start</h1>
          <p class="panel-lead">${escapeHtml(safeUserMessage(error))}</p>
          <p class="panel-hint">Serve via <code>npm run serve:mfa</code> and start MFA on :1025.</p>
        </section>`;
    }
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => void bootApp());
} else {
  void bootApp();
}
