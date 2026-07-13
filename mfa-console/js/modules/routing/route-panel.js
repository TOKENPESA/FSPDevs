import { postRoute } from "../../mfa-api.js";
import { escapeHtml } from "../../dom-security.js";
import { errorMessage } from "../../../../packages/fsp-ui-types/errors.js";
import { state, logEvent, markDirty, touchCommEdge, touchCommNode } from "../../../../dashboard/state.js";
import {
  settlePaymentTransfer,
  startPaymentTransfer,
} from "../../../../dashboard/events/payment.js";
import { formatShannons } from "../../../../dashboard/format.js";
import { metricSection, metricCell } from "../../stats-ui.js";

/**
 * @param {string} id
 * @returns {HTMLInputElement | null}
 */
function inputById(id) {
  const el = document.getElementById(id);
  return el instanceof HTMLInputElement ? el : null;
}

/**
 * @param {ParentNode} root
 * @param {string} selector
 * @returns {HTMLInputElement | null}
 */
function queryInput(root, selector) {
  const el = root.querySelector(selector);
  return el instanceof HTMLInputElement ? el : null;
}

export const routePanel = {
  id: "mfa-routing",
  title: "Routing & Payments",
  navLabel: "Route & Pay",
  navIcon: "routing",
  badge: "payments",
  navDescription: "Compute hop paths across the mesh and execute L2 payments via MFA.",
  render() {
    return `
      <div class="workspace-card">
        <div class="workspace-card-head">
          <h2>Transaction simulator</h2>
          <p class="panel-hint">Routes respect active simulation size (FA 1…${state.networkSize})</p>
        </div>
        <form class="form-grid-2" data-route-form>
          <div class="workspace-field">
            <label for="route-source-ui">Source FA</label>
            <input id="route-source-ui" type="number" min="1" max="${state.networkSize}" value="1">
          </div>
          <div class="workspace-field">
            <label for="route-dest-ui">Destination FA</label>
            <input id="route-dest-ui" type="number" min="1" max="${state.networkSize}" value="${Math.min(state.networkSize, 512)}">
          </div>
          <div class="workspace-field">
            <label for="route-amount-ui">Amount (shannons)</label>
            <input id="route-amount-ui" type="number" min="1" value="1000000">
          </div>
          <div class="workspace-field" style="align-self:end">
            <button type="submit" class="panel-btn panel-btn-primary">Route &amp; Pay</button>
          </div>
        </form>
        <div class="workspace-card" style="margin-top:0.85rem">
          <p class="panel-hint" data-route-result>Awaiting route request…</p>
        </div>
      </div>`;
  },
  renderAside() {
    return metricSection(
      "Active route",
      metricCell("Path", state.activeRoute.length ? state.activeRoute.map((id) => `FA-${id}`).join(" → ") : "—", "Last computed hop chain"),
      { hint: "Requires live MFA on :1025" },
    );
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    const form = root.querySelector("[data-route-form]");
    const result = root.querySelector("[data-route-result]");
    const sourceInput = inputById("route-source");
    const destInput = inputById("route-dest");
    const amountInput = inputById("route-amount");

    form?.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      if (!result) return;

      const source = Number.parseInt(queryInput(root, "#route-source-ui")?.value ?? "1", 10);
      const destination = Number.parseInt(queryInput(root, "#route-dest-ui")?.value ?? "2", 10);
      const amount = Number.parseInt(queryInput(root, "#route-amount-ui")?.value ?? "0", 10);

      if (!Number.isInteger(source) || source < 1 || source > state.networkSize) {
        result.textContent = `Source must be 1…${state.networkSize}`;
        return;
      }
      if (!Number.isInteger(destination) || destination < 1 || destination > state.networkSize) {
        result.textContent = `Destination must be 1…${state.networkSize}`;
        return;
      }
      if (!Number.isFinite(amount) || amount < 1) {
        result.textContent = "Amount must be a positive integer";
        return;
      }

      if (sourceInput) sourceInput.value = String(source);
      if (destInput) destInput.value = String(destination);
      if (amountInput) amountInput.value = String(amount);

      result.textContent = `Routing FA-${source} → FA-${destination}…`;
      logEvent(`Routing FA-${source} → FA-${destination} (${amount} shannons)…`);

      try {
        const data = await postRoute({
          source,
          destination,
          amountShannons: amount,
          activeNetworkLimit: state.networkSize,
          execute: true,
        });

        if (data.status === "ROUTE_FOUND" && Array.isArray(data.path) && data.path.length >= 2) {
          startPaymentTransfer(data.path, source, destination, amount);
          for (let i = 0; i < data.path.length - 1; i++) {
            touchCommEdge(data.path[i], data.path[i + 1], "mesh");
          }
          touchCommNode(data.path[0], [data.path[1]], 1);
          const pathLabel = data.path.map((/** @type {number} */ id) => `FA-${id}`).join(" → ");
          result.innerHTML = `Route found (${escapeHtml(String(data.execution_latency_ms))}ms): <strong>${escapeHtml(pathLabel)}</strong>`;
          logEvent(`ROUTE_FOUND: ${pathLabel}`, "heal");

          if (data.payment_status === "SUCCESS") {
            settlePaymentTransfer(true, data.payment_fee_shannons ?? 0);
            logEvent(
              `PAYMENT OK · ${formatShannons(amount)} · fee ${formatShannons(data.payment_fee_shannons ?? 0)}`,
              "heal",
            );
          } else if (data.payment_status === "FAILED" || data.payment_status === "TIMEOUT") {
            settlePaymentTransfer(false);
            logEvent(`Payment ${data.payment_status}`, "warn");
          }
          markDirty();
        } else {
          settlePaymentTransfer(false);
          result.textContent = `Route failed: ${data.status ?? "unknown"}`;
          logEvent(`Route failed: ${data.status}`, "warn");
        }
      } catch (err) {
        result.textContent = `Request failed: ${errorMessage(err)}`;
        logEvent(`Route request failed: ${errorMessage(err)}`, "warn");
      }
    });
  },
};
