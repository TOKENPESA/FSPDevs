//! Fixed-point fiat and Shannon scaling primitives shared across FSP crates.

use serde::{Deserialize, Serialize};

/// Mobile money / telco float tracking (atomic sub-units, e.g. shannons or cents).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelcoFloatFixedPoint {
    pub provider: String,
    pub account_id: String,
    pub live_balance_units: u64,
    pub critical_floor_units: u64,
}

impl TelcoFloatFixedPoint {
    pub fn has_dropped_below_floor(&self) -> bool {
        self.live_balance_units <= self.critical_floor_units
    }
}

/// Returns `10^decimals` capped for safe u64 arithmetic.
pub fn atomic_scale_multiplier(atomic_decimals: u32) -> u64 {
    10u64.saturating_pow(atomic_decimals.min(19))
}

/// Deterministic integer fiat→shannon conversion using an atomic scale multiplier.
pub fn fiat_minor_to_shannons_atomic(fiat_minor_units: u64, scale_multiplier: u64) -> u64 {
    fiat_minor_units.saturating_mul(scale_multiplier)
}

/// Converts a floating fiat amount to rounded Shannon atoms using `atomic_decimals`.
pub fn round_fiat_to_shannons(fiat_amount: f64, atomic_decimals: u32) -> u64 {
    if !fiat_amount.is_finite() || fiat_amount <= 0.0 {
        return 0;
    }
    let scale = atomic_scale_multiplier(atomic_decimals) as f64;
    fiat_amount.mul_add(scale, 0.0).round().max(0.0) as u64
}

/// Converts fiat at an explicit shannon-per-unit FX rate with rounding before the u64 cast.
pub fn fiat_at_rate_to_shannons(fiat_amount: f64, shannons_per_fiat_unit: f64) -> u64 {
    if !fiat_amount.is_finite()
        || !shannons_per_fiat_unit.is_finite()
        || fiat_amount <= 0.0
        || shannons_per_fiat_unit <= 0.0
    {
        return 0;
    }
    fiat_amount
        .mul_add(shannons_per_fiat_unit, 0.0)
        .round()
        .max(0.0) as u64
}

/// Integer inverse of [`fiat_at_rate_to_shannons`] using a whole shannon-per-unit rate.
pub fn shannons_at_rate_to_fiat_units(shannons: u64, shannons_per_fiat_unit: u64) -> u64 {
    if shannons_per_fiat_unit == 0 {
        return 0;
    }
    shannons / shannons_per_fiat_unit
}

/// Converts basis points to percent milles (bps / 100 → percent with 2 implied decimals retained as milles).
/// Example: 250 bps → 2500 milles of a percent unit used as `2.50%` display when divided by 1000.
pub fn interest_bps_to_percent_millis(rate_bps: u32) -> u32 {
    rate_bps.saturating_mul(10)
}

/// Applies basis-point spread markup to a Shannon volume (integer-only path).
pub fn apply_bps_spread_shannons(base_shannons: u64, spread_bps: u32) -> u64 {
    let markup = base_shannons.saturating_mul(spread_bps as u64) / 10_000;
    base_shannons.saturating_add(markup)
}

/// Applies a parts-per-million fee/markup to a Shannon amount (integer-only).
pub fn apply_ppm_shannons(base_shannons: u64, ppm: u32) -> u64 {
    base_shannons.saturating_mul(ppm as u64) / 1_000_000
}

/// Deducts a basis-point levy from a Shannon principal (integer-only; floors the levy).
pub fn apply_bps_levy_shannons(base_shannons: u64, levy_bps: u32) -> u64 {
    let levy = base_shannons.saturating_mul(levy_bps as u64) / 10_000;
    base_shannons.saturating_sub(levy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_scale_multiplier_caps_decimals() {
        assert_eq!(atomic_scale_multiplier(8), 100_000_000);
    }

    #[test]
    fn round_fiat_to_shannons_rounds_before_cast() {
        assert_eq!(round_fiat_to_shannons(1.5, 8), 150_000_000);
    }

    #[test]
    fn apply_bps_spread_adds_markup() {
        assert_eq!(apply_bps_spread_shannons(1_000_000, 100), 1_010_000);
    }

    #[test]
    fn fiat_at_rate_to_shannons_rounds() {
        assert_eq!(fiat_at_rate_to_shannons(100.0, 38.0), 3_800);
        assert_eq!(fiat_at_rate_to_shannons(10.5, 38.0), 399);
    }

    #[test]
    fn shannons_at_rate_to_fiat_units_floors() {
        assert_eq!(shannons_at_rate_to_fiat_units(3_800, 38), 100);
    }

    #[test]
    fn apply_ppm_shannons_exact() {
        assert_eq!(apply_ppm_shannons(1_000_000, 10_000), 10_000);
    }
}
