import {
  MFA_API_BASE_URL,
  MFA_COMPLIANCE_TICKET_URL,
  MFA_HEALTH_URL,
  MFA_ROUTE_URL,
  MFA_SIMULATION_URL,
  MFA_SURVEILLANCE_URL,
  mfaAuthHeaders,
  mfaAuthedUrl,
} from "../../dashboard/config.js";
import { fetchWithTimeout } from "../../dashboard/fetch-timeout.js";

export {
  MFA_API_BASE_URL,
  MFA_HEALTH_URL,
  MFA_SIMULATION_URL,
  MFA_ROUTE_URL,
  mfaAuthHeaders,
  mfaAuthedUrl,
};

export async function fetchMfaHealth(timeoutMs = 5000) {
  const res = await fetchWithTimeout(
    MFA_HEALTH_URL,
    { mode: "cors" },
    timeoutMs,
  );
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/**
 * @param {number} edgeNodes
 */
export async function postSimulation(edgeNodes) {
  const res = await fetchWithTimeout(MFA_SIMULATION_URL, {
    method: "POST",
    headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify({ edge_nodes: edgeNodes }),
  });
  if (!res.ok) throw new Error(`Simulation sync failed (${res.status})`);
  return res.json();
}

/**
 * @param {{
 *   source: number,
 *   destination: number,
 *   amountShannons: number,
 *   activeNetworkLimit: number,
 *   execute?: boolean,
 * }} params
 */
export async function postRoute({
  source,
  destination,
  amountShannons,
  activeNetworkLimit,
  execute = true,
}) {
  const res = await fetchWithTimeout(MFA_ROUTE_URL, {
    method: "POST",
    headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify({
      source,
      destination,
      amount_shannons: amountShannons,
      active_network_limit: activeNetworkLimit,
      execute,
    }),
  });
  return res.json();
}

/**
 * @param {Record<string, unknown>} payload
 */
export async function postFloatCrisisClearing(payload) {
  const res = await fetchWithTimeout(`${MFA_API_BASE_URL}/clearing/float-crisis`, {
    method: "POST",
    headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error(`Clearing failed (${res.status})`);
  return res.json();
}

export async function requestComplianceTicket() {
  const res = await fetchWithTimeout(MFA_COMPLIANCE_TICKET_URL, {
    method: "POST",
    headers: mfaAuthHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify({}),
  });
  if (!res.ok) throw new Error(`Ticket request failed (${res.status})`);
  return res.json();
}

/**
 * @param {string} ticket
 */
export function complianceStreamUrl(ticket) {
  const url = new URL(MFA_SURVEILLANCE_URL);
  url.searchParams.set("ticket", ticket);
  return url.toString();
}

export function monitorWebSocketUrl() {
  return mfaAuthedUrl("ws://127.0.0.1:1025/ws/monitor");
}
