/** @typedef {import("../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const invoke = window.__TAURI__?.core?.invoke;

export function hasTauri() {
  return Boolean(invoke);
}

/**
 * @param {string} targetModule
 * @param {string} method
 * @param {Record<string, unknown>} payload
 * @returns {Promise<unknown>}
 */
export async function dispatchToModule(targetModule, method, payload) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return invoke("dispatch_to_module", {
    targetModule,
    method,
    payload,
  });
}

/**
 * @param {Record<string, unknown>} payload
 * @returns {Promise<unknown>}
 */
export async function executeDicoContribution(payload) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return invoke("execute_dico_contribution", { payload });
}

/**
 * @param {Record<string, unknown>} params
 * @returns {Promise<Record<string, number>>}
 */
export async function calculateInvoicePreview(params) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<Record<string, number>>} */ (invoke("calculate_invoice_preview", params));
}

/**
 * @param {string} eventName
 * @param {(payload: unknown) => void} handler
 * @returns {Promise<(() => void) | null>}
 */
export function onTauriEvent(eventName, handler) {
  const listen = window.__TAURI__?.event?.listen;
  if (!listen) return Promise.resolve(null);
  return listen(eventName, (event) => handler(event.payload));
}

/** @returns {Promise<SidecarRuntimeStats>} */
export async function getSidecarStats() {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<SidecarRuntimeStats>} */ (invoke("get_sidecar_stats"));
}

/**
 * @param {number} agentId
 * @returns {Promise<string>}
 */
export async function resolveDicobaMemberId(agentId) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<string>} */ (invoke("resolve_dicoba_member_id_for_agent", { agentId }));
}

/**
 * @param {Object} params
 * @param {string} params.customerPubkey
 * @param {number} params.amountShannons
 * @param {number} params.fiatReceived
 * @returns {Promise<Record<string, unknown>>}
 */
export async function executeCashInTransaction({
  customerPubkey,
  amountShannons,
  fiatReceived,
}) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<Record<string, unknown>>} */ (
    invoke("execute_cash_in_transaction", {
      customerPubkey,
      amountShannons,
      fiatReceived,
    })
  );
}

/**
 * @param {Object} params
 * @param {number} params.currentFiat
 * @param {number} params.digitalL2BalanceShannons
 * @returns {Promise<string>}
 */
export async function triggerManualFiatRebalance({
  currentFiat,
  digitalL2BalanceShannons,
}) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<string>} */ (
    invoke("trigger_manual_fiat_rebalance", {
      currentFiat,
      digitalL2BalanceShannons,
    })
  );
}

/**
 * @param {Object} params
 * @param {string} params.targetModule
 * @param {number} params.targetAgent
 * @param {string} params.method
 * @param {Record<string, unknown>} params.payload
 * @returns {Promise<{ uri: string, qrSvg?: string, qr_svg?: string }>}
 */
export async function generateOobFallbackUri({
  targetModule,
  targetAgent,
  method,
  payload,
}) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<{ uri: string, qrSvg?: string, qr_svg?: string }>} */ (
    invoke("generate_oob_fallback_uri", {
      targetModule,
      targetAgent,
      method,
      payload,
    })
  );
}

/**
 * @param {string} uriString
 * @returns {Promise<string>}
 */
export async function processOobFallback(uriString) {
  if (!invoke) {
    throw new Error("Tauri runtime unavailable");
  }
  return /** @type {Promise<string>} */ (invoke("process_oob_fallback", { uriString }));
}

/**
 * @returns {Promise<{
 *   address: string,
 *   pubkey: string,
 *   network: string,
 *   fnnRpcUrl: string,
 *   fundingLockScript: Record<string, unknown>,
 *   source: string,
 * }>}
 */
export async function getFnnAddress() {
  if (typeof invoke !== "function") {
    throw new Error("Tauri runtime unavailable");
  }
  /** @type {{
   *   address: string,
   *   pubkey: string,
   *   network: string,
   *   fnnRpcUrl: string,
   *   fundingLockScript: Record<string, unknown>,
   *   source: string,
   * }} */
  const snapshot = /** @type {any} */ (await invoke("get_fnn_address"));
  return snapshot;
}

/**
 * Open a URL in the OS default browser (Tauri shell plugin).
 * @param {string} url
 */
export async function openExternalUrl(url) {
  const shellOpen = window.__TAURI__?.shell?.open;
  if (typeof shellOpen === "function") {
    await shellOpen(url);
    return;
  }
  // Tauri 2 plugin invoke fallback
  if (invoke) {
    try {
      await invoke("plugin:shell|open", { path: url });
      return;
    } catch {
      // fall through
    }
  }
  const opened = window.open(url, "_blank", "noopener,noreferrer");
  if (!opened) {
    throw new Error("Unable to open external browser");
  }
}
