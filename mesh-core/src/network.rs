use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DIDComm v2 envelope wrapping FSP peer module packets for OOB QR transport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DidCommEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub from: String,
    pub to: String,
    pub body: PeerModulePacket,
}

impl DidCommEnvelope {
    pub const DEFAULT_TYPE: &'static str = "https://didcomm.org/fsp/1.0/oob";

    pub fn agent_did(agent_id: u16) -> String {
        format!("did:fsp:agent:{agent_id}")
    }

    pub fn for_packet(packet: &PeerModulePacket) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            type_: Self::DEFAULT_TYPE.to_string(),
            from: Self::agent_did(packet.source_agent_id),
            to: Self::agent_did(packet.target_agent_id),
            body: packet.clone(),
        }
    }
}

/// Inter-agent module RPC envelope routed across the mesh control plane.
/// When present, `signature` carries a secp256k1 ECDSA proof verified against the source FA pubkey.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerModulePacket {
    pub source_agent_id: u16,
    pub target_agent_id: u16,
    pub target_module: String,
    pub method: String,
    pub payload: Value,
    pub signature: Option<String>,
}

impl PeerModulePacket {
    /// Converts the packet into a universal FSP fallback URI for QR codes or text.
    /// The payload is wrapped in a DIDComm v2 envelope before Base64 encoding.
    pub fn to_fallback_uri(&self) -> Result<String, String> {
        let envelope = DidCommEnvelope::for_packet(self);
        let json_bytes = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        let base64_payload = URL_SAFE_NO_PAD.encode(json_bytes);
        Ok(format!("fsp://oob?data={}", base64_payload))
    }

    /// Parses an FSP fallback URI back into a routed network packet.
    /// Accepts DIDComm-wrapped payloads and legacy raw `PeerModulePacket` blobs.
    pub fn from_fallback_uri(uri: &str) -> Result<Self, String> {
        let prefix = "fsp://oob?data=";
        if !uri.starts_with(prefix) {
            return Err("Invalid FSP Out-of-Band URI protocol".to_string());
        }

        let base64_payload = &uri[prefix.len()..];
        let json_bytes = URL_SAFE_NO_PAD
            .decode(base64_payload)
            .map_err(|e| e.to_string())?;

        if let Ok(envelope) = serde_json::from_slice::<DidCommEnvelope>(&json_bytes) {
            return Ok(envelope.body);
        }

        serde_json::from_slice(&json_bytes).map_err(|e| e.to_string())
    }

    /// Canonical signing payload (signature field excluded).
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        #[derive(Serialize)]
        struct SigningView<'a> {
            source_agent_id: u16,
            target_agent_id: u16,
            target_module: &'a str,
            method: &'a str,
            payload: &'a Value,
        }

        let view = SigningView {
            source_agent_id: self.source_agent_id,
            target_agent_id: self.target_agent_id,
            target_module: &self.target_module,
            method: &self.method,
            payload: &self.payload,
        };

        serde_json::to_vec(&view).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn peer_module_packet_round_trips_json_with_signature() {
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 12,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({
                "loan_id": "loan-9f3c",
                "guarantor_member_id": "2e82a7fb-49ff-5fb7-b030-fb79509a3a7c",
                "principal_shannons": 1_900_000
            }),
            signature: Some("a1b2c3d4e5f6".to_string()),
        };

        let json = serde_json::to_string(&packet).expect("serialize");
        let restored: PeerModulePacket = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(packet, restored);
    }

    #[test]
    fn didcomm_envelope_round_trips_fallback_uri() {
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 12,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({ "loan_id": "loan-9f3c" }),
            signature: None,
        };

        let uri = packet.to_fallback_uri().expect("encode uri");
        assert!(uri.starts_with("fsp://oob?data="));
        let restored = PeerModulePacket::from_fallback_uri(&uri).expect("decode uri");
        assert_eq!(packet, restored);
    }

    #[test]
    fn from_fallback_uri_accepts_legacy_raw_packet() {
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 12,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({ "loan_id": "legacy" }),
            signature: None,
        };
        let json_bytes = serde_json::to_vec(&packet).expect("serialize");
        let base64_payload = URL_SAFE_NO_PAD.encode(json_bytes);
        let uri = format!("fsp://oob?data={base64_payload}");
        let restored = PeerModulePacket::from_fallback_uri(&uri).expect("legacy decode");
        assert_eq!(packet, restored);
    }

    #[test]
    fn from_fallback_uri_rejects_invalid_protocol() {
        let err = PeerModulePacket::from_fallback_uri("https://example.com").unwrap_err();
        assert!(err.contains("Invalid FSP Out-of-Band URI protocol"));
    }
}
