import { createLogger } from "../dashboard/logger.js";
import dicobaModule from "./modules/dicoba/index.js";
import fiatBridgeModule from "./modules/fiat-bridge/index.js";

const log = createLogger("module-registry");

/** @typedef {import("../../../../dashboard/types.js").SidecarModule} SidecarModule */

/** All UI modules that may be mounted when the backend profile enables them. */
/** @type {Map<string, SidecarModule>} */
export const MODULE_CATALOG = new Map([
  ["dicoba", dicobaModule],
  ["fiat_bridge", fiatBridgeModule],
]);

const KNOWN_MODULE_IDS = new Set(MODULE_CATALOG.keys());

/**
 * Return UI module descriptors for backend-mounted module ids only.
 * Unknown ids from the server are ignored (fail-closed on the client).
 * @param {string[]} [mountedIds]
 * @returns {SidecarModule[]}
 */
export function modulesForMounted(mountedIds = []) {
  const ordered = [];
  for (const id of mountedIds) {
    if (!KNOWN_MODULE_IDS.has(id)) {
      log.warn(`ignoring unknown mounted module: ${id}`);
      continue;
    }
    const module = MODULE_CATALOG.get(id);
    if (module) {
      ordered.push(module);
    }
  }
  return ordered;
}
