/** @returns {boolean} */
function isBrowser() {
  return typeof window !== "undefined" && typeof window.location !== "undefined";
}

/** @param {string} hostname */
export function isLoopbackHostname(hostname) {
  return hostname === "127.0.0.1" || hostname === "localhost" || hostname === "[::1]";
}

/** Public MFA console hosts (nginx TLS). */
const PUBLIC_MFA_HOSTS = new Set([
  "mfa.fsprotocol.com",
  "fsprotocol.com",
  "www.fsprotocol.com",
]);

/** @param {string} [hostname] */
export function isPublicMfaHostname(hostname = isBrowser() ? window.location.hostname : "") {
  return PUBLIC_MFA_HOSTS.has(hostname);
}

/**
 * Resolve MFA HTTP base for the current page.
 * - Tauri desktop/mobile shell → live TLS MFA (Android cleartext policy)
 * - Production console (same host as nginx) → same origin (HTTPS)
 * - Local browser dashboard on loopback → MFA on :1025
 */
function resolveMfaHttpBase() {
  if (!isBrowser()) {
    return "https://mfa.fsprotocol.com";
  }
  // Tauri webview (file://, tauri.localhost, custom protocol) → secure MFA
  if (typeof window.__TAURI__ !== "undefined") {
    return "https://mfa.fsprotocol.com";
  }
  const { protocol, hostname, origin } = window.location;
  if (protocol === "file:" || hostname === "tauri.localhost") {
    return "https://mfa.fsprotocol.com";
  }
  if (isPublicMfaHostname(hostname)) {
    return origin;
  }
  if (isLoopbackHostname(hostname)) {
    return "http://127.0.0.1:1025";
  }
  // Reverse-proxied unknown host: same origin
  return origin;
}

export const MFA_API_BASE_URL = resolveMfaHttpBase();
export const FA_MODULE_API_BASE_URL = "http://127.0.0.1:19444";
export const MFA_MODULE_API_BASE_URL = MFA_API_BASE_URL;
export const MFA_HEALTH_URL = `${MFA_API_BASE_URL}/`;
export const MFA_SIMULATION_URL = `${MFA_API_BASE_URL}/simulation`;
export const MFA_ROUTE_URL = `${MFA_API_BASE_URL}/route`;
export const MFA_SURVEILLANCE_URL = `${MFA_API_BASE_URL}/api/v1/compliance/stream`;
export const MFA_COMPLIANCE_TICKET_URL = `${MFA_API_BASE_URL}/compliance/ticket`;
export const EDGE_NODES_STORAGE_KEY = "fspdevs-edge-nodes";
export const MFA_API_TOKEN_STORAGE_KEY = "fspdevs-mfa-api-token";
export const DEFAULT_MFA_API_TOKEN = "fspdevs-local-api-devonly";
export const RING_MAX = 1024;
export const COMM_TTL_MS = 30_000;
export const PAYMENT_TRAVEL_CAP = 0.92;
export const PAYMENT_SETTLE_DISPLAY_MS = 8_000;

export const MFA_HUB = { x: 52, y: 52 };

export const PATH_STYLES = {
  ring: { color: "#50ff9a", width: 2.8, dash: [7, 5], speed: 0.004, label: "Ring +1" },
  skip: { color: "#5eb5ff", width: 2.8, dash: [9, 6], speed: 0.005, label: "Skip +2" },
  chord: { color: "#c678ff", width: 2.8, dash: [5, 7], speed: 0.003, label: "Opposite" },
  mfa: { color: "#ffb347", width: 2.2, dash: [11, 9], speed: 0.006, label: "MFA uplink" },
};

export const COMM_STYLE = {
  mesh: { color: "#00e5ff", width: 2.4, dash: [6, 4], speed: 0.007 },
  heal: { color: "#00d4ff", width: 3, dash: [4, 3], speed: 0.009 },
  mfa: PATH_STYLES.mfa,
};

export function mfaApiToken() {
  try {
    return localStorage.getItem(MFA_API_TOKEN_STORAGE_KEY) || DEFAULT_MFA_API_TOKEN;
  } catch {
    return DEFAULT_MFA_API_TOKEN;
  }
}

/** Persist API token (production console needs MFA_API_TOKEN from the droplet). */
/** @param {string} token */
export function setMfaApiToken(token) {
  const value = String(token ?? "").trim();
  if (!value) return;
  try {
    localStorage.setItem(MFA_API_TOKEN_STORAGE_KEY, value);
  } catch {
    // ignore quota / private mode
  }
}

/**
 * Optional one-shot seed: ?mfa_token=… (stripped from the address bar after save).
 */
export function seedMfaApiTokenFromQuery() {
  if (!isBrowser()) return;
  try {
    const url = new URL(window.location.href);
    const fromQuery = url.searchParams.get("mfa_token");
    if (!fromQuery) return;
    setMfaApiToken(fromQuery);
    url.searchParams.delete("mfa_token");
    const next = `${url.pathname}${url.search}${url.hash}`;
    window.history.replaceState({}, "", next);
  } catch {
    // ignore
  }
}

export function mfaAuthHeaders(extra = {}) {
  return {
    Authorization: `Bearer ${mfaApiToken()}`,
    ...extra,
  };
}

/** @param {string} baseUrl @returns {string} */
export function mfaAuthedUrl(baseUrl) {
  const url = new URL(baseUrl);
  url.searchParams.set("token", mfaApiToken());
  return url.toString();
}

/** Host label for sidebar / metrics (e.g. mfa.fsprotocol.com or 127.0.0.1:1025). */
export function mfaDisplayHost() {
  try {
    return new URL(MFA_API_BASE_URL).host;
  } catch {
    return "mfa.fsprotocol.com";
  }
}

/** Bare monitor WS URL (no token query). */
export function mfaMonitorWsBaseUrl() {
  const base = MFA_API_BASE_URL.endsWith("/") ? MFA_API_BASE_URL : `${MFA_API_BASE_URL}/`;
  const u = new URL("ws/monitor", base);
  u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
  return u.toString();
}

/** Monitor WS URL with API token query (skipped on public host with default local token). */
export function mfaMonitorWsUrl() {
  const base = mfaMonitorWsBaseUrl();
  if (isPublicMfaHostname() && mfaApiToken() === DEFAULT_MFA_API_TOKEN) {
    return base;
  }
  return mfaAuthedUrl(base);
}
