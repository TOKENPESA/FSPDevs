/**
 * MFA policy plugin store client — authenticated HTTP to supervisor :1025.
 * Mirrors FA's fa-module-store-api.js pattern (FA uses Tauri IPC in desktop shell).
 */

import {
  MFA_MODULE_API_BASE_URL,
  mfaAuthHeaders,
} from "../../dashboard/config.js";
import { createModuleApiClient } from "../../dashboard/dashboard-module-api.js";

/** @typedef {import("../../dashboard/dashboard-module-api.js").ModuleApiClient} ModuleApiClient */

/** @type {ModuleApiClient | null} */
let cached = null;

/** @returns {ModuleApiClient} */
export function getMfaModuleApi() {
  if (!cached) {
    cached = createModuleApiClient({
      baseUrl: MFA_MODULE_API_BASE_URL,
      buildHeaders: () => mfaAuthHeaders({ Accept: "application/json" }),
    });
  }
  return cached;
}
