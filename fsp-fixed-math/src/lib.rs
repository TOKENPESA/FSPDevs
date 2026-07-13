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

/// Applies basis-point spread markup to a Shannon volume (integer-only path).
pub fn apply_bps_spread_shannons(base_shannons: u64, spread_bps: u32) -> u64 {
    let markup = base_shannons.saturating_mul(spread_bps as u64) / 10_000;
    base_shannons.saturating_add(markup)
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
}
