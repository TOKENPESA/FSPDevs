/**
 * Sidecar operations console for the web dashboard bundle.
 * Tauri desktop uses fiber-agent/src-tauri/frontend/sidecar-console.js (keep in sync).
 */

import { $, $input, requireInput, setText } from "./dom.js";
import { errorMessage } from "./packages/fsp-ui-types/errors.js";

function tauriCore() {
  return window.__TAURI__?.core ?? null;
}

function tauriEvent() {
  return window.__TAURI__?.event ?? null;
}

/** @param {string} command @param {Record<string, unknown>} [args] */
async function invoke(command, args = {}) {
  const core = tauriCore();
  if (!core?.invoke) return null;
  return core.invoke(command, args);
}

/** @param {string} event @param {(event: { payload: unknown }) => void} handler */
async function listen(event, handler) {
  const events = tauriEvent();
  if (!events?.listen) return () => {};
  return events.listen(event, handler);
}

/** @param {string} message @param {string} [styleClass] */
function appendLogLine(message, styleClass = "") {
  const streamContainer = document.getElementById("console-stream");
  if (!streamContainer) return;
  const timestamp = new Date().toLocaleTimeString();
  const line = document.createElement("div");
  line.className = `log-line ${styleClass}`;
  line.textContent = `[${timestamp}] ${message}`;
  streamContainer.appendChild(line);
  streamContainer.scrollTop = streamContainer.scrollHeight;
}

/** @param {string} activeProfile */
function applyPowerProfileUi(activeProfile) {
  const btnRealtime = document.getElementById("btn-realtime");
  const btnBattsave = document.getElementById("btn-battsave");
  const hintText = document.getElementById("power-hint");
  if (!btnRealtime || !btnBattsave || !hintText) return;

  if (activeProfile === "AggressiveRealTime") {
    btnRealtime.classList.add("active");
    btnBattsave.classList.remove("active");
    hintText.textContent =
      "High-frequency tracking active. Zero latency. Increased power draw.";
    hintText.style.color = "var(--warn)";
  } else {
    btnBattsave.classList.add("active");
    btnRealtime.classList.remove("active");
    hintText.textContent =
      "Adaptive check backoffs active. Preserving battery parameters.";
    hintText.style.color = "var(--muted)";
  }
}

/** @param {string} targetProfile */
async function updatePowerProfile(targetProfile) {
  try {
    const result = await invoke("toggle_hardware_profile", {
      newProfile: targetProfile,
    });
    if (typeof result === "string") applyPowerProfileUi(result);
  } catch (err) {
    appendLogLine(`❌ Power system mutation failed: ${errorMessage(err)}`, "danger");
    applyPowerProfileUi(targetProfile);
  }
}

async function recalculateFees() {
  const flatInput = $input("input-flat");
  const ppmInput = $input("input-ppm");
  const levyInput = $input("input-levy");
  const withdrawalInput = $input("input-withdrawal");
  if (!flatInput || !ppmInput || !levyInput || !withdrawalInput) return;

  const flatVal = parseFloat(flatInput.value);
  const ppmVal = parseInt(ppmInput.value, 10);
  const levyVal = parseFloat(levyInput.value) / 100.0;
  const sampleWithdrawal = parseFloat(withdrawalInput.value);

  setText($("val-withdrawal"), `${sampleWithdrawal.toLocaleString()} TZS`);
  setText($("val-flat"), `${flatVal} TZS`);
  setText($("val-ppm"), `${ppmVal.toLocaleString()} PPM`);
  setText($("val-levy"), `${(levyVal * 100).toFixed(2)} %`);

  try {
    const breakdown = await invoke("calculate_invoice_preview", {
      targetFiat: sampleWithdrawal,
      flatCommission: flatVal,
      proportionalPpm: ppmVal,
      sovereignLevy: levyVal,
    });

    if (breakdown && typeof breakdown === "object") {
      const preview = /** @type {Record<string, number>} */ (breakdown);
      setText($("fee-l1"), `${preview.layer1_l2_routing_fee_fiat.toFixed(1)} TZS`);
      setText($("fee-l2"), `${preview.layer2_kiosk_commission_fiat.toFixed(1)} TZS`);
      setText($("fee-l3"), `${preview.layer3_sovereign_levy_fiat.toFixed(1)} TZS`);
      setText($("fee-total"), `${preview.absolute_total_fiat_cost.toFixed(1)} TZS`);
      return;
    }
  } catch {
    /* mock fallback */
  }

  const l1Mock = 1.5;
  const l2Mock = flatVal + sampleWithdrawal * (ppmVal / 1_000_000.0);
  const l3Mock = sampleWithdrawal * levyVal;
  setText($("fee-l1"), `${l1Mock} TZS`);
  setText($("fee-l2"), `${l2Mock} TZS`);
  setText($("fee-l3"), `${l3Mock} TZS`);
  setText($("fee-total"), `${(sampleWithdrawal + l1Mock + l2Mock + l3Mock).toFixed(1)} TZS`);
}

function syncDicobaContribution() {
  const amountFiat = parseFloat($input("input-dicoba-amount")?.value ?? "0");
  const rate = parseFloat($input("input-dicoba-rate")?.value ?? "38");
  const shannons = Math.max(0, Math.floor(amountFiat * rate));

  setText($("val-dicoba-amount"), `${amountFiat.toLocaleString()} TZS`);
  setText($("val-dicoba-rate"), `${rate}`);
  setText($("val-dicoba-shannons"), `${shannons.toLocaleString()} shannons`);
}

function buildVaultConfigFromForm() {
  const groupName =
    $input("input-dicoba-group")?.value?.trim() || "Mabibo Dicoba";
  const leaderPubkey =
    $input("input-dicoba-leader")?.value?.trim() || "03leader_a";
  const vaultId = globalThis.crypto?.randomUUID?.() ?? "00000000-0000-4000-8000-000000000001";

  const now = Math.floor(Date.now() / 1000);
  return {
    vault_id: vaultId,
    group_name: groupName,
    cycle_start_timestamp: now,
    cycle_end_timestamp: now + 2_592_000,
    cycle_state: "Active",
    base_asset_iso: "TZS",
    share_price_shannons: 100_000,
    social_fund_flat_fee_shannons: 25_000,
    base_interest_rate_bps: 500,
    peak_interest_rate_bps: 2_500,
    pool_shares_shannons: 0,
    pool_social_fund_shannons: 0,
    pool_fines_and_interest_shannons: 0,
    governance_lock: {
      total_signers: 3,
      required_signatures: 2,
      leader_pubkeys: [leaderPubkey],
    },
    members: [],
    l1_cell_outpoint: "0xdicoba:0",
  };
}

async function submitChamaContribution() {
  const amountFiat = parseFloat($input("input-dicoba-amount")?.value ?? "0");
  const shannonsConversionRate = parseFloat(
    $input("input-dicoba-rate")?.value ?? "38",
  );
  const vaultConfig = buildVaultConfigFromForm();
  const atomicShannons = Math.max(0, Math.floor(amountFiat * shannonsConversionRate));

  appendLogLine(
    `💧 [DICOBA] Streaming ${atomicShannons.toLocaleString()} shannons to ${vaultConfig.group_name}…`,
    "info",
  );

  try {
    const receipt = await invoke("execute_dico_contribution", {
      payload: {
        vaultConfig,
        amountFiat,
        shannonsConversionRate,
      },
    });

    if (receipt && typeof receipt === "object") {
      const record = /** @type {Record<string, unknown>} */ (receipt);
      appendLogLine(
        `✅ [DICOBA] Contribution recorded — tx ${record.transaction_id}, vault ${record.vault_id}, ${Number(record.amount_shannons).toLocaleString()} shannons`,
        "success",
      );
      return;
    }
  } catch (err) {
    appendLogLine(`❌ [DICOBA] ${errorMessage(err)}`, "danger");
    if (tauriCore()) return;
  }

  appendLogLine(
    `✓ [MOCK DICOBA] Local ledger would credit ${atomicShannons.toLocaleString()} shannons to ${vaultConfig.group_name}`,
    "success",
  );
}

function syncFloatReserves() {
  const fiatInput = $input("input-fiat");
  if (!fiatInput) return;
  const currentFiat = parseFloat(fiatInput.value);
  setText($("val-fiat"), `${currentFiat.toLocaleString()} TZS`);

  const fillLocal = document.getElementById("fill-local");
  const fillRemote = document.getElementById("fill-remote");
  if (!fillLocal || !fillRemote) return;
  const percentageBase = (currentFiat / 1_000_000.0) * 100;
  fillLocal.style.width = `${Math.min(Math.max(percentageBase, 15), 85)}%`;
  fillRemote.style.width = `${100 - parseFloat(fillLocal.style.width)}%`;
}

async function triggerManualRebalanceTest() {
  const fiatInput = requireInput("input-fiat");
  const currentFiat = parseFloat(fiatInput.value);
  appendLogLine(
    `⚙️ [SIMULATOR] Dispatched manual cash-out event simulation. Active Fiat: TZS ${currentFiat}`,
    "info",
  );

  try {
    const responseMessage = await invoke("trigger_manual_fiat_rebalance", {
      currentFiat,
    });
    if (typeof responseMessage === "string") {
      if (responseMessage.includes("breached")) {
        appendLogLine(`⚠️ [TELEMETRY BREACH] ${responseMessage}`, "warn");
        if (responseMessage.includes("CLEARING_COMPLETE")) {
          appendLogLine(
            "💰 [REGIONAL CLEARING] MFA float-crisis intake accepted via /clearing/float-crisis.",
            "success",
          );
        } else if (responseMessage.includes("dispatched")) {
          appendLogLine(
            "📡 [CLEARING DISPATCH] Float-crisis telemetry posted to MFA /clearing/float-crisis.",
            "info",
          );
        }
      } else {
        appendLogLine(`✓ [TELEMETRY SAFE] ${responseMessage}`, "success");
      }
    }
  } catch (err) {
    appendLogLine(`❌ [CLEARING DISPATCH] ${errorMessage(err)}`, "danger");
    if (currentFiat <= 200_000) {
      appendLogLine(
        `⚠️ [MOCK TELEMETRY BREACH] Local reserves hit TZS ${currentFiat}. Triggering clearinghouse rebalance routing circuits.`,
        "warn",
      );
      appendLogLine(
        "✅ [MOCK API DISPATCH] Vodacom M-Pesa network cleared B2C payout request successfully. Capital refilled.",
        "success",
      );
    } else {
      appendLogLine(
        "✓ [MOCK TELEMETRY] Float values remain inside target baseline threshold boundaries.",
        "success",
      );
    }
  }
}

function bindControls() {
  document.getElementById("btn-realtime")?.addEventListener("click", () => {
    void updatePowerProfile("AggressiveRealTime");
  });
  document.getElementById("btn-battsave")?.addEventListener("click", () => {
    void updatePowerProfile("BatterySaver");
  });
  document.getElementById("btn-rebalance")?.addEventListener("click", () => {
    void triggerManualRebalanceTest();
  });
  document.getElementById("input-withdrawal")?.addEventListener("input", () => {
    void recalculateFees();
  });
  document.getElementById("input-flat")?.addEventListener("input", () => {
    void recalculateFees();
  });
  document.getElementById("input-ppm")?.addEventListener("input", () => {
    void recalculateFees();
  });
  document.getElementById("input-levy")?.addEventListener("input", () => {
    void recalculateFees();
  });
  document.getElementById("input-fiat")?.addEventListener("input", () => {
    syncFloatReserves();
  });
  document.getElementById("input-dicoba-amount")?.addEventListener("input", () => {
    syncDicobaContribution();
  });
  document.getElementById("input-dicoba-rate")?.addEventListener("input", () => {
    syncDicobaContribution();
  });
  document.getElementById("btn-dicoba-contribute")?.addEventListener("click", () => {
    void submitChamaContribution();
  });

  window.updatePowerProfile = updatePowerProfile;
  window.recalculateFees = recalculateFees;
  window.syncFloatReserves = syncFloatReserves;
  window.syncDicobaContribution = syncDicobaContribution;
  window.submitChamaContribution = submitChamaContribution;
  window.triggerManualRebalanceTest = triggerManualRebalanceTest;
}

export async function initSidecarConsole() {
  if (!document.getElementById("console-stream")) return;

  bindControls();
  if (tauriCore()) {
    await updatePowerProfile("BatterySaver");
  } else {
    applyPowerProfileUi("BatterySaver");
  }
  void recalculateFees();
  syncFloatReserves();
  syncDicobaContribution();

  if (tauriCore()) {
    await listen("profile-changed-callback", (event) => {
      if (typeof event.payload === "string") applyPowerProfileUi(event.payload);
    });
    await listen("float-crisis", (event) => {
      appendLogLine(`⚠️ [FLOAT CRISIS] ${String(event.payload)}`, "warn");
    });
    await listen("clearing-dispatch", (event) => {
      const payload = event.payload && typeof event.payload === "object"
        ? /** @type {Record<string, unknown>} */ (event.payload)
        : {};
      const status = payload.status ?? "UNKNOWN";
      if (status === "CLEARING_COMPLETE") {
        appendLogLine(
          "✅ [REGIONAL CLEARING] Float-crisis rebalance complete · EnterpriseClearinghouse handles balance-depletion refuel separately.",
          "success",
        );
      } else if (status === "CLEARING_ABORTED") {
        appendLogLine(
          `⚠️ [REGIONAL CLEARING] Aborted — ${payload.reason ?? "telco gateway timeout"}`,
          "warn",
        );
      } else {
        appendLogLine(`📡 [REGIONAL CLEARING] Response: ${JSON.stringify(event.payload)}`, "info");
      }
    });
  }
}

export { appendLogLine };
