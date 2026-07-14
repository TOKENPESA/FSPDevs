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

/** @param {string | undefined} status */
function friendlyFnnStatus(status) {
  switch ((status ?? "").toLowerCase()) {
    case "online":
      return "Online";
    case "simulated":
      return "Demo mode";
    case "offline":
      return "Offline";
    default:
      return "Unknown";
  }
}

/** @param {string | undefined} backend */
function friendlyFnnBackend(backend) {
  const value = (backend ?? "").toLowerCase();
  if (value.includes("sim")) return "Demo network";
  if (value.includes("live")) return "Live network";
  return backend || "—";
}

function mfaHint(/** @type {SidecarRuntimeStats} */ runtime) {
  if (runtime.mfaControlConnected) {
    return "Connected and signed in";
  }
  if (runtime.mfaReachable || runtime.mfaConnectionStatus === "reachable") {
    return "Reachable, but not signed in yet";
  }
  return "Can't reach the hub right now";
}

function fnnHint(/** @type {SidecarRuntimeStats} */ runtime) {
  if (runtime.fnnConnectionStatus === "simulated") {
    return "Running in demo mode — payments stay on this device";
  }
  if (runtime.fnnConnectionStatus === "online") {
    return "Connected to your local payment network";
  }
  return "Payment network is offline";
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
    {
      label: runtime.mfaControlConnected ? "Hub connected" : "Hub offline",
      status: connectionStatus(runtime, "mfa"),
    },
    {
      label: friendlyFnnStatus(runtime.fnnConnectionStatus),
      status: connectionStatus(runtime, "fnn"),
    },
    {
      label: `${runtime.meshChannelsActive ?? 0} payment links`,
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
        ${kpiCard("Status", "Offline", "Open the desktop app to load live updates", { status: "offline" })}
      </div>
    `;
  }

  return `
    <div class="dashboard-kpi-strip">
      ${kpiCard("Available balance", formatShannonsCompact(runtime.fnnTotalLiquidityShannons), formatFiatFromShannons(runtime.fnnTotalLiquidityShannons, runtime.fiatConversionRate), { status: "online" })}
      ${kpiCard("Payment links", `${runtime.meshChannelsActive ?? 0} / ${runtime.meshChannelsTotal ?? 0}`, "Links you can send and receive on", { status: (runtime.meshChannelsActive ?? 0) > 0 ? "online" : "neutral" })}
      ${kpiCard("Completed payments", formatCount(runtime.edgeSettled ?? 0, { label: "done" }), formatCount(runtime.edgePending ?? 0, { label: "waiting" }), { status: (runtime.edgeSettled ?? 0) > 0 ? "online" : "neutral" })}
      ${kpiCard("Waiting to sync", formatCount(runtime.queuedTelemetry ?? 0, { label: "updates" }), runtime.mfaControlConnected ? "Hub sync is on" : "Saved until the hub reconnects", { status: (runtime.queuedTelemetry ?? 0) > 0 ? "warn" : "online" })}
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
          <h3>Balance split</h3>
          <span class="aside-hint">How funds are split on your payment links</span>
        </div>

        <div
          class="liquidity-donut"
          style="--seg-out: ${localPct.toFixed(2)}%; --seg-in: ${(localPct + remotePct).toFixed(2)}%;"
        >
          <div class="liquidity-donut-hole">
            <strong title="${formatShannons(runtime.fnnTotalLiquidityShannons)}">${formatShannonsCompact(runtime.fnnTotalLiquidityShannons)}</strong>
            <span>total balance</span>
          </div>
        </div>

        <ul class="liquidity-legend">
          <li>
            <span class="liquidity-dot outbound"></span>
            <span class="liquidity-legend-copy">
              <strong>You can send</strong>
              <span>${localPct.toFixed(1)}% · ${formatShannons(local)}</span>
            </span>
          </li>
          <li>
            <span class="liquidity-dot inbound"></span>
            <span class="liquidity-legend-copy">
              <strong>You can receive</strong>
              <span>${remotePct.toFixed(1)}% · ${formatShannons(remote)}</span>
            </span>
          </li>
          <li>
            <span class="liquidity-dot cached"></span>
            <span class="liquidity-legend-copy">
              <strong>Saved link data</strong>
              <span>${formatCount(cached, { label: "saved links" })}</span>
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

/** @param {string[]} mounted @returns {string} */
function friendlyModuleList(mounted) {
  if (mounted.length === 0) return "None";
  return mounted
    .map((id) => {
      if (id === "dicoba") return "Group savings";
      if (id === "fiat_bridge") return "Mobile money";
      return id.replaceAll("_", " ");
    })
    .join(", ");
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
function renderMetricSections(runtime) {
  if (!runtime) {
    return dashboardMetricSection(
      "Status",
      "Live updates unavailable",
      metricCell("Status", "Offline", "Open the desktop app to load live status"),
    );
  }

  const mounted = runningModules(runtime);
  const runningHint =
    mounted.length === 0
      ? "No tools are running on this device"
      : "Tools currently running on this device";

  return [
    dashboardMetricSection(
      "Connections",
      "Hub, network, and partner links",
      [
        metricCell("Connected hub", runtime.mfaName ?? "—", mfaHint(runtime), {
          trend: runtime.mfaControlConnected === true,
        }),
        metricCell("Network status", friendlyFnnStatus(runtime.fnnConnectionStatus), fnnHint(runtime), {
          trend: runtime.fnnConnectionStatus !== "offline",
        }),
        metricCell("Network mode", friendlyFnnBackend(runtime.fnnBackend), "How this device moves money"),
        metricCell(
          "Partner",
          runtime.meshPeerAgentId ? `FA-${runtime.meshPeerAgentId}` : "—",
          shortPubkey(runtime.meshPeerPubkey),
        ),
        metricCell("Your network ID", shortPubkey(runtime.nodePubkey), "ID for this device on the network"),
      ].join(""),
    ),
    dashboardMetricSection(
      "Balances & payment links",
      "What you can send or receive",
      [
        metricCell(
          "Sendable balance",
          formatShannons(runtime.totalLocalBalanceShannons ?? 0),
          formatFiatFromShannons(runtime.totalLocalBalanceShannons ?? 0, runtime.fiatConversionRate),
          { trend: (runtime.totalLocalBalanceShannons ?? 0) > 0 },
        ),
        metricCell(
          "Receivable balance",
          formatShannons(runtime.totalRemoteBalanceShannons ?? 0),
          formatFiatFromShannons(runtime.totalRemoteBalanceShannons ?? 0, runtime.fiatConversionRate),
          { trend: (runtime.totalRemoteBalanceShannons ?? 0) > 0 },
        ),
        metricCell(
          "Payment links",
          `${runtime.meshChannelsActive ?? 0} / ${runtime.meshChannelsTotal ?? 0}`,
          "Open links ready for payments",
          { trend: (runtime.meshChannelsActive ?? 0) > 0 },
        ),
      ].join(""),
    ),
    dashboardMetricSection(
      "Payment history",
      "Payments recorded on this device",
      [
        metricCell(
          "Completed payments",
          formatCount(runtime.edgeSettled ?? 0, { label: "done" }),
          "Fully confirmed",
          { trend: (runtime.edgeSettled ?? 0) > 0 },
        ),
        metricCell(
          "Waiting payments",
          formatCount(runtime.edgePending, { label: "waiting" }),
          "Still processing",
        ),
        metricCell(
          "Failed payments",
          formatCount(runtime.edgeFailed ?? 0, { label: "failed" }),
          "Cancelled or rejected",
        ),
        metricCell(
          "Waiting to sync",
          formatCount(runtime.queuedTelemetry, { label: "updates" }),
          "Updates waiting while the hub is offline",
        ),
      ].join(""),
    ),
    dashboardMetricSection(
      "This device",
      "Device profile and installed tools",
      [
        metricCell("Active agent", formatAgentLabel(runtime), (runtime.hardwareProfile ?? "").replaceAll("_", " "), {
          trend: true,
        }),
        metricCell(
          "Performance mode",
          runtime.powerProfile ?? "—",
          "How hard this device is working",
          { trend: runtime.powerProfile === "AggressiveRealTime" },
        ),
        metricCell("App profile", runtime.sidecarProfile ?? "unknown", "Which tools this device is set up for"),
        metricCell("Active tools", friendlyModuleList(mounted), runningHint),
        ...(mounted.includes("dicoba") && runtime.dicobaMemberId
          ? [
              metricCell(
                "Your DiCoBa ID",
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
          No tools are <strong>running</strong> on this device.
          Open <strong>App Store</strong> to install tools, or switch the device profile.
        </div>`
      : "";

  const mfaOobBanner =
    runtime && !runtime.mfaControlConnected
      ? `<div class="dashboard-banner dashboard-banner-warn" role="status">
          Hub is offline — use <strong>Send offline</strong> below to share actions by QR or link.
        </div>`
      : "";

  return `
    <section class="dashboard-page" data-dashboard-stats>
      <header class="dashboard-hero">
        <div class="dashboard-hero-copy">
          <p class="dashboard-eyebrow">Overview</p>
          <h1>Fiber Agent</h1>
          <p>
            Live status for ${agentLabel}: connections, balances, and payment sync.
            If cash on hand runs low, the hub can arrange a refill.
          </p>
          ${renderStatusPills(runtime)}
          <div class="dashboard-hero-actions">
            <button type="button" class="hero-btn hero-btn-primary" data-action="refresh-dashboard-stats">
              ${icon("dashboard", 16)}
              Refresh
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
