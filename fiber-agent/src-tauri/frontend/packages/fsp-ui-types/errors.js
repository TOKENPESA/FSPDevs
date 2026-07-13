/**
 * Safe error message extraction for catch blocks under strict checkJs.
 */

/** @param {unknown} err @returns {string} */
export function errorMessage(err) {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}
