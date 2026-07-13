/**
 * Structured logging for dashboard, MFA console, and sidecar frontends.
 * Never use console.log directly — import createLogger instead.
 */

const LEVELS = { debug: 0, info: 1, warn: 2, error: 3 };

/** @param {'debug' | 'info' | 'warn' | 'error'} level @param {string} scope @param {string} message @param {unknown} [detail] */
function emit(level, scope, message, detail) {
  const prefix = `[${scope}]`;
  const sink = level === "error" ? console.error : level === "warn" ? console.warn : console.info;
  if (detail !== undefined) {
    sink(prefix, message, detail);
  } else {
    sink(prefix, message);
  }
}

/**
 * @param {string} scope - Component tag, e.g. "mfa-ui", "sidecar-runtime"
 */
export function createLogger(scope) {
  return {
    /** @param {string} message @param {unknown} [detail] */
    debug(message, detail) {
      emit("debug", scope, message, detail);
    },
    /** @param {string} message @param {unknown} [detail] */
    info(message, detail) {
      emit("info", scope, message, detail);
    },
    /** @param {string} message @param {unknown} [detail] */
    warn(message, detail) {
      emit("warn", scope, message, detail);
    },
    /** @param {string} message @param {unknown} [detail] */
    error(message, detail) {
      emit("error", scope, message, detail);
    },
  };
}

/** Default dashboard logger */
export const log = createLogger("dashboard");
