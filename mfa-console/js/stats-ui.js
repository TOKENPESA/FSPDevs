import { escapeHtml } from "./dom-security.js";
import { formatCount, formatShannons as formatShannonsMoney } from "../../dashboard/money.js";

/**
 * @param {number | bigint | string | null | undefined} value
 */
export function formatShannons(value) {
  return formatShannonsMoney(value, { suffix: false });
}

export { formatCount };

/**
 * @param {Date} date
 */
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

/** @param {Date} [date] */
export function formatLiveClock(date = new Date()) {
  return formatDateTime(date);
}

/**
 * @param {number | string | null | undefined} unix
 */
export function formatLastSync(unix) {
  if (!unix) return "—";
  return formatDateTime(new Date(Number(unix) * 1000));
}

/**
 * @param {string} label
 * @param {string} value
 * @param {string} [hint]
 * @param {{ trend?: boolean }} [options]
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
 * @param {string} title
 * @param {string} cellsHtml
 * @param {{ hint?: string }} [options]
 */
export function metricSection(title, cellsHtml, { hint = "" } = {}) {
  return `
    <div class="module-stats-block">
      <div class="module-stats-head">
        <div>
          <h3>${escapeHtml(title)}</h3>
          ${hint ? `<span class="aside-hint">${escapeHtml(hint)}</span>` : ""}
        </div>
      </div>
      <div class="metric-grid metric-grid--aside">
        ${cellsHtml}
      </div>
    </div>
  `;
}
