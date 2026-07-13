use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use mesh_core::types::FiatProvider;
use serde::Deserialize;

use crate::module_catalog::is_known_module_id;
use crate::storage::DEFAULT_STATE_DIR;

const PROFILE_FILE_NAME: &str = "sidecar.profile.toml";
const MAX_PROFILE_BYTES: u64 = 32_768;
const MAX_MSISDN_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarProfilePreset {
    Kiosk,
    Coop,
    Relay,
    Utility,
    Full,
    Custom,
}

impl SidecarProfilePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kiosk => "kiosk",
            Self::Coop => "coop",
            Self::Relay => "relay",
            Self::Utility => "utility",
            Self::Full => "full",
            Self::Custom => "custom",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "kiosk" => Some(Self::Kiosk),
            "coop" => Some(Self::Coop),
            "relay" => Some(Self::Relay),
            "utility" => Some(Self::Utility),
            "full" => Some(Self::Full),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FiatBridgeModuleConfig {
    pub enabled: bool,
    pub provider: Option<String>,
    pub msisdn: Option<String>,
    pub critical_fiat_floor: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DicobaModuleConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct TelcoB2cSweepModuleConfig {
    pub enabled: bool,
    pub default_provider: Option<String>,
    pub critical_floor_units: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct LumeYieldingModuleConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SecuritiesComplianceModuleConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FiberAgentSwarmModuleConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProfileModulesSection {
    pub dicoba: DicobaModuleConfig,
    pub fiat_bridge: FiatBridgeModuleConfig,
    pub telco_b2c_sweep: TelcoB2cSweepModuleConfig,
    pub lume_yielding: LumeYieldingModuleConfig,
    pub securities_compliance: SecuritiesComplianceModuleConfig,
    pub fiber_agent_swarm: FiberAgentSwarmModuleConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ProfileSidecarSection {
    pub agent_id: Option<u16>,
    pub profile: String,
}

impl Default for ProfileSidecarSection {
    fn default() -> Self {
        Self {
            agent_id: None,
            profile: SidecarProfilePreset::Full.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ProfileNetworkSection {
    pub mfa_host: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawSidecarProfile {
    sidecar: ProfileSidecarSection,
    modules: ProfileModulesSection,
    network: ProfileNetworkSection,
}

#[derive(Debug, Clone)]
pub struct SidecarProfile {
    pub agent_id: u16,
    pub preset: SidecarProfilePreset,
    pub modules: ProfileModulesSection,
    pub mfa_host: Option<String>,
    pub source: String,
}

impl SidecarProfile {
    pub fn enabled_module_ids(&self) -> Vec<&'static str> {
        let mut ids = Vec::new();
        if self.modules.dicoba.enabled {
            ids.push("dicoba");
        }
        if self.modules.fiat_bridge.enabled {
            ids.push("fiat_bridge");
        }
        if self.modules.telco_b2c_sweep.enabled {
            ids.push("telco_b2c_sweep");
        }
        if self.modules.lume_yielding.enabled {
            ids.push("lume_yielding");
        }
        if self.modules.securities_compliance.enabled {
            ids.push("securities_compliance");
        }
        if self.modules.fiber_agent_swarm.enabled {
            ids.push("fiber_agent_swarm");
        }
        ids
    }

    pub fn preset_label(&self) -> &'static str {
        self.preset.as_str()
    }
}

pub fn resolve_profile_path(agent_id: u16) -> PathBuf {
    if let Ok(path) = env::var("SIDECAR_PROFILE_PATH") {
        return PathBuf::from(path);
    }

    let dir = env::var("FIBER_AGENT_STATE_DIR").unwrap_or_else(|_| DEFAULT_STATE_DIR.to_string());
    PathBuf::from(dir)
        .join(format!("fa-{agent_id:04}"))
        .join(PROFILE_FILE_NAME)
}

pub fn load_sidecar_profile(agent_id: u16) -> Result<SidecarProfile, String> {
    let path = resolve_profile_path(agent_id);
    if path.is_file() {
        return load_profile_from_path(agent_id, &path);
    }

    if let Ok(preset_raw) = env::var("SIDECAR_PROFILE") {
        if let Some(preset) = SidecarProfilePreset::parse(&preset_raw) {
            return Ok(profile_from_preset(agent_id, preset, "env:SIDECAR_PROFILE"));
        }
        return Err(format!(
            "SIDECAR_PROFILE '{preset_raw}' is invalid (kiosk|coop|relay|utility|full|custom)"
        ));
    }

    Ok(profile_from_preset(
        agent_id,
        SidecarProfilePreset::Full,
        "default:full",
    ))
}

pub fn load_profile_from_path(agent_id: u16, path: &Path) -> Result<SidecarProfile, String> {
    validate_profile_path(path)?;
    let metadata = fs::metadata(path)
        .map_err(|err| format!("profile metadata unavailable ({}): {err}", path.display()))?;
    if metadata.len() > MAX_PROFILE_BYTES {
        return Err(format!(
            "profile file exceeds {MAX_PROFILE_BYTES} byte limit: {}",
            path.display()
        ));
    }

    let raw_text = fs::read_to_string(path)
        .map_err(|err| format!("profile read failed ({}): {err}", path.display()))?;
    let raw: RawSidecarProfile = toml::from_str(&raw_text)
        .map_err(|err| format!("profile parse failed ({}): {err}", path.display()))?;

    let preset = SidecarProfilePreset::parse(&raw.sidecar.profile).unwrap_or({
        log::warn!(
            "Unknown profile preset '{}' in {}; treating as custom",
            raw.sidecar.profile,
            path.display()
        );
        SidecarProfilePreset::Custom
    });

    if let Some(file_agent_id) = raw.sidecar.agent_id {
        if file_agent_id != agent_id {
            return Err(format!(
                "profile agent_id {file_agent_id} does not match runtime agent FA-{agent_id}"
            ));
        }
    }

    let modules = raw.modules;
    validate_module_config(&modules)?;
    validate_network_config(&raw.network)?;

    Ok(SidecarProfile {
        agent_id,
        preset,
        modules,
        mfa_host: sanitize_optional_host(raw.network.mfa_host),
        source: path.display().to_string(),
    })
}

pub fn profile_from_preset(
    agent_id: u16,
    preset: SidecarProfilePreset,
    source: &str,
) -> SidecarProfile {
    let mut modules = ProfileModulesSection::default();
    apply_preset_defaults(preset, &mut modules);
    SidecarProfile {
        agent_id,
        preset,
        modules,
        mfa_host: None,
        source: source.to_string(),
    }
}

fn apply_preset_defaults(preset: SidecarProfilePreset, modules: &mut ProfileModulesSection) {
    match preset {
        SidecarProfilePreset::Kiosk => {
            modules.dicoba.enabled = false;
            modules.fiat_bridge.enabled = true;
            modules.telco_b2c_sweep.enabled = true;
        }
        SidecarProfilePreset::Coop | SidecarProfilePreset::Full => {
            modules.dicoba.enabled = true;
            modules.fiat_bridge.enabled = true;
            modules.telco_b2c_sweep.enabled = true;
            modules.lume_yielding.enabled = true;
            modules.securities_compliance.enabled = true;
            modules.fiber_agent_swarm.enabled = true;
        }
        SidecarProfilePreset::Relay | SidecarProfilePreset::Utility => {
            modules.dicoba.enabled = false;
            modules.fiat_bridge.enabled = false;
        }
        SidecarProfilePreset::Custom => {}
    }
}

fn validate_profile_path(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("profile path must not contain '..' segments".to_string());
    }
    Ok(())
}

fn validate_module_config(modules: &ProfileModulesSection) -> Result<(), String> {
    if modules.dicoba.enabled && !is_known_module_id("dicoba") {
        return Err("dicoba is not a known module".to_string());
    }
    if modules.fiat_bridge.enabled {
        if !is_known_module_id("fiat_bridge") {
            return Err("fiat_bridge is not a known module".to_string());
        }
        if let Some(msisdn) = modules.fiat_bridge.msisdn.as_deref() {
            validate_msisdn(msisdn)?;
        }
        if let Some(provider) = modules.fiat_bridge.provider.as_deref() {
            parse_fiat_provider(provider)?;
        }
        if let Some(floor) = modules.fiat_bridge.critical_fiat_floor {
            if !floor.is_finite() || !(0.0..=1_000_000_000.0).contains(&floor) {
                return Err("critical_fiat_floor must be a finite value between 0 and 1e9".to_string());
            }
        }
    }
    if modules.telco_b2c_sweep.enabled && !is_known_module_id("telco_b2c_sweep") {
        return Err("telco_b2c_sweep is not a known module".to_string());
    }
    if modules.lume_yielding.enabled && !is_known_module_id("lume_yielding") {
        return Err("lume_yielding is not a known module".to_string());
    }
    if modules.securities_compliance.enabled && !is_known_module_id("securities_compliance") {
        return Err("securities_compliance is not a known module".to_string());
    }
    if modules.fiber_agent_swarm.enabled && !is_known_module_id("fiber_agent_swarm") {
        return Err("fiber_agent_swarm is not a known module".to_string());
    }
    Ok(())
}

fn validate_network_config(network: &ProfileNetworkSection) -> Result<(), String> {
    if let Some(host) = network.mfa_host.as_deref() {
        validate_host_token(host)?;
    }
    Ok(())
}

pub fn validate_msisdn(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MSISDN_LEN {
        return Err(format!(
            "msisdn must be 1..={MAX_MSISDN_LEN} digits after trimming"
        ));
    }
    if !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("msisdn must contain digits only".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn parse_fiat_provider(raw: &str) -> Result<FiatProvider, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mpesa" => Ok(FiatProvider::Mpesa),
        "airtel" | "airtel_money" | "airtelmoney" => Ok(FiatProvider::AirtelMoney),
        "mtn" | "mtn_money" | "mtnmoney" => Ok(FiatProvider::MtnMoney),
        other => Err(format!("unsupported fiat provider '{other}'")),
    }
}

fn validate_host_token(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err("mfa_host must be between 1 and 128 characters".to_string());
    }
    if trimmed.contains("://") || trimmed.contains('/') {
        return Err("mfa_host must be host:port without scheme or path".to_string());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | ':' | '-' | '_'))
    {
        return Err("mfa_host contains invalid characters".to_string());
    }
    Ok(())
}

fn sanitize_optional_host(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        validate_host_token(&value)
            .map(|()| value.trim().to_string())
            .ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiosk_preset_disables_dicoba() {
        let profile = profile_from_preset(7, SidecarProfilePreset::Kiosk, "test");
        assert!(!profile.modules.dicoba.enabled);
        assert!(profile.modules.fiat_bridge.enabled);
    }

    #[test]
    fn relay_preset_mounts_no_modules() {
        let profile = profile_from_preset(3, SidecarProfilePreset::Relay, "test");
        assert!(profile.enabled_module_ids().is_empty());
    }

    #[test]
    fn msisdn_rejects_symbols() {
        assert!(validate_msisdn("2557abc").is_err());
        assert!(validate_msisdn("255700000000").is_ok());
    }

    #[test]
    fn profile_path_rejects_parent_dir() {
        assert!(validate_profile_path(Path::new("../sidecar.profile.toml")).is_err());
    }
}
