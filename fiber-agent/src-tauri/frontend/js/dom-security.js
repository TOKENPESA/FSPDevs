/** @param {unknown} value @returns {string} */
export function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Strip internal/SQLite details before showing errors in the UI. */
/** @param {unknown} error @param {string} [fallback] @returns {string} */
export function safeUserMessage(error, fallback = "Operation failed") {
  const message =
    error instanceof Error ? error.message : typeof error === "string" ? error : String(error ?? "");
  const raw = message.trim();
  if (!raw) return fallback;
  if (/sqlite|rusqlite|database|storage operation failed/i.test(raw)) {
    return "A storage error occurred. Check sidecar logs.";
  }
  if (/timed out|abort/i.test(raw)) {
    return "Request timed out. Try again.";
  }
  return raw.length > 200 ? `${raw.slice(0, 200)}…` : raw;
}

/** @param {HTMLElement | null | undefined} el @param {string} html */
export function setElementHtml(el, html) {
  if (!el) return;
  el.innerHTML = html;
}

/** @param {HTMLElement | null | undefined} el @param {string} message @param {{ html?: boolean }} [options] */
export function setLogMessage(el, message, { html = false } = {}) {
  if (!el) return;
  el.style.display = "block";
  if (html) {
    el.innerHTML = message;
    return;
  }
  el.textContent = message;
}
