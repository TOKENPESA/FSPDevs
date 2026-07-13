use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical schema version tag for control plane tracking
pub const MESH_MONITOR_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FiatProvider {
    Mpesa,
    AirtelMoney,
    MtnMoney,
}

/// Layer-2 / RGB++ / UDT asset identifier for multi-asset HTLC channels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum L2Asset {
    CkbNative,
    RusdStablecoin,
    /// RGB++ isomorphic asset bound to a CKB cell type (not RGB20).
    #[serde(rename = "RGB++", alias = "RGB20")]
    RgbPlusPlus(String),
    /// Generic UDT / xUDT script-hash identifier on CKB.
    #[serde(rename = "UDT", alias = "xUDT")]
    UDT(String),
}

impl L2Asset {
    pub fn ledger_label(&self) -> String {
        match self {
            Self::CkbNative => "CKB".to_string(),
            Self::RusdStablecoin => "RUSD".to_string(),
            Self::RgbPlusPlus(hash) => format!("RGB++:{hash}"),
            Self::UDT(hash) => format!("UDT:{hash}"),
        }
    }

    pub fn from_ledger_label(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("CKB") {
            return Ok(Self::CkbNative);
        }
        if trimmed.eq_ignore_ascii_case("RUSD") {
            return Ok(Self::RusdStablecoin);
        }
        if let Some(hash) = trimmed.strip_prefix("RGB++:") {
            if hash.is_empty() {
                return Err("RGB++ asset id must not be empty".to_string());
            }
            return Ok(Self::RgbPlusPlus(hash.to_string()));
        }
        // Legacy ledger rows may still carry the pre-RGB++ label.
        if let Some(hash) = trimmed.strip_prefix("RGB20:") {
            if hash.is_empty() {
                return Err("RGB++ asset id must not be empty".to_string());
            }
            return Ok(Self::RgbPlusPlus(hash.to_string()));
        }
        if let Some(hash) = trimmed.strip_prefix("UDT:") {
            if hash.is_empty() {
                return Err("UDT asset id must not be empty".to_string());
            }
            return Ok(Self::UDT(hash.to_string()));
        }
        if let Some(hash) = trimmed.strip_prefix("xUDT:") {
            if hash.is_empty() {
                return Err("xUDT asset id must not be empty".to_string());
            }
            return Ok(Self::UDT(hash.to_string()));
        }
        Err(format!("unknown L2 asset label '{trimmed}'"))
    }
}

/// Per-asset atomic balance carried on an edge transaction or channel HTLC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetCapacity {
    pub asset: L2Asset,
    pub amount_atomic: u64,
    /// Fractional RWA metadata (parts per billion) when RGB++ isomorphic binding applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rwa_fraction_nanos: Option<u64>,
    /// RGB++ cell type hash for isomorphic CKB binding verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb_cell_type_hash: Option<String>,
}

impl AssetCapacity {
    pub fn new(asset: L2Asset, amount_atomic: u64) -> Self {
        Self {
            asset,
            amount_atomic,
            rwa_fraction_nanos: None,
            rgb_cell_type_hash: None,
        }
    }

    pub fn rgb_plus_plus(
        cell_type_hash: impl Into<String>,
        amount_atomic: u64,
        fraction_nanos: Option<u64>,
    ) -> Self {
        let hash = cell_type_hash.into();
        Self {
            asset: L2Asset::RgbPlusPlus(hash.clone()),
            amount_atomic,
            rwa_fraction_nanos: fraction_nanos,
            rgb_cell_type_hash: Some(hash),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeTxType {
    /// Customer gives paper cash; agent routes L2 tokens out to customer wallet.
    CashIn,
    /// Customer transfers L2 tokens to agent; agent releases physical cash or telco push.
    CashOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiatMetadata {
    pub provider: FiatProvider,
    pub agent_account: String,
    pub local_currency: String,
    pub reference_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeTransaction {
    pub tx_id: Uuid,
    pub agent_id: u16,
    pub tx_type: EdgeTxType,
    /// Multi-asset HTLC capacities (replaces legacy single `amount_atomic`).
    pub capacities: Vec<AssetCapacity>,
    pub fiat_amount: f64,
    pub counterparty_pubkey: String,
    pub payment_hash: Option<String>,
    pub preimage: Option<String>,
    pub timestamp: i64,
    pub is_synchronized: bool,
}

/// Builder input for legacy single-asset edge ledger rows.
#[derive(Debug, Clone)]
pub struct SingleCapacityParams {
    pub tx_id: Uuid,
    pub agent_id: u16,
    pub tx_type: EdgeTxType,
    pub asset: L2Asset,
    pub amount_atomic: u64,
    pub fiat_amount: f64,
    pub counterparty_pubkey: String,
    pub payment_hash: Option<String>,
    pub preimage: Option<String>,
    pub timestamp: i64,
    pub is_synchronized: bool,
}

impl EdgeTransaction {
    pub fn single_capacity(params: SingleCapacityParams) -> Self {
        Self {
            tx_id: params.tx_id,
            agent_id: params.agent_id,
            tx_type: params.tx_type,
            capacities: vec![AssetCapacity::new(params.asset, params.amount_atomic)],
            fiat_amount: params.fiat_amount,
            counterparty_pubkey: params.counterparty_pubkey,
            payment_hash: params.payment_hash,
            preimage: params.preimage,
            timestamp: params.timestamp,
            is_synchronized: params.is_synchronized,
        }
    }

    pub fn total_atomic(&self) -> u64 {
        self.capacities
            .iter()
            .fold(0u64, |acc, cap| acc.saturating_add(cap.amount_atomic))
    }

    pub fn capacity_for(&self, asset: &L2Asset) -> Option<u64> {
        self.capacities
            .iter()
            .find(|cap| &cap.asset == asset)
            .map(|cap| cap.amount_atomic)
    }

    pub fn primary_capacity(&self) -> Option<&AssetCapacity> {
        self.capacities.first()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FloatExhaustionTelemetry {
    pub agent_id: u16,
    pub provider: FiatProvider,
    pub current_fiat_balance: f64,
    pub critical_fiat_floor: f64,
    pub digital_l2_balance_shannons: u64,
    pub drain_velocity_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeLayersConfig {
    /// Layer 2: Flat kiosk commission baseline (e.g., 500 TZS flat)
    pub kiosk_flat_commission: f64,
    /// Layer 2: Variable kiosk markup margin parts-per-million (e.g., 10,000 PPM = 1%)
    pub kiosk_proportional_ppm: u32,
    /// Layer 3: Sovereign Government transfer levy percentage (e.g., 0.001 = 0.1% transaction tax)
    pub sovereign_levy_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeCalculationBreakdown {
    pub principal_fiat_amount: f64,
    pub layer1_l2_routing_fee_fiat: f64,
    pub layer2_kiosk_commission_fiat: f64,
    pub layer3_sovereign_levy_fiat: f64,
    pub absolute_total_fiat_cost: f64,
    pub absolute_total_shannons: u64,
}

fn default_pulse_status() -> String {
    "MESH_HEARTBEAT".to_string()
}

/// Sidecar → MFA telemetry envelope (HTTP POST /telemetry).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MeshPulsePayload {
    #[serde(alias = "agent", alias = "reporter")]
    pub agent_id: u16,
    pub timestamp: u64,
    /// Monotonic increment block fence (replay protection).
    pub nonce: u64,
    pub local_capacity_shannons: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_hex: Option<String>,
    #[serde(default = "default_pulse_status")]
    pub status: String,
    #[serde(default)]
    pub active_mesh_neighbors: Vec<u16>,
    #[serde(default, alias = "target")]
    pub report_target: u16,
    #[serde(default)]
    pub attempt: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fnn_pubkey_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_connect_address: Option<String>,
    /// Per-asset outbound channel capacities reported by edge sidecars (RGB++, xUDT, CKB).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_capacities: Vec<AssetCapacity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEnvelope {
    pub schema_version: String,
    pub timestamp: u64,
    pub event_id: Uuid,
    #[serde(flatten)]
    pub data: MonitorEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum MonitorEventData {
    #[serde(rename = "REQUITY_INJECTION")]
    LiquidityInjection {
        node: u16,
        amount_shannons: u64,
        vault: String,
    },
    #[serde(rename = "TOPOLOGY_SYNC")]
    TopologySync {
        version: u64,
        updated_channels_count: usize,
    },
    #[serde(rename = "COPILOT_PREDICTION_ALERT")]
    CopilotAlert {
        node: u16,
        channel_id: String,
        drain_rate_shannons_sec: f64,
        seconds_remaining: f64,
    },
    #[serde(rename = "INTENT_SWAP_SUCCESS")]
    IntentSwapSuccess {
        swap_id: Uuid,
        amount: u64,
    },
}

impl MonitorEnvelope {
    pub fn wrap(data: MonitorEventData) -> Self {
        Self {
            schema_version: MESH_MONITOR_SCHEMA_VERSION.to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            event_id: Uuid::new_v4(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_envelope_serializes_with_event_tag() {
        let envelope = MonitorEnvelope::wrap(MonitorEventData::TopologySync {
            version: 42,
            updated_channels_count: 3,
        });
        let json = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(json["schema_version"], MESH_MONITOR_SCHEMA_VERSION);
        assert_eq!(json["event"], "TOPOLOGY_SYNC");
        assert_eq!(json["payload"]["version"], 42);
    }

    #[test]
    fn edge_transaction_round_trips_json() {
        let tx = EdgeTransaction::single_capacity(SingleCapacityParams {
            tx_id: Uuid::new_v4(),
            agent_id: 44,
            tx_type: EdgeTxType::CashOut,
            asset: L2Asset::CkbNative,
            amount_atomic: 50_000_000,
            fiat_amount: 12_500.0,
            counterparty_pubkey: "0xabc".to_string(),
            payment_hash: Some("hash".to_string()),
            preimage: None,
            timestamp: 1_700_000_000,
            is_synchronized: false,
        });
        let json = serde_json::to_string(&tx).expect("serialize");
        let decoded: EdgeTransaction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.agent_id, 44);
        assert_eq!(decoded.tx_type, EdgeTxType::CashOut);
        assert_eq!(decoded.total_atomic(), 50_000_000);
    }

    #[test]
    fn multi_asset_capacities_sum_correctly() {
        let tx = EdgeTransaction {
            tx_id: Uuid::new_v4(),
            agent_id: 1,
            tx_type: EdgeTxType::CashIn,
            capacities: vec![
                AssetCapacity::new(L2Asset::CkbNative, 1_000_000),
                AssetCapacity::rgb_plus_plus("0xrgbpp_stock", 500, Some(250_000_000)),
            ],
            fiat_amount: 0.0,
            counterparty_pubkey: "pk".to_string(),
            payment_hash: None,
            preimage: None,
            timestamp: 0,
            is_synchronized: false,
        };
        assert_eq!(tx.total_atomic(), 1_000_500);
        assert_eq!(
            tx.capacity_for(&L2Asset::RgbPlusPlus("0xrgbpp_stock".to_string())),
            Some(500)
        );
    }

    #[test]
    fn l2_asset_ledger_label_round_trip() {
        let asset = L2Asset::RgbPlusPlus("0xdead".to_string());
        let label = asset.ledger_label();
        assert_eq!(label, "RGB++:0xdead");
        assert_eq!(
            L2Asset::from_ledger_label(&label).expect("parse"),
            asset
        );
    }

    #[test]
    fn fee_layers_config_round_trips_json() {
        let config = FeeLayersConfig {
            kiosk_flat_commission: 500.0,
            kiosk_proportional_ppm: 10_000,
            sovereign_levy_rate: 0.001,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let decoded: FeeLayersConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
    }

    #[test]
    fn mesh_pulse_accepts_legacy_agent_alias() {
        let json = r#"{"agent":44,"timestamp":1,"nonce":2,"local_capacity_shannons":500}"#;
        let payload: MeshPulsePayload = serde_json::from_str(json).expect("deserialize");
        assert_eq!(payload.agent_id, 44);
        assert_eq!(payload.nonce, 2);
        assert_eq!(payload.local_capacity_shannons, 500);
    }
}
