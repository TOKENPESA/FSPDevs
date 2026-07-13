/**
 * @param {unknown} value
 */
export function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/**
 * @param {unknown} error
 * @param {string} [fallback]
 */
export function safeUserMessage(error, fallback = "Operation failed") {
  const raw = String(
    error instanceof Error ? error.message : error ?? "",
  ).trim();
  if (!raw) return fallback;
  if (/sqlite|rusqlite|database|storage operation failed/i.test(raw)) {
    return "A storage error occurred. Check service logs.";
  }
  if (/timed out|abort/i.test(raw)) {
    return "Request timed out. Try again.";
  }
  return raw.length > 200 ? `${raw.slice(0, 200)}…` : raw;
}
