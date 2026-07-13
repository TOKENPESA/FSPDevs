import { fetchHubHealth } from "../../../../dashboard/api/mfa.js";
import { updateHubPanel } from "../../../../dashboard/events/liquidity.js";
import { state } from "../../../../dashboard/state.js";
import { escapeHtml } from "../../dom-security.js";
import { loadMfaRuntime } from "../../mfa-runtime.js";
import { formatCount, metricSection, metricCell } from "../../stats-ui.js";

/** @typedef {import('../../types.js').MfaRuntimeDetail} MfaRuntimeDetail */

export const liquidityPanel = {
  id: "mfa-liquidity",
  title: "Hub & Liquidity",
  navLabel: "Hub status",
  navIcon: "liquidity",
  badge: "hub",
  navDescription:
    "FNN hub RPC, channel funding, regional float-crisis clearing, and enterprise balance-depletion refuel.",
  render() {
    return `
      <div class="workspace-card">
        <div class="workspace-card-head">
          <h2>FNN hub</h2>
          <p class="panel-hint">Liquidity copilot provisions channels when sidecars report low float</p>
        </div>
        <div class="mesh-control-row" style="margin-bottom:0.75rem">
          <button type="button" class="panel-btn panel-btn-primary" data-action="refresh-hub">Refresh hub</button>
        </div>
        <div class="metric-grid" data-hub-metrics>
          ${hubMetricsHtml()}
        </div>
      </div>
      <div class="workspace-card" style="margin-top:1rem">
        <div class="workspace-card-head">
          <h2>Clearing planes</h2>
          <p class="panel-hint">
            Regional float-crisis via <code>/clearing/float-crisis</code> ·
            Enterprise refuel via <code>TelemetryPacket</code> → <code>EnterpriseClearinghouse</code>
          </p>
        </div>
        <div class="metric-grid" data-clearing-metrics>
          ${clearingMetricsPlaceholder()}
        </div>
      </div>`;
  },
  renderAside() {
    return metricSection(
      "Liquidity counters",
      [
        metricCell("Injections", String(state.liquidity.injections), "Successful funds"),
        metricCell("In flight", String(state.liquidity.inFlight), "Pending operations"),
        metricCell("Faucet hints", String(state.liquidity.faucetHints), "Needs testnet CKB"),
        metricCell("Failed", String(state.liquidity.failed), "Errors"),
      ].join(""),
    );
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    const paintClearing = async () => {
      const runtime = await loadMfaRuntime({ force: true });
      const metrics = root.querySelector("[data-clearing-metrics]");
      if (metrics) metrics.innerHTML = clearingMetricsHtml(runtime);
    };

    void paintClearing();

    root.querySelector("[data-action='refresh-hub']")?.addEventListener("click", async () => {
      await fetchHubHealth();
      updateHubPanel();
      const metrics = root.querySelector("[data-hub-metrics]");
      if (metrics) metrics.innerHTML = hubMetricsHtml();
      await paintClearing();
    });
  },
};

function hubMetricsHtml() {
  return [
    metricCell("Hub RPC", state.hub.rpcUrl ?? "—", "FNN JSON-RPC endpoint"),
    metricCell("Channel funding", state.hub.fundingShannons ?? "—", "Per-channel allocation"),
    metricCell("Sidecar alerts", String(state.hub.sidecarAlerts ?? "—"), "Balance watch policy"),
    metricCell("Last event", escapeHtml(state.liquidity.lastEvent || "—"), "Most recent liquidity action"),
  ].join("");
}

function clearingMetricsPlaceholder() {
  return metricCell("Clearing", "Loading…", "Fetch MFA health for env readiness");
}

/**
 * @param {MfaRuntimeDetail | null} runtime
 */
function clearingMetricsHtml(runtime) {
  if (!runtime) {
    return metricCell("Clearing", "Unavailable", "MFA health unreachable");
  }

  return [
    metricCell(
      "Regional env",
      runtime.clearingRegionalReady ? "Ready" : "Blocked",
      runtime.clearingHint,
      { trend: runtime.clearingRegionalReady },
    ),
    metricCell(
      "Enterprise vault",
      runtime.clearingCorporateVault ?? "—",
      runtime.clearingEnterprisePath ?? "BalanceDepleted → FNN intent swap",
      { trend: Boolean(runtime.clearingCorporateVault) },
    ),
    metricCell(
      "Asset corridors",
      formatCount(runtime.assetCorridors?.length ?? 0, { label: "ISO codes" }),
      (runtime.assetCorridors ?? []).join(" · ") || "AssetRegistryHub empty",
      { trend: (runtime.assetCorridors?.length ?? 0) > 0 },
    ),
    metricCell(
      "Topology WAL",
      "mesh_topology_journal.wal",
      "Pulse journal appended per telemetry packet",
    ),
  ].join("");
}
