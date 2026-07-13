import { escapeHtml } from "./dom-security.js";
import {
  DEFAULT_CONVERSION_RATE,
  formatCount,
  formatFiatFromShannons,
  formatShannons as formatShannonsMoney,
} from "../dashboard/money.js";

export { DEFAULT_CONVERSION_RATE, formatCount, formatFiatFromShannons };

/** @param {number | bigint | string | null | undefined} value @returns {string} */
export function formatShannons(value) {
  return formatShannonsMoney(value, { suffix: false });
}

/** @param {string | null | undefined} pubkey @returns {string} */
export function shortPubkey(pubkey) {
  if (!pubkey || pubkey === "unavailable") return "—";
  if (pubkey.length <= 16) return pubkey;
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-6)}`;
}

/** @param {string | null | undefined} memberId @returns {string} */
export function shortMemberId(memberId) {
  if (!memberId) return "—";
  if (memberId.length <= 16) return memberId;
  return `${memberId.slice(0, 8)}…${memberId.slice(-8)}`;
}

/** @param {Date} date @returns {string} */
export function formatDateTime(date) {
  return date.toLocaleString("en-GB", {
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
    hour12: true,
    timeZoneName: "short",
  });
}

/** @param {Date} [date] @returns {string} */
export function formatLiveClock(date = new Date()) {
  return formatDateTime(date);
}

/** @param {number | string | null | undefined} unix @returns {string} */
export function formatLastSync(unix) {
  if (!unix) return "—";
  return formatDateTime(new Date(Number(unix) * 1000));
}

/**
 * @param {string} label
 * @param {string} value
 * @param {string} [hint]
 * @param {{ trend?: boolean }} [options]
 * @returns {string}
 */
export function metricCell(label, value, hint = "", { trend = false } = {}) {
  return `
    <article class="metric-cell">
      <span class="metric-label">${escapeHtml(label)}</span>
      <strong class="metric-value">${escapeHtml(value)}</strong>
      ${hint ? `<span class="metric-hint${trend ? " metric-hint-trend" : ""}">${escapeHtml(hint)}</span>` : ""}
    </article>
  `;
}

/**
 * @param {string} label
 * @param {string} value
 * @param {string} [hint]
 * @param {{ status?: string }} [options]
 * @returns {string}
 */
export function kpiCard(label, value, hint = "", { status = "neutral" } = {}) {
  return `
    <article class="kpi-card kpi-card--${status}">
      <span class="kpi-label">${escapeHtml(label)}</span>
      <strong class="kpi-value">${escapeHtml(value)}</strong>
      ${hint ? `<span class="kpi-hint">${escapeHtml(hint)}</span>` : ""}
    </article>
  `;
}

/**
 * @param {string} title
 * @param {string} subtitle
 * @param {string} cellsHtml
 * @returns {string}
 */
export function dashboardMetricSection(title, subtitle, cellsHtml) {
  return `
    <section class="metric-section">
      <header class="metric-section-head">
        <h3>${escapeHtml(title)}</h3>
        ${subtitle ? `<p>${escapeHtml(subtitle)}</p>` : ""}
      </header>
      <div class="metric-grid metric-grid--cards">
        ${cellsHtml}
      </div>
    </section>
  `;
}

/**
 * @param {string} title
 * @param {string} cellsHtml
 * @param {{ hint?: string, actionHtml?: string }} [options]
 * @returns {string}
 */
export function metricSection(title, cellsHtml, { hint = "", actionHtml = "" } = {}) {
  return `
    <div class="module-stats-block">
      <div class="module-stats-head">
        <div>
          <h3>${escapeHtml(title)}</h3>
          ${hint ? `<span class="aside-hint">${escapeHtml(hint)}</span>` : ""}
        </div>
        ${actionHtml}
      </div>
      <div class="metric-grid metric-grid--aside">
        ${cellsHtml}
      </div>
    </div>
  `;
}
