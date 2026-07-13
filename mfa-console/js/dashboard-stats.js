import { createLogger } from "../../dashboard/logger.js";
import { loadMfaRuntime } from "./mfa-runtime.js";
import { icon } from "./icons.js";
import { formatCount, formatShannons, metricCell } from "./stats-ui.js";
import { connectMonitor } from "../../dashboard/events/monitor.js";

/** @typedef {import('./types.js').MfaRuntimeDetail} MfaRuntimeDetail */

const log = createLogger("mfa-dashboard");

export async function loadDashboardSnapshot() {
  try {
    return await loadMfaRuntime({ force: true });
  } catch (error) {
    log.warn("stats unavailable", error);
    return null;
  }
}

/**
 * @param {MfaRuntimeDetail} runtime
 */
function connectedFaHint(runtime) {
  const cap = runtime.simulationEdgeNodes ?? "—";
  const ids = runtime.connectedAgentIds ?? [];
  if (ids.length === 0) {
    return `0 / ${cap} on mesh · spawn sidecars to connect`;
  }
  if (ids.length <= 6) {
    return `FA-${ids.join(", FA-")} · ${cap} configured`;
  }
  const sample = ids.slice(0, 4).join(", ");
  return `FA-${sample}, … +${ids.length - 4} · ${cap} configured`;
}

/**
 * @param {MfaRuntimeDetail} runtime
 */
function registryHint(runtime) {
  const corridors = runtime.assetCorridors ?? [];
  if (corridors.length === 0) return "AssetRegistryHub empty — bootstrap on MFA start";
  return corridors.join(" · ");
}

/**
 * @param {MfaRuntimeDetail} runtime
 */
function runningPluginsHint(runtime) {
  const plugins = runtime.runningPlugins ?? [];
  if (plugins.length === 0) {
    return "No policy plugins mounted — install from App Store";
  }
  return plugins.join(" · ");
}

/**
 * @param {MfaRuntimeDetail | null} runtime
 */
function renderMetricGrid(runtime) {
  if (!runtime) {
    return metricCell("MFA Status", "Offline", "Start MFA on 127.0.0.1:1025");
  }

  return [
    metricCell("Supervisor", runtime.service ?? "MFA", "127.0.0.1:1025", {
      trend: !runtime.error,
    }),
    metricCell(
      "Connected FAs",
      String(runtime.connectedAgents ?? 0),
      connectedFaHint(runtime),
      { trend: (runtime.connectedAgents ?? 0) > 0 },
    ),
    metricCell(
      "Monitor Stream",
      runtime.monitorConnected ? "Connected" : "Disconnected",
      runtime.monitorConnected ? "WebSocket /ws/monitor" : "Click Connect on dashboard",
      { trend: runtime.monitorConnected },
    ),
    metricCell(
      "Regional Clearing",
      runtime.clearingRegionalReady ? "Ready" : "Blocked",
      runtime.clearingHint ?? "",
      { trend: runtime.clearingRegionalReady },
    ),
    metricCell(
      "Enterprise Refuel",
      runtime.clearingCorporateVault ?? "—",
      runtime.clearingEnterprisePath ?? "TelemetryPacket pipeline",
      { trend: Boolean(runtime.clearingCorporateVault) },
    ),
    metricCell(
      "Running plugins",
      formatCount(runtime.runningPlugins?.length ?? 0, { label: "active" }),
      runningPluginsHint(runtime),
      { trend: (runtime.runningPlugins?.length ?? 0) > 0 },
    ),
    metricCell(
      "Asset Registry",
      formatCount(runtime.assetCorridors?.length ?? 0, { label: "corridors" }),
      registryHint(runtime),
      { trend: (runtime.assetCorridors?.length ?? 0) > 0 },
    ),
    metricCell(
      "Topology Journal",
      "WAL active",
      runtime.clearingTopologyJournal ?? "mesh_topology_journal.wal",
    ),
    metricCell(
      "Simulation Cap",
      String(runtime.simulationEdgeNodes ?? "—"),
      "Max FA IDs in routing view",
    ),
    metricCell(
      "Monitor Telemetry",
      String(runtime.monitorLiveNodes ?? 0),
      `${runtime.offlineNodes ?? 0} marked offline on canvas`,
      { trend: (runtime.monitorLiveNodes ?? 0) > 0 },
    ),
    metricCell("Mesh Heals", String(runtime.healCount ?? 0), "Hot-swap recoveries"),
    metricCell(
      "Hub RPC",
      runtime.hubRpcUrl ?? "—",
      runtime.hubFunding != null
        ? `${formatShannons(runtime.hubFunding)} shannons / channel`
        : "FNN hub endpoint",
    ),
    metricCell(
      "Liquidity OK",
      String(runtime.liquidityInjections ?? 0),
      "Successful hub injections",
      { trend: (runtime.liquidityInjections ?? 0) > 0 },
    ),
    metricCell(
      "Compliance Tickets",
      `Ephemeral · ${runtime.complianceTicketTtl ?? 30}s`,
      "Single-use SSE burn on connect",
    ),
  ].join("");
}

/**
 * @param {MfaRuntimeDetail | null} runtime
 */
export function renderDashboardStats(runtime) {
  const connected = runtime?.connectedAgents ?? 0;
  const cap = runtime?.simulationEdgeNodes ?? 1024;
  const plugins = runtime?.runningPlugins ?? [];
  const pluginBanner =
    runtime && plugins.length === 0
      ? `<div class="dashboard-banner dashboard-banner-warn" role="status">
          No policy plugins are <strong>running</strong> on this supervisor.
          Install or resume plugins from <strong>App Store</strong>.
        </div>`
      : "";

  return `
    <section class="dashboard-page" data-dashboard-stats>
      <header class="dashboard-hero">
        <div class="dashboard-hero-copy">
          <h1>Master Fiber Agent Operations</h1>
          <p>
            Supervise the ${cap}-node mesh control plane with ${connected} connected sidecar${connected === 1 ? "" : "s"}.
            Enterprise clearinghouse refuel, regional float-crisis intake, AssetRegistryHub corridors,
            and EphemeralTicket compliance streams run on this supervisor.
          </p>
          <div class="dashboard-hero-actions">
            <button type="button" class="hero-btn hero-btn-primary" data-action="refresh-dashboard-stats">
              Refresh stats
            </button>
            <button type="button" class="hero-btn" data-navigate="mfa-app-store">
              ${icon("appStore", 16)}
              App Store
            </button>
            <button type="button" class="hero-btn" data-action="connect-monitor">
              Connect monitor
            </button>
          </div>
        </div>
        <div class="dashboard-hero-visual" aria-hidden="true">
          <div class="hero-node-stack">
            <div class="hero-node hero-node-main">${icon("mesh", 28)}</div>
            <div class="hero-node hero-node-orbit">${icon("routing", 18)}</div>
            <div class="hero-node hero-node-orbit delay">${icon("liquidity", 18)}</div>
          </div>
        </div>
      </header>
      <div class="dashboard-alerts">
        ${pluginBanner}
      </div>
      <div class="dashboard-body">
        <div class="dashboard-metrics">
          <div class="metric-grid">${renderMetricGrid(runtime)}</div>
        </div>
      </div>
    </section>`;
}

/**
 * @param {HTMLElement} root
 * @param {{ onRefresh?: () => void, onConnect?: () => void, onNavigate?: (routeId: string) => void }} actions
 */
export function bindDashboardActions(root, { onRefresh, onConnect, onNavigate }) {
  root.querySelector('[data-action="refresh-dashboard-stats"]')?.addEventListener("click", () => {
    onRefresh?.();
  });
  root.querySelector('[data-action="connect-monitor"]')?.addEventListener("click", () => {
    if (onConnect) onConnect();
    else void connectMonitor();
  });
  root.querySelectorAll("[data-navigate]").forEach((button) => {
    button.addEventListener("click", () => {
      const routeId = button instanceof HTMLButtonElement ? button.dataset.navigate : "";
      if (routeId) onNavigate?.(routeId);
    });
  });
}
