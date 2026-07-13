import { createLogger } from "../../dashboard/logger.js";
import { fetchMfaHealth } from "./mfa-api.js";

/** @typedef {import('./types.js').MfaRuntimeDetail} MfaRuntimeDetail */

const log = createLogger("mfa-runtime");

export const MFA_RUNTIME_EVENT = "mfa-runtime-updated";

/** @type {MfaRuntimeDetail | null} */
let cache = null;

/** @returns {MfaRuntimeDetail | null} */
export function getMfaRuntime() {
  return cache;
}

/**
 * @param {Record<string, unknown>} clearing
 */
function clearingEnvHint(clearing) {
  if (!clearing) return "Clearing config unavailable";
  if (clearing.regional_env_ready) {
    if (clearing.regional_mock_active) {
      return "Regional mock active · enterprise refuel via TelemetryPacket";
    }
    return "Regional telco API configured · enterprise refuel live";
  }
  return `Set ${clearing.telco_api_env ?? "MFA_TELCO_CLEARING_API_URL"} or ${clearing.telco_mock_env ?? "MFA_TELCO_CLEARING_MOCK"}`;
}

/**
 * @param {{ force?: boolean }} [options]
 * @returns {Promise<MfaRuntimeDetail | null>}
 */
export async function loadMfaRuntime({ force = false } = {}) {
  if (!force && cache) return cache;

  try {
    const health = await fetchMfaHealth();
    const { state } = await import("../../dashboard/state.js");
    const clearing = /** @type {Record<string, unknown>} */ (health.clearing ?? {});
    const registry = /** @type {Record<string, unknown>} */ (health.asset_registry ?? {});
    const auth = /** @type {Record<string, unknown>} */ (health.auth ?? {});

    cache = {
      service: health.service ?? "Master Fiber Agent",
      hubRpcUrl: health.hub?.rpc_url ?? "—",
      hubFunding: health.hub?.funding_allocation_shannons ?? null,
      simulationEdgeNodes: health.simulation_edge_nodes ?? state.networkSize,
      connectedAgents: health.connected_agents ?? 0,
      connectedAgentIds: Array.isArray(health.connected_agent_ids)
        ? health.connected_agent_ids
        : [],
      monitorConnected: state.ws?.readyState === WebSocket.OPEN,
      monitorLiveNodes: state.comm.nodes.size,
      offlineNodes: state.dead.size,
      healCount: state.healCount,
      liquidityInjections: state.liquidity.injections,
      clearingRegionalReady: clearing.regional_env_ready === true,
      clearingMockActive: clearing.regional_mock_active === true,
      clearingCorporateVault:
        typeof clearing.corporate_treasury_vault === "string"
          ? clearing.corporate_treasury_vault
          : "corporate-clearing-vault",
      clearingEnterprisePath:
        typeof clearing.enterprise_balance_depletion === "string"
          ? clearing.enterprise_balance_depletion
          : "TelemetryPacket → EnterpriseClearinghouse",
      clearingTopologyJournal:
        typeof clearing.topology_journal === "string"
          ? clearing.topology_journal
          : "mesh_topology_journal.wal",
      assetCorridors: Array.isArray(registry.corridors) ? registry.corridors : [],
      runningPlugins: Array.isArray(health.running_plugins)
        ? health.running_plugins.map((/** @type {unknown} */ name) => String(name))
        : [],
      complianceTicketTtl:
        typeof auth.ticket_ttl_secs === "number" ? auth.ticket_ttl_secs : 30,
      collectedAtUnix: Math.floor(Date.now() / 1000),
      clearingHint: clearingEnvHint(clearing),
    };
  } catch (error) {
    log.warn("health unavailable", error);
    const { state } = await import("../../dashboard/state.js");
    cache = {
      service: "Master Fiber Agent",
      hubRpcUrl: "—",
      hubFunding: null,
      simulationEdgeNodes: state.networkSize,
      connectedAgents: 0,
      connectedAgentIds: [],
      monitorConnected: false,
      monitorLiveNodes: state.comm.nodes.size,
      offlineNodes: state.dead.size,
      healCount: state.healCount,
      liquidityInjections: state.liquidity.injections,
      clearingRegionalReady: false,
      clearingMockActive: false,
      clearingCorporateVault: "—",
      clearingEnterprisePath: "—",
      clearingTopologyJournal: "mesh_topology_journal.wal",
      assetCorridors: [],
      runningPlugins: [],
      complianceTicketTtl: 30,
      collectedAtUnix: null,
      clearingHint: "MFA unreachable — start fnn-testnet/start-live-mfa.ps1",
      error: "MFA unreachable",
    };
  }

  window.dispatchEvent(new CustomEvent(MFA_RUNTIME_EVENT, { detail: cache }));
  return cache;
}

/** @type {number | null} */
let runtimeWatcher = null;

/**
 * @param {number} [intervalMs]
 */
export function startMfaRuntimeWatcher(intervalMs = 5000) {
  if (runtimeWatcher) return;
  runtimeWatcher = window.setInterval(() => {
    void loadMfaRuntime({ force: true });
  }, intervalMs);
}
