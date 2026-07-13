/**
 * FA module store client — Tauri IPC in desktop shell, HTTP fallback otherwise.
 */

import { faModuleApi as httpFaModuleApi } from "../dashboard/dashboard-module-api.js";
import { hasTauri } from "./sidecar-api.js";

/** @typedef {import("../dashboard/dashboard-module-api.js").ModuleApiClient} ModuleApiClient */

const invoke = window.__TAURI__?.core?.invoke;

/** @typedef {import("../dashboard/dashboard-module-api.js").InstalledModuleRecord} InstalledModuleRecord */

/** @type {ModuleApiClient} */
const tauriFaModuleApi = {
  async fetchCatalog() {
    if (!invoke) {
      throw new Error("Tauri runtime unavailable");
    }
    const modules = await invoke("fetch_module_catalog");
    if (!Array.isArray(modules)) {
      throw new Error("catalog payload missing modules array");
    }
    return modules;
  },

  async fetchInstalled() {
    if (!invoke) {
      throw new Error("Tauri runtime unavailable");
    }
    const installed = await invoke("fetch_installed_modules");
    if (!Array.isArray(installed)) {
      throw new Error("installed payload missing installed array");
    }
    return installed;
  },

  async installModule(moduleName, configJson) {
    if (!invoke) {
      throw new Error("Tauri runtime unavailable");
    }
    return /** @type {InstalledModuleRecord} */ (
      await invoke("install_sidecar_module", {
        request: {
          moduleName,
          config: configJson ?? {},
        },
      })
    );
  },

  async uninstallModule(moduleName) {
    if (!invoke) {
      throw new Error("Tauri runtime unavailable");
    }
    await invoke("uninstall_sidecar_module", {
      request: { moduleName },
    });
  },

  async toggleModule(moduleName, isActive) {
    if (!invoke) {
      throw new Error("Tauri runtime unavailable");
    }
    return /** @type {InstalledModuleRecord} */ (
      await invoke("toggle_sidecar_module", {
        request: { moduleName, isActive },
      })
    );
  },
};

/** @returns {ModuleApiClient} */
export function getFaModuleApi() {
  return hasTauri() ? tauriFaModuleApi : httpFaModuleApi;
}
