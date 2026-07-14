import {
  DEFAULT_CONVERSION_RATE,
  fiatMinorToShannons,
  parseAtomicInt,
  parseFiatMinorUnits,
  shannonsToFiatMinor,
} from "../dashboard/money.js";
import { createLogger } from "../dashboard/logger.js";
import { errorMessage } from "../packages/fsp-ui-types/errors.js";
import { getSidecarStats } from "./sidecar-api.js";

/** @typedef {import("../../../../dashboard/types.js").SidecarRuntimeStats} SidecarRuntimeStats */

const log = createLogger("sidecar-runtime");

/** @type {SidecarRuntimeStats | null} */
let cache = null;

export const SIDECAR_RUNTIME_EVENT = "sidecar-runtime-updated";

/** @returns {SidecarRuntimeStats | null} */
export function getSidecarRuntime() {
  return cache;
}

/** @param {SidecarRuntimeStats | null | undefined} [runtime] @returns {number} */
export function conversionRate(runtime = cache) {
  return runtime?.fiatConversionRate ?? DEFAULT_CONVERSION_RATE;
}

/** @param {number | bigint | string} shannons @param {number} [rate] @returns {number} */
export function shannonsToFiat(shannons, rate = conversionRate()) {
  return Number(shannonsToFiatMinor(shannons, rate));
}

/** @param {number | string} fiat @param {number} [rate] @returns {number} */
export function fiatToShannons(fiat, rate = conversionRate()) {
  const fiatMinor = parseFiatMinorUnits(fiat, "fiat");
  return Number(fiatMinorToShannons(fiatMinor, rate));
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {number} */
export function defaultFloatReserveFiat(runtime) {
  const rate = conversionRate(runtime);
  const shannons = runtime?.totalLocalBalanceShannons ?? 0;
  const derived = Number(shannonsToFiatMinor(shannons, rate));
  return derived > 0 ? derived : runtime?.criticalFiatFloor ?? 50_000;
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {number} */
export function defaultCashInShannons(runtime) {
  const local = runtime?.totalLocalBalanceShannons ?? 0;
  if (local === 0) return 1_000_000;
  return Math.min(1_000_000, Math.max(100_000, Math.floor(local / 100)));
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
export function defaultCustomerPubkey(runtime) {
  const peer = runtime?.meshPeerPubkey?.trim();
  if (peer && peer !== "unavailable") return peer;
  return "";
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
export function formatAgentLabel(runtime) {
  if (!runtime?.agentId) return "FA—";
  return `FA-${runtime.agentId}`;
}

/** @param {SidecarRuntimeStats | null | undefined} runtime @returns {string} */
export function formatBrandSubtitle(runtime) {
  if (!runtime?.agentId) return "Loading…";
  const descriptor = (runtime.hardwareProfile ?? runtime.sidecarProfile ?? "agent")
    .replaceAll("_", " ");
  return `${formatAgentLabel(runtime)} · ${descriptor}`;
}

/** @param {unknown} error @returns {string} */
export function formatBrandSubtitleError(error) {
  const message = errorMessage(error);
  if (/tauri runtime unavailable/i.test(message)) {
    return "Open the desktop app";
  }
  return "Status unavailable";
}

/**
 * @param {{ force?: boolean, snapshot?: SidecarRuntimeStats | null }} [options]
 * @returns {Promise<SidecarRuntimeStats | null>}
 */
export async function loadSidecarRuntime({ force = false, snapshot = null } = {}) {
  if (snapshot) {
    cache = snapshot;
  } else if (!force && cache) {
    // reuse cached runtime
  } else {
    try {
      cache = await getSidecarStats();
    } catch (error) {
      log.warn("stats unavailable", error);
      cache = null;
    }
  }

  if (cache) {
    window.dispatchEvent(
      new CustomEvent(SIDECAR_RUNTIME_EVENT, { detail: cache }),
    );
  }
  return cache;
}

/**
 * @param {HTMLInputElement} shannonsInput
 * @param {HTMLInputElement} fiatInput
 * @param {number} [rate]
 */
export function bindShannonsFiatSync(shannonsInput, fiatInput, rate = conversionRate()) {
  let syncing = false;

  const syncFromShannons = () => {
    if (syncing) return;
    syncing = true;
    try {
      const shannons = parseAtomicInt(shannonsInput.value, "shannons");
      fiatInput.value = String(shannonsToFiatMinor(shannons, rate));
    } catch {
      fiatInput.value = "0";
    }
    syncing = false;
  };

  const syncFromFiat = () => {
    if (syncing) return;
    syncing = true;
    try {
      const fiatMinor = parseFiatMinorUnits(fiatInput.value, "fiat");
      shannonsInput.value = String(fiatMinorToShannons(fiatMinor, rate));
    } catch {
      shannonsInput.value = "0";
    }
    syncing = false;
  };

  shannonsInput.addEventListener("input", syncFromShannons);
  fiatInput.addEventListener("input", syncFromFiat);
}

/**
 * @param {HTMLElement | null} panel
 * @param {SidecarRuntimeStats | null | undefined} runtime
 */
export function applyRuntimeToFloatPanel(panel, runtime) {
  if (!panel || !runtime) return;

  const reserveInput = panel.querySelector("[data-float-reserve]");
  const pubkeyInput = panel.querySelector("[data-cash-in-pubkey]");
  const shannonsInput = panel.querySelector("[data-cash-in-shannons]");
  const fiatInput = panel.querySelector("[data-cash-in-fiat]");
  const floorHint = panel.querySelector("[data-critical-floor-hint]");
  const peerHint = panel.querySelector("[data-mesh-peer-hint]");
  const rate = conversionRate(runtime);

  if (reserveInput instanceof HTMLInputElement) {
    reserveInput.value = String(defaultFloatReserveFiat(runtime));
  }
  if (pubkeyInput instanceof HTMLInputElement && !pubkeyInput.dataset.userEdited) {
    pubkeyInput.value = defaultCustomerPubkey(runtime);
  }
  if (shannonsInput instanceof HTMLInputElement && !shannonsInput.dataset.userEdited) {
    shannonsInput.value = String(defaultCashInShannons(runtime));
  }
  if (fiatInput instanceof HTMLInputElement && !fiatInput.dataset.userEdited) {
    fiatInput.value = String(
      shannonsToFiatMinor(defaultCashInShannons(runtime), rate),
    );
  }
  if (floorHint) {
    const floor = Math.round(runtime.criticalFiatFloor ?? 50_000).toLocaleString();
    const hubOk =
      runtime.mfaControlConnected === true ||
      String(runtime.mfaConnectionStatus ?? "").toLowerCase().includes("connect");
    const hubLabel = hubOk ? "Hub online" : "Hub offline";
    floorHint.textContent = `Alert if cash falls below ${floor} TZS · ${hubLabel}`;
  }
  if (peerHint) {
    const peerId = runtime.meshPeerAgentId;
    peerHint.textContent = peerId
      ? `Customer ID filled from partner FA-${peerId}`
      : "Enter a customer network ID if it is not filled in automatically";
  }
}

/**
 * @param {HTMLElement | null} panel
 * @param {SidecarRuntimeStats | null | undefined} runtime
 */
export function applyRuntimeToFeePanel(panel, runtime) {
  if (!panel || !runtime) return;

  const volumeInput = panel.querySelector("[data-fiat-volume]");
  if (volumeInput instanceof HTMLInputElement && !volumeInput.dataset.userEdited) {
    volumeInput.value = String(defaultFloatReserveFiat(runtime));
  }
}

/** @param {HTMLInputElement | null | undefined} input */
export function markUserEdited(input) {
  if (!input) return;
  input.addEventListener("input", () => {
    input.dataset.userEdited = "true";
  });
}
