import { createLogger } from "../dashboard/logger.js";
import { safeUserMessage, escapeHtml } from "./dom-security.js";
import { modulesForMounted } from "./module-registry.js";
import { navIcon } from "./icons.js";
import { getSidecarStats, hasTauri } from "./sidecar-api.js";
import { SidecarUiHost } from "./ui-host.js";
import fundingModule from "./modules/funding/index.js";
import appStoreModule from "./modules/app-store/index.js";

const log = createLogger("sidecar-ui");

const host = new SidecarUiHost(
  document.getElementById("main-content"),
  document.getElementById("sidebar-nav"),
);

async function mountUiModulesFromBackend() {
  // Always-on onboarding + store (not gated by backend profile)
  host.ensureRegistered(fundingModule);
  host.ensureRegistered(appStoreModule);

  if (!hasTauri()) {
    log.warn("Tauri runtime unavailable — module panels stay hidden");
    return;
  }

  let stats = null;
  try {
    stats = await getSidecarStats();
  } catch (error) {
    log.warn("could not load mounted modules", error);
    return;
  }

  const mounted = Array.isArray(stats?.mountedModules) ? stats.mountedModules : [];
  for (const sidecarModule of modulesForMounted(mounted)) {
    host.register(sidecarModule);
  }

  if (mounted.length === 0) {
    log.info(`profile '${stats?.sidecarProfile ?? "unknown"}' has no mounted modules`);
  }
}

async function bootApp() {
  try {
    await mountUiModulesFromBackend();
    await host.boot();
  } catch (error) {
    log.error("boot failed", error);
    const content = document.getElementById("main-content");
    const nav = document.getElementById("sidebar-nav");
    if (nav && !nav.innerHTML.trim()) {
      nav.innerHTML = `
        <button type="button" class="nav-item active" data-route-id="dashboard">
          <span class="nav-icon-box">${navIcon("dashboard", 16)}</span>
          <span class="nav-label">Dashboard</span>
        </button>
      `;
    }
    if (content) {
      content.innerHTML = `
        <section class="content-panel">
          <h1>Sidecar UI failed to start</h1>
          <p class="panel-lead">${escapeHtml(safeUserMessage(error))}</p>
          <p class="panel-hint">Check the devtools console, then restart <code>npm run tauri:dev</code>.</p>
        </section>
      `;
    }
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => {
    void bootApp();
  });
} else {
  void bootApp();
}
