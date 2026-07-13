/**
 * @fspdevs/fsp-fixed-math — fixed-point fiat and Shannon primitives.
 * Never use parseFloat for financial inputs in calling code.
 */

export const CurrencyConfig = {
  SCALE_FACTOR: 100000n,

  /** @param {string} fiatAmountString */
  toIntegerUnits(fiatAmountString) {
    const [integral, fractional = ""] = fiatAmountString.split(".");
    const paddedFraction = fractional.padEnd(5, "0").substring(0, 5);
    return BigInt(integral) * CurrencyConfig.SCALE_FACTOR + BigInt(paddedFraction);
  },

  /** @param {bigint | string | number} integerUnits */
  toDisplayString(integerUnits) {
    const units = BigInt(integerUnits);
    const integral = units / CurrencyConfig.SCALE_FACTOR;
    const fractional = units % CurrencyConfig.SCALE_FACTOR;
    return `${integral}.${fractional.toString().padStart(5, "0")}`;
  },
};

export const DEFAULT_CONVERSION_RATE = 38;

/** @param {bigint | string | number} [rate] */
export function conversionRateBigInt(rate = DEFAULT_CONVERSION_RATE) {
  if (typeof rate === "bigint") {
    return rate > 0n ? rate : 1n;
  }
  if (typeof rate === "number") {
    if (!Number.isFinite(rate) || rate <= 0) return 1n;
    return BigInt(Math.trunc(rate));
  }
  const raw = String(rate).trim();
  if (!/^\d+$/.test(raw)) {
    throw new Error("Invalid conversion rate: must be a whole number");
  }
  const parsed = BigInt(raw);
  return parsed > 0n ? parsed : 1n;
}

/** @param {unknown} value @param {string} [field] */
export function parseAtomicInt(value, field = "amount") {
  if (value == null || value === "") return 0n;
  const raw = String(value).trim().replace(/,/g, "");
  if (!/^\d+$/.test(raw)) {
    throw new Error(`Invalid ${field}: must be a whole number`);
  }
  return BigInt(raw);
}

/** @param {unknown} value @param {string} [field] */
export function parseFiatMinorUnits(value, field = "fiat") {
  if (value == null || value === "") return 0n;
  const raw = String(value).trim().replace(/,/g, "");
  if (raw.includes(".")) {
    if (!/^\d+(\.\d+)?$/.test(raw)) {
      throw new Error(`Invalid ${field}: must be a decimal or whole number`);
    }
    return CurrencyConfig.toIntegerUnits(raw);
  }
  if (!/^\d+$/.test(raw)) {
    throw new Error(`Invalid ${field}: must be a whole number`);
  }
  return BigInt(raw);
}

/** @param {unknown} integerUnits @param {string} [currency] */
export function formatScaledFiat(integerUnits, currency = "TZS") {
  if (integerUnits == null) return "—";
  const n = typeof integerUnits === "bigint" ? integerUnits : parseAtomicInt(integerUnits, "fiat");
  return `${CurrencyConfig.toDisplayString(n)} ${currency}`;
}

/** @param {bigint | string | number} fiatMinor @param {bigint | string | number} [rate] */
export function fiatMinorToShannons(fiatMinor, rate = DEFAULT_CONVERSION_RATE) {
  const r = conversionRateBigInt(rate);
  const minor = typeof fiatMinor === "bigint" ? fiatMinor : parseAtomicInt(fiatMinor, "fiatMinor");
  return minor * r;
}

/** @param {bigint | string | number} shannons @param {bigint | string | number} [rate] */
export function shannonsToFiatMinor(shannons, rate = DEFAULT_CONVERSION_RATE) {
  const r = conversionRateBigInt(rate);
  const s = typeof shannons === "bigint" ? shannons : parseAtomicInt(shannons, "shannons");
  return s / r;
}
