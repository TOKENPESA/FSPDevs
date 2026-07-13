import { DEFAULT_FETCH_TIMEOUT_MS, fetchWithTimeout } from "./fetch-timeout.js";

const MFA_API_TOKEN = "fspdevs-local-api-devonly";

/** Use Vite proxy on local console ports; direct :1025 elsewhere (needs MFA CORS). */
function resolveMfaBase() {
  if (typeof window === "undefined") {
    return import.meta.env.DEV ? "/mfa-api" : "http://127.0.0.1:1025";
  }
  const { hostname, port } = window.location;
  const loopback = hostname === "127.0.0.1" || hostname === "localhost" || hostname === "[::1]";
  if (loopback && (port === "5173" || import.meta.env.DEV)) {
    return "/mfa-api";
  }
  return "http://127.0.0.1:1025";
}

const MFA_BASE = resolveMfaBase();

/** @typedef {{ module_id: string, module_name: string, description?: string, kind?: string }} CatalogEntry */
/** @typedef {{ id: string, module_name: string, is_active: boolean, config?: Record<string, unknown> }} InstalledRecord */

function authHeaders(extra = {}) {
  let token = MFA_API_TOKEN;
  try {
    token = localStorage.getItem("fspdevs-mfa-api-token") || MFA_API_TOKEN;
  } catch {
    /* ignore */
  }
  return {
    Authorization: `Bearer ${token}`,
    Accept: "application/json",
    ...extra,
  };
}

/**
 * @param {string} path
 * @param {RequestInit} [init]
 */
async function mfaRequest(path, init = {}) {
  const res = await fetchWithTimeout(
    `${MFA_BASE}${path}`,
    {
      ...init,
      mode: "cors",
      headers: {
        ...authHeaders(),
        ...(init.headers ?? {}),
      },
    },
    DEFAULT_FETCH_TIMEOUT_MS,
  );
  const text = await res.text();
  /** @type {Record<string, unknown>} */
  let body = {};
  if (text.trim()) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new Error(`MFA returned non-JSON (${res.status})`);
    }
  }
  if (!res.ok) {
    const reason =
      typeof body.reason === "string"
        ? body.reason
        : typeof body.error === "string"
          ? body.error
          : `MFA request failed (${res.status})`;
    throw new Error(reason);
  }
  return body;
}

export async function fetchMfaHealth() {
  const res = await fetchWithTimeout(`${MFA_BASE}/`, { mode: "cors" }, 5000);
  if (!res.ok) throw new Error(`MFA health HTTP ${res.status}`);
  return res.json();
}

export const mfaModuleApi = {
  async fetchCatalog() {
    const body = await mfaRequest("/api/modules/catalog");
    const modules = body.modules;
    if (!Array.isArray(modules)) throw new Error("catalog missing modules array");
    return /** @type {CatalogEntry[]} */ (modules);
  },

  async fetchInstalled() {
    const body = await mfaRequest("/api/modules/installed");
    const installed = body.installed;
    if (!Array.isArray(installed)) throw new Error("installed missing array");
    return /** @type {InstalledRecord[]} */ (installed);
  },

  async installModule(moduleName, configJson = {}) {
    await mfaRequest("/api/modules/install", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ module_name: moduleName, config: configJson }),
    });
  },

  async uninstallModule(moduleName) {
    await mfaRequest("/api/modules/uninstall", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ module_name: moduleName }),
    });
  },

  async toggleModule(moduleName, isActive) {
    await mfaRequest("/api/modules/toggle", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ module_name: moduleName, is_active: isActive }),
    });
  },
};

/** @type {Record<string, string>} */
export const PLUGIN_DISPLAY_NAMES = {
  lume_pricing: "LumePricingEngine",
  sovereign_compliance: "SovereignComplianceFilter",
  automated_refueling: "AutomatedRefuelingBrain",
  clearinghouse_swap: "ClearinghouseSwapModule",
};

/**
 * @param {CatalogEntry[]} catalog
 * @param {InstalledRecord[]} installed
 */
export function mergePluginRegistry(catalog, installed) {
  const installedById = new Map(
    installed.map((row) => [row.module_name.toLowerCase(), row]),
  );

  return catalog.map((entry) => {
    const id = entry.module_id || entry.module_name;
    const key = id.toLowerCase();
    const row = installedById.get(key);
    return {
      id: key,
      name: PLUGIN_DISPLAY_NAMES[key] ?? entry.module_name ?? id,
      kind: entry.kind ?? "policy",
      description: entry.description ?? "",
      installed: Boolean(row),
      mounted: row?.is_active !== false && Boolean(row),
      config: row?.config ? JSON.stringify(row.config, null, 2) : "{}",
    };
  });
}
