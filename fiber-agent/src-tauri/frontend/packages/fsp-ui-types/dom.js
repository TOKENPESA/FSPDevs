/**
 * Typed DOM query helpers for strict checkJs projects.
 * Duck-type fallbacks allow Node import-smoke tests without full DOM classes.
 */

/** @param {unknown} el @returns {el is HTMLCanvasElement} */
function isCanvasElement(el) {
  if (!el || typeof el !== "object") return false;
  if (typeof HTMLCanvasElement !== "undefined" && el instanceof HTMLCanvasElement) return true;
  return "getContext" in el && typeof /** @type {{ getContext?: unknown }} */ (el).getContext === "function";
}

/** @param {unknown} el @returns {el is HTMLInputElement} */
function isInputElement(el) {
  if (!el || typeof el !== "object") return false;
  if (typeof HTMLInputElement !== "undefined" && el instanceof HTMLInputElement) return true;
  return "value" in el && !("getContext" in el);
}

/** @param {unknown} el @returns {el is HTMLSelectElement} */
function isSelectElement(el) {
  if (!el || typeof el !== "object") return false;
  if (typeof HTMLSelectElement !== "undefined" && el instanceof HTMLSelectElement) return true;
  return "options" in el && "value" in el;
}

/** @param {unknown} el @returns {el is HTMLButtonElement} */
function isButtonElement(el) {
  if (!el || typeof el !== "object") return false;
  if (typeof HTMLButtonElement !== "undefined" && el instanceof HTMLButtonElement) return true;
  return "disabled" in el && !("getContext" in el) && !("options" in el);
}

/** @param {string} id @returns {HTMLElement | null} */
export function $(id) {
  return document.getElementById(id);
}

/** @param {string} id @returns {HTMLCanvasElement | null} */
export function $canvas(id) {
  const el = document.getElementById(id);
  return isCanvasElement(el) ? el : null;
}

/** @param {string} id @returns {HTMLInputElement | null} */
export function $input(id) {
  const el = document.getElementById(id);
  return isInputElement(el) ? el : null;
}

/** @param {string} id @returns {HTMLSelectElement | null} */
export function $select(id) {
  const el = document.getElementById(id);
  return isSelectElement(el) ? el : null;
}

/** @param {string} id @returns {HTMLButtonElement | null} */
export function $button(id) {
  const el = document.getElementById(id);
  return isButtonElement(el) ? el : null;
}

/** @param {string} id @returns {HTMLCanvasElement} */
export function requireCanvas(id) {
  const el = $canvas(id);
  if (!el) throw new Error(`Canvas element #${id} not found`);
  return el;
}

/** @param {string} id @returns {CanvasRenderingContext2D} */
export function requireCanvas2d(id) {
  const ctx = requireCanvas(id).getContext("2d");
  if (!ctx) throw new Error(`2d context unavailable for #${id}`);
  return ctx;
}

/** @param {string} id @returns {HTMLInputElement} */
export function requireInput(id) {
  const el = $input(id);
  if (!el) throw new Error(`Input element #${id} not found`);
  return el;
}

/** @param {HTMLElement | null | undefined} el @param {string} text */
export function setText(el, text) {
  if (el) el.textContent = text;
}
