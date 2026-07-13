import { createLogger } from "../dashboard/logger.js";
import { formatShannonsCompact } from "../dashboard/money.js";
import { formatAgentLabel, loadSidecarRuntime } from "./sidecar-runtime.js";
import { icon } from "./icons.js";
import { renderOobFallbackCard } from "./oob-fallback.js";
import {
  dashboardMetricSection,
  formatCount,
  formatFiatFromShannons,
  formatShannons,
  kpiCard,
  metricCell,
  shortPubkey,
  shortMemberId,
} from "./stats-ui.js";

/** @typedef {import("../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const log = createLogger("dashboard");

export async function loadDashboardSnapshot() {
  try {
    return await loadSidecarRuntime({ force: true });
  } catch (error) {
    log.warn("sidecar stats unavailable", error);
    return null;
  }
}

function mfaHint(/** @type {SidecarRuntimeStats} */ runtime) {
  if (runtime.mfaControlConnected) {
    return `Registered · ${runtime.mfaWsUrl ?? runtime.mfaHost}`;
  }
  if (runtime.mfaReachable || runtime.mfaConnectionStatus === "reachable") {
    return `Reachable only · WS not registered`;
  }
  return `Unreachable · ${runtime.mfaHost}`;
}

function fnnHint(/** @type {SidecarRuntimeStats} */ runtime) {
  if (runtime.fnnConnectionStatus === "simulated") {
    return `Mock engine · ${runtime.fnnP2pEndpoint ?? "simulate mode"}`;
  }
  if (runtime.fnnConnectionStatus === "online") {
    return `${(runtime.fnnRpcUrl ?? "").replace("http://", "")} · P2P ${runtime.fnnP2pEndpoint ?? "—"}`;
  }
  return `${(runtime.fnnRpcUrl ?? "").replace("http://", "")} · P2P ${runtime.fnnP2pEndpoint ?? "—"}`;
}

/**
 * @param {SidecarRuntimeStats | null | undefined} runtime
 * @param {"mfa" | "fnn"} key
 * @returns {"online" | "warn" | "offline" | "neutral"}
 */
function connectionStatus(runtime, key) {
  if (!runtime) return "offline";
  if (key === "mfa") {
    if (runtime.mfaControlConnected) return "online";
    if (runtime.mfaReachable || runtime.mfaConnectionStatus === "reachable") return "warn";
    return "offline";
  }
  if (key === "fnn") {
    if (runtime.fnnConnectionStatus === "online") return "online";
    if (runtime.fnnConnectionStatus === "simulated") return "warn";
    return "offline";
  }
  return "neutral";
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
function renderStatusPills(runtime) {
  if (!runtime) return "";
  const pills = [
    { label: formatAgentLabel(runtime), status: "online" },
    { label: runtime.mfaControlConnected ? "MFA linked" : "MFA offline", status: connectionStatus(runtime, "mfa") },
    { label: runtime.fnnConnectionStatus ?? "FNN unknown", status: connectionStatus(runtime, "fnn") },
    {
      label: `${runtime.meshChannelsActive ?? 0} channels`,
      status: (runtime.meshChannelsActive ?? 0) > 0 ? "online" : "neutral",
    },
  ];
  return `
    <div class="dashboard-status-pills" role="list">
      ${pills
        .map(
          (pill) =>
            `<span class="status-pill status-pill--${pill.status}" role="listitem">${pill.label}</span>`,
        )
        .join("")}
    </div>
  `;
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
function renderKpiStrip(runtime) {
  if (!runtime) {
    return `
      <div class="dashboard-kpi-strip">
        ${kpiCard("Runtime", "Offline", "Start the Tauri shell", { status: "offline" })}
      </div>
    `;
  }

  return `
    <div class="dashboard-kpi-strip">
      ${kpiCard("Total liquidity", formatShannonsCompact(runtime.fnnTotalLiquidityShannons), formatFiatFromShannons(runtime.fnnTotalLiquidityShannons, runtime.fiatConversionRate), { status: "online" })}
      ${kpiCard("Open channels", `${runtime.meshChannelsActive ?? 0} / ${runtime.meshChannelsTotal ?? 0}`, "Active payment channels", { status: (runtime.meshChannelsActive ?? 0) > 0 ? "online" : "neutral" })}
      ${kpiCard("Edge settlements", formatCount(runtime.edgeSettled ?? 0, { label: "confirmed" }), formatCount(runtime.edgePending ?? 0, { label: "pending" }), { status: (runtime.edgeSettled ?? 0) > 0 ? "online" : "neutral" })}
      ${kpiCard("Telemetry backlog", formatCount(runtime.queuedTelemetry ?? 0, { label: "pulses" }), runtime.mfaControlConnected ? "MFA control online" : "WAL on supervisor", { status: (runtime.queuedTelemetry ?? 0) > 0 ? "warn" : "online" })}
    </div>
  `;
}

/** @param {SidecarRuntimeStats} runtime @returns {string} */
function renderLiquidityAside(runtime) {
  const local = runtime.totalLocalBalanceShannons ?? 0;
  const remote = runtime.totalRemoteBalanceShannons ?? 0;
  const total = local + remote || 1;
  const localPct = (local / total) * 100;
  const remotePct = (remote / total) * 100;
  const cached = runtime.cachedChannels ?? 0;

  return `
    <aside class="dashboard-aside">
      <div class="aside-card">
        <div class="aside-head">
          <h3>Mesh Liquidity</h3>
          <span class="aside-hint">FNN channel capacity split</span>
        </div>

        <div
          class="liquidity-donut"
          style="--seg-out: ${localPct.toFixed(2)}%; --seg-in: ${(localPct + remotePct).toFixed(2)}%;"
        >
          <div class="liquidity-donut-hole">
            <strong title="${formatShannons(runtime.fnnTotalLiquidityShannons)}">${formatShannonsCompact(runtime.fnnTotalLiquidityShannons)}</strong>
            <span>total shannons</span>
          </div>
        </div>

        <ul class="liquidity-legend">
          <li>
            <span class="liquidity-dot outbound"></span>
            <span class="liquidity-legend-copy">
              <strong>Outbound</strong>
              <span>${localPct.toFixed(1)}% · ${formatShannons(local)}</span>
            </span>
          </li>
          <li>
            <span class="liquidity-dot inbound"></span>
            <span class="liquidity-legend-copy">
              <strong>Inbound</strong>
              <span>${remotePct.toFixed(1)}% · ${formatShannons(remote)}</span>
            </span>
          </li>
          <li>
            <span class="liquidity-dot cached"></span>
            <span class="liquidity-legend-copy">
              <strong>Cached snapshots</strong>
              <span>${formatCount(cached, { label: "channel rows" })}</span>
            </span>
          </li>
        </ul>
      </div>
    </aside>
  `;
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string[]} */
function runningModules(runtime) {
  return Array.isArray(runtime?.mountedModules) ? runtime.mountedModules : [];
}

/** @param {string} moduleId @returns {string} */
function heroIconForModule(moduleId) {
  if (moduleId === "dicoba") return "dicoba";
  if (moduleId === "fiat_bridge") return "float";
  return "modules";
}

/** @param {string[]} mounted @returns {string} */
function renderHeroVisual(mounted) {
  const orbitIcons = mounted.slice(0, 2).map((id) => heroIconForModule(id));
  return `
    <div class="hero-node-stack">
      <div class="hero-node hero-node-main">${icon("modules", 28)}</div>
      ${
        orbitIcons[0]
          ? `<div class="hero-node hero-node-orbit">${icon(orbitIcons[0], 18)}</div>`
          : ""
      }
      ${
        orbitIcons[1]
          ? `<div class="hero-node hero-node-orbit delay">${icon(orbitIcons[1], 18)}</div>`
          : ""
      }
    </div>
  `;
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
function renderMetricSections(runtime) {
  if (!runtime) {
    return dashboardMetricSection(
      "Runtime",
      "Sidecar stats unavailable",
      metricCell("Status", "Offline", "Start the Tauri desktop shell to load live stats"),
    );
  }

  const mounted = runningModules(runtime);
  const runningHint =
    mounted.length === 0
      ? "No FSP modules are actively running on this sidecar"
      : "Hot-mounted modules currently executing RPC and peer routes";

  return [
    dashboardMetricSection(
      "Connectivity",
      "MFA control plane, FNN pairing, and mesh peers",
      [
        metricCell("Connected MFA", runtime.mfaName ?? "—", mfaHint(runtime), {
          trend: runtime.mfaControlConnected === true,
        }),
        metricCell("FNN status", runtime.fnnConnectionStatus ?? "—", fnnHint(runtime), {
          trend: runtime.fnnConnectionStatus !== "offline",
        }),
        metricCell("FNN backend", runtime.fnnBackend ?? "—", runtime.fnnMode ?? "—"),
        metricCell("Mesh peer", runtime.meshPeerAgentId ? `FA-${runtime.meshPeerAgentId}` : "—", shortPubkey(runtime.meshPeerPubkey)),
        metricCell("FNN pubkey", shortPubkey(runtime.nodePubkey), "Paired node identity"),
      ].join(""),
    ),
    dashboardMetricSection(
      "Liquidity & channels",
      "Outbound/inbound capacity and open mesh channels",
      [
        metricCell(
          "Outbound liquidity",
          formatShannons(runtime.totalLocalBalanceShannons ?? 0),
          formatFiatFromShannons(runtime.totalLocalBalanceShannons ?? 0, runtime.fiatConversionRate),
          { trend: (runtime.totalLocalBalanceShannons ?? 0) > 0 },
        ),
        metricCell(
          "Inbound liquidity",
          formatShannons(runtime.totalRemoteBalanceShannons ?? 0),
          formatFiatFromShannons(runtime.totalRemoteBalanceShannons ?? 0, runtime.fiatConversionRate),
          { trend: (runtime.totalRemoteBalanceShannons ?? 0) > 0 },
        ),
        metricCell(
          "Open channels",
          `${runtime.meshChannelsActive ?? 0} / ${runtime.meshChannelsTotal ?? 0}`,
          "Active payment channels",
          { trend: (runtime.meshChannelsActive ?? 0) > 0 },
        ),
      ].join(""),
    ),
    dashboardMetricSection(
      "Edge ledger",
      "Settlements flowing through the sidecar edge store",
      [
        metricCell(
          "Settled edge tx",
          formatCount(runtime.edgeSettled ?? 0, { label: "settlements" }),
          "Confirmed ledger settlements",
          { trend: (runtime.edgeSettled ?? 0) > 0 },
        ),
        metricCell(
          "Pending edge tx",
          formatCount(runtime.edgePending, { label: "pending" }),
          "Awaiting settlement",
        ),
        metricCell(
          "Failed edge tx",
          formatCount(runtime.edgeFailed ?? 0, { label: "failed" }),
          "Rejected or rolled-back edge records",
        ),
        metricCell(
          "Queued telemetry",
          formatCount(runtime.queuedTelemetry, { label: "pulses" }),
          "Offline MFA pulse backlog",
        ),
      ].join(""),
    ),
    dashboardMetricSection(
      "Agent & modules",
      "Hardware profile, running FSP modules, and runtime identity",
      [
        metricCell("Active agent", formatAgentLabel(runtime), (runtime.hardwareProfile ?? "").replaceAll("_", " "), {
          trend: true,
        }),
        metricCell(
          "Power profile",
          runtime.powerProfile ?? "—",
          "Adaptive edge power controller",
          { trend: runtime.powerProfile === "AggressiveRealTime" },
        ),
        metricCell("Sidecar profile", runtime.sidecarProfile ?? "unknown", runtime.profileSource ?? "profile"),
        metricCell("Running modules", mounted.join(", ") || "none", runningHint),
        ...(mounted.includes("dicoba") && runtime.dicobaMemberId
          ? [
              metricCell(
                "DiCoBa member ID",
                shortMemberId(runtime.dicobaMemberId),
                runtime.dicobaMemberId,
                { trend: true },
              ),
            ]
          : []),
      ].join(""),
    ),
  ].join("");
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
export function renderDashboardStats(runtime) {
  const agentLabel = formatAgentLabel(runtime);
  const mounted = runningModules(runtime);
  const profileBanner =
    runtime && mounted.length === 0
      ? `<div class="dashboard-banner dashboard-banner-warn" role="status">
          No FSP modules are <strong>running</strong> on this sidecar.
          Install or resume modules from <strong>App Store</strong>, or switch to a profile that mounts kiosk/coop/full modules.
        </div>`
      : "";

  const mfaOobBanner =
    runtime && !runtime.mfaControlConnected
      ? `<div class="dashboard-banner dashboard-banner-warn" role="status">
          MFA control plane is not registered — use <strong>FSP Out-of-Band</strong> below to relay peer module packets offline.
        </div>`
      : "";

  return `
    <section class="dashboard-page" data-dashboard-stats>
      <header class="dashboard-hero">
        <div class="dashboard-hero-copy">
          <p class="dashboard-eyebrow">Operations console</p>
          <h1>Fiber Sidecar</h1>
          <p>
            Live health for ${agentLabel}: FNN pairing, edge ledger, and regional clearing telemetry.
            Float-crisis posts route to MFA; balance depletion triggers EnterpriseClearinghouse refuel.
          </p>
          ${renderStatusPills(runtime)}
          <div class="dashboard-hero-actions">
            <button type="button" class="hero-btn hero-btn-primary" data-action="refresh-dashboard-stats">
              ${icon("dashboard", 16)}
              Refresh stats
            </button>
            <button type="button" class="hero-btn" data-navigate="fa-app-store">
              ${icon("appStore", 16)}
              App Store
            </button>
          </div>
        </div>
        <div class="dashboard-hero-visual" aria-hidden="true">
          ${renderHeroVisual(mounted)}
        </div>
      </header>

      ${renderKpiStrip(runtime)}

      <div class="dashboard-alerts">
        ${profileBanner}
        ${mfaOobBanner}
      </div>

      <div class="dashboard-main">
        <div class="dashboard-metrics">
          ${renderMetricSections(runtime)}
        </div>
        ${runtime ? renderLiquidityAside(runtime) : ""}
      </div>

      <div class="dashboard-bottom">
        ${renderOobFallbackCard()}
      </div>
    </section>
  `;
}

/**
 * @param {HTMLElement} root
 * @param {{ onRefresh?: () => void, onNavigate?: (routeId: string) => void }} [actions]
 */
export function bindDashboardActions(root, { onRefresh, onNavigate } = {}) {
  root
    .querySelector('[data-action="refresh-dashboard-stats"]')
    ?.addEventListener("click", () => onRefresh?.());

  root.querySelectorAll("[data-navigate]").forEach((button) => {
    button.addEventListener("click", () => {
      const routeId = /** @type {HTMLButtonElement} */ (button).dataset.navigate;
      if (routeId) onNavigate?.(routeId);
    });
  });
}
