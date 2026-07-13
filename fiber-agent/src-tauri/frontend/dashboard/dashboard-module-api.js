/**
 * Hot-swap module / plugin registry API — FA sidecar (:19444) and MFA supervisor (:1025).
 */

import {
  FA_MODULE_API_BASE_URL,
  MFA_MODULE_API_BASE_URL,
  mfaAuthHeaders,
} from "./config.js";
import { DEFAULT_FETCH_TIMEOUT_MS, fetchWithTimeout } from "./fetch-timeout.js";

/** @typedef {Object} ModuleCatalogEntry
 * @property {string} module_id
 * @property {string} module_name
 * @property {string} [description]
 * @property {string} [kind]
 * @property {string[]} [rpc_methods]
 */

/** @typedef {Object} InstalledModuleRecord
 * @property {string} id
 * @property {string} module_name
 * @property {boolean} is_active
 * @property {Record<string, unknown>} [config]
 */

/** @typedef {Object} ModuleApiClientOptions
 * @property {string} baseUrl
 * @property {() => Record<string, string>} [buildHeaders]
 * @property {number} [timeoutMs]
 */

/** @typedef {Object} ModuleApiClient
 * @property {() => Promise<ModuleCatalogEntry[]>} fetchCatalog
 * @property {() => Promise<InstalledModuleRecord[]>} fetchInstalled
 * @property {(moduleName: string, configJson: Record<string, unknown>) => Promise<InstalledModuleRecord>} installModule
 * @property {(moduleName: string) => Promise<void>} uninstallModule
 * @property {(moduleName: string, isActive: boolean) => Promise<InstalledModuleRecord>} toggleModule
 */

const PATHS = {
  catalog: "/api/modules/catalog",
  installed: "/api/modules/installed",
  install: "/api/modules/install",
  uninstall: "/api/modules/uninstall",
  toggle: "/api/modules/toggle",
};

/**
 * @param {Response} res
 * @param {string} action
 */
async function parseJsonResponse(res, action) {
  const text = await res.text();
  /** @type {Record<string, unknown>} */
  let body = {};
  if (text.trim()) {
    try {
      body = /** @type {Record<string, unknown>} */ (JSON.parse(text));
    } catch {
      throw new Error(`${action} returned non-JSON response (${res.status})`);
    }
  }
  if (!res.ok) {
    const reason =
      typeof body.reason === "string"
        ? body.reason
        : typeof body.error === "string"
          ? body.error
          : `${action} failed (${res.status})`;
    throw new Error(reason);
  }
  return body;
}

/**
 * @param {ModuleApiClientOptions} options
 * @returns {ModuleApiClient}
 */
export function createModuleApiClient({
  baseUrl,
  buildHeaders = () => ({}),
  timeoutMs = DEFAULT_FETCH_TIMEOUT_MS,
}) {
  const origin = baseUrl.replace(/\/$/, "");

  /**
   * @param {string} path
   * @param {RequestInit} [init]
   */
  async function request(path, init = {}) {
    /** @type {Record<string, string>} */
    const headers = {
      Accept: "application/json",
      ...buildHeaders(),
      ...(/** @type {Record<string, string>} */ (init.headers ?? {})),
    };
    if (init.body && !("Content-Type" in headers)) {
      headers["Content-Type"] = "application/json";
    }
    const res = await fetchWithTimeout(
      `${origin}${path}`,
      { ...init, headers, mode: "cors" },
      timeoutMs,
    );
    return parseJsonResponse(res, path);
  }

  return {
    async fetchCatalog() {
      const body = await request(PATHS.catalog, { method: "GET" });
      const modules = body.modules;
      if (!Array.isArray(modules)) {
        throw new Error("catalog payload missing modules array");
      }
      return /** @type {ModuleCatalogEntry[]} */ (modules);
    },

    async fetchInstalled() {
      const body = await request(PATHS.installed, { method: "GET" });
      const installed = body.installed;
      if (!Array.isArray(installed)) {
        throw new Error("installed payload missing installed array");
      }
      return /** @type {InstalledModuleRecord[]} */ (installed);
    },

    async installModule(moduleName, configJson) {
      const body = await request(PATHS.install, {
        method: "POST",
        body: JSON.stringify({
          module_name: moduleName,
          config: configJson ?? {},
        }),
      });
      return {
        id: String(body.id ?? ""),
        module_name: String(body.module_name ?? moduleName),
        is_active: body.is_active !== false,
        config: configJson,
      };
    },

    async uninstallModule(moduleName) {
      await request(PATHS.uninstall, {
        method: "POST",
        body: JSON.stringify({ module_name: moduleName }),
      });
    },

    async toggleModule(moduleName, isActive) {
      const body = await request(PATHS.toggle, {
        method: "PUT",
        body: JSON.stringify({
          module_name: moduleName,
          is_active: isActive,
        }),
      });
      return {
        id: String(body.id ?? ""),
        module_name: String(body.module_name ?? moduleName),
        is_active: Boolean(body.is_active),
      };
    },
  };
}

/** MFA policy plugin registry (auth required). */
export const mfaModuleApi = createModuleApiClient({
  baseUrl: MFA_MODULE_API_BASE_URL,
  buildHeaders: () => mfaAuthHeaders(),
});

/** FA edge module registry (local sidecar API). */
export const faModuleApi = createModuleApiClient({
  baseUrl: FA_MODULE_API_BASE_URL,
});
