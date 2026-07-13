use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TelemetryAlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BalanceDepletedPayload {
    pub agent_id: u16,
    pub short_channel_id: String,
    pub available_outbound_shannons: u64,
    pub minimum_required_shannons: u64,
    pub agent_fnn_pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TelemetryEvent {
    Heartbeat { agent_id: u16 },
    ModuleStatus {
        agent_id: u16,
        module_name: String,
        is_active: bool,
    },
    /// The trigger for autonomous funding.
    BalanceDepleted(BalanceDepletedPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryPacket {
    pub packet_id: Uuid,
    pub timestamp_ms: u64,
    pub severity: TelemetryAlertSeverity,
    pub event: TelemetryEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_depleted_packet_round_trips_json() {
        let packet = TelemetryPacket {
            packet_id: Uuid::nil(),
            timestamp_ms: 1_700_000_000_000,
            severity: TelemetryAlertSeverity::Critical,
            event: TelemetryEvent::BalanceDepleted(BalanceDepletedPayload {
                agent_id: 44,
                short_channel_id: "0xabc123".to_string(),
                available_outbound_shannons: 50_000,
                minimum_required_shannons: 1_000_000,
                agent_fnn_pubkey: "03deadbeef".to_string(),
            }),
        };

        let json = serde_json::to_string(&packet).expect("serialize");
        let restored: TelemetryPacket = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(packet, restored);
    }
}
