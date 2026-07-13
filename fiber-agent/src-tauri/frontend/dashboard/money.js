/**
 * Dashboard money formatting — re-exports @fspdevs/fsp-fixed-math primitives.
 */

export {
  CurrencyConfig,
  DEFAULT_CONVERSION_RATE,
  conversionRateBigInt,
  parseAtomicInt,
  parseFiatMinorUnits,
  formatScaledFiat,
  fiatMinorToShannons,
  shannonsToFiatMinor,
} from "../packages/fsp-fixed-math/index.js";

import { parseAtomicInt, parseFiatMinorUnits, shannonsToFiatMinor, DEFAULT_CONVERSION_RATE } from "../packages/fsp-fixed-math/index.js";

/** @param {unknown} value @param {{ label?: string }} [opts] @returns {string} */
export function formatCount(value, { label = "items" } = {}) {
  const n = Number(value ?? 0);
  const text = Number.isFinite(n) ? Math.max(0, Math.trunc(n)).toLocaleString() : "0";
  return `${text} ${label}`;
}

/** @param {unknown} shannons @param {{ suffix?: boolean }} [opts] @returns {string} */
export function formatShannons(shannons, { suffix = true } = {}) {
  if (shannons == null) return "—";
  const n = typeof shannons === "bigint" ? shannons : parseAtomicInt(shannons, "shannons");
  const text = n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return suffix ? `${text} shannons` : text;
}

/** @param {unknown} shannons @returns {string} */
export function formatShannonsCompact(shannons) {
  if (shannons == null) return "—";
  const value = typeof shannons === "bigint" ? shannons : parseAtomicInt(shannons, "shannons");
  const raw = value.toString();
  if (raw.length <= 9) {
    return raw.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  }
  const tiers = [
    { scale: 12n, suffix: "T" },
    { scale: 9n, suffix: "B" },
    { scale: 6n, suffix: "M" },
    { scale: 3n, suffix: "K" },
  ];
  for (const { scale, suffix } of tiers) {
    const divisor = 10n ** scale;
    if (value >= divisor) {
      const whole = value / divisor;
      const subDivisor = divisor / 10n;
      const remainder = value % divisor;
      const frac = subDivisor > 0n ? remainder / subDivisor : 0n;
      const text = frac > 0n ? `${whole.toString()}.${frac.toString()}` : whole.toString();
      return `${text}${suffix}`;
    }
  }
  return raw;
}

/** @param {unknown} fiatMinor @param {string} [currency] @returns {string} */
export function formatFiatMinor(fiatMinor, currency = "TZS") {
  if (fiatMinor == null) return "—";
  const n = typeof fiatMinor === "bigint" ? fiatMinor : parseFiatMinorUnits(fiatMinor, "fiat");
  return `${n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",")} ${currency}`;
}

/** @param {unknown} shannons @param {number | bigint} [rate] @returns {string} */
export function formatFiatFromShannons(shannons, rate = DEFAULT_CONVERSION_RATE) {
  const normalized = typeof shannons === "bigint" ? shannons : parseAtomicInt(shannons, "shannons");
  return formatFiatMinor(shannonsToFiatMinor(normalized, rate));
}
