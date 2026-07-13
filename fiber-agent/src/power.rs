#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    /// Full power, instant payment listening.
    AggressiveRealTime,
    /// Deep sleep windows, interrupt-driven wake.
    BatterySaver,
}

impl PowerProfile {
    pub fn base_poll_interval_ms(self) -> u64 {
        match self {
            Self::AggressiveRealTime => 50,
            Self::BatterySaver => 5_000,
        }
    }

    pub fn poll_interval_ms(self) -> u64 {
        self.base_poll_interval_ms()
    }

    pub fn from_env() -> Self {
        match std::env::var("FNN_MODE") {
            Ok(mode) if mode.eq_ignore_ascii_case("simulate") || mode.eq_ignore_ascii_case("sim") => {
                Self::BatterySaver
            }
            _ => Self::AggressiveRealTime,
        }
    }

    pub fn apply_env(self) {
        match self {
            Self::BatterySaver => std::env::set_var("FNN_MODE", "simulate"),
            Self::AggressiveRealTime => std::env::remove_var("FNN_MODE"),
        }
    }
}

const LOW_BATTERY_THRESHOLD_PCT: u8 = 20;
const LOW_BATTERY_POLL_MS: u64 = 300_000;

pub struct AdaptivePowerController {
    pub current_profile: PowerProfile,
    pub battery_level_pct: u8,
    pub network_latency_ms: u32,
}

impl Default for AdaptivePowerController {
    fn default() -> Self {
        Self {
            current_profile: PowerProfile::from_env(),
            battery_level_pct: read_device_battery_level_pct(),
            network_latency_ms: 0,
        }
    }
}

impl AdaptivePowerController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh_device_sensors(&mut self) {
        self.battery_level_pct = read_device_battery_level_pct();
        self.network_latency_ms = read_network_latency_ms();
    }

    pub fn poll_interval_ms(&mut self) -> u64 {
        self.refresh_device_sensors();

        if self.battery_level_pct < LOW_BATTERY_THRESHOLD_PCT {
            return LOW_BATTERY_POLL_MS;
        }

        let base = self.current_profile.base_poll_interval_ms();
        if self.network_latency_ms > 500 {
            return base.saturating_mul(2).min(LOW_BATTERY_POLL_MS);
        }
        base
    }

    pub fn set_profile(&mut self, profile: PowerProfile) {
        match profile {
            PowerProfile::AggressiveRealTime => {
                log::info!("[POWER] Switching to AggressiveRealTime profile.");
            }
            PowerProfile::BatterySaver => {
                log::info!("[POWER] Switching to BatterySaver profile.");
            }
        }
        profile.apply_env();
        self.current_profile = profile;
    }
}

fn read_device_battery_level_pct() -> u8 {
    std::env::var("FNN_DEVICE_BATTERY_LEVEL")
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .map(|level| level.min(100) as u8)
        .unwrap_or(100)
}

fn read_network_latency_ms() -> u32 {
    std::env::var("FNN_DEVICE_NETWORK_LATENCY_MS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_intervals_match_profile_tiers() {
        assert_eq!(
            PowerProfile::AggressiveRealTime.base_poll_interval_ms(),
            50
        );
        assert_eq!(PowerProfile::BatterySaver.base_poll_interval_ms(), 5_000);
    }

    #[test]
    fn low_battery_forces_five_minute_throttle() {
        std::env::set_var("FNN_DEVICE_BATTERY_LEVEL", "15");
        let mut controller = AdaptivePowerController::new();
        assert_eq!(controller.poll_interval_ms(), LOW_BATTERY_POLL_MS);
        std::env::remove_var("FNN_DEVICE_BATTERY_LEVEL");
    }

    #[test]
    fn set_profile_updates_controller_state() {
        let mut controller = AdaptivePowerController::new();
        controller.set_profile(PowerProfile::BatterySaver);
        assert_eq!(controller.current_profile, PowerProfile::BatterySaver);
        std::env::set_var("FNN_DEVICE_BATTERY_LEVEL", "100");
        assert_eq!(controller.poll_interval_ms(), 5_000);
        std::env::remove_var("FNN_DEVICE_BATTERY_LEVEL");
    }
}
