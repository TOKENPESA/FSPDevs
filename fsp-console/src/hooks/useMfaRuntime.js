import { useCallback, useEffect, useState } from "react";
import { fetchMfaHealth } from "../api/mfa.js";

/**
 * @typedef {Object} MfaRuntime
 * @property {boolean} online
 * @property {string} [error]
 * @property {number} connectedAgents
 * @property {number[]} connectedAgentIds
 * @property {string[]} runningPlugins
 * @property {string} hubRpcUrl
 * @property {number | null} hubFunding
 * @property {number} simulationEdgeNodes
 * @property {boolean} clearingRegionalReady
 * @property {string[]} assetCorridors
 */

/** @returns {MfaRuntime} */
function offlineState(error = "MFA unreachable") {
  return {
    online: false,
    error,
    connectedAgents: 0,
    connectedAgentIds: [],
    runningPlugins: [],
    hubRpcUrl: "—",
    hubFunding: null,
    simulationEdgeNodes: 1024,
    clearingRegionalReady: false,
    assetCorridors: [],
  };
}

export function useMfaRuntime(pollMs = 5000) {
  const [runtime, setRuntime] = useState(/** @type {MfaRuntime} */ (offlineState("")));

  const refresh = useCallback(async () => {
    try {
      const health = await fetchMfaHealth();
      const clearing = /** @type {Record<string, unknown>} */ (health.clearing ?? {});
      const hub = /** @type {Record<string, unknown>} */ (health.hub ?? {});
      const registry = /** @type {Record<string, unknown>} */ (health.asset_registry ?? {});

      setRuntime({
        online: true,
        connectedAgents: Number(health.connected_agents ?? 0),
        connectedAgentIds: Array.isArray(health.connected_agent_ids)
          ? health.connected_agent_ids
          : [],
        runningPlugins: Array.isArray(health.running_plugins)
          ? health.running_plugins.map(String)
          : [],
        hubRpcUrl: String(hub.rpc_url ?? "—"),
        hubFunding:
          typeof hub.funding_allocation_shannons === "number"
            ? hub.funding_allocation_shannons
            : null,
        simulationEdgeNodes: Number(health.simulation_edge_nodes ?? 1024),
        clearingRegionalReady: clearing.regional_env_ready === true,
        assetCorridors: Array.isArray(registry.corridors)
          ? registry.corridors.map(String)
          : [],
      });
    } catch (err) {
      setRuntime(
        offlineState(err instanceof Error ? err.message : "MFA unreachable"),
      );
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), pollMs);
    return () => clearInterval(id);
  }, [pollMs, refresh]);

  return { runtime, refresh };
}
