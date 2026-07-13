use std::collections::HashMap;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1};
use sha2::{Digest, Sha256};

use crate::config::DEDUPE_CAP;
use crate::state::AppState;
use crate::types::RouteRequestPayload;
use mesh_core::types::MeshPulsePayload;
use mesh_core::{telemetry_canonical_message, valid_agent_id, MeshError};

const MAX_CLOCK_SKEW_SECONDS: i64 = 15;

/// Per-agent telemetry nonce high-water marks — rejects replayed or out-of-order heartbeats.
static TELEMETRY_NONCE_CACHE: Lazy<std::sync::RwLock<HashMap<u16, u64>>> =
    Lazy::new(|| std::sync::RwLock::new(HashMap::new()));

pub fn verify_telemetry_sequence(payload: &MeshPulsePayload) -> Result<(), MeshError> {
    let new_nonce = payload.nonce;
    let mut registry = TELEMETRY_NONCE_CACHE
        .write()
        .map_err(|_| MeshError::InvalidPayload("telemetry nonce registry poisoned".to_string()))?;

    if let Some(&last_seen_nonce) = registry.get(&payload.agent_id) {
        if new_nonce <= last_seen_nonce {
            return Err(MeshError::InvalidPayload(format!(
                "❌ [REPLAY FAULT] Rejected out-of-order packet on FA-{}. Incoming nonce: {}, High-watermark: {}",
                payload.agent_id, new_nonce, last_seen_nonce
            )));
        }
    }

    registry.insert(payload.agent_id, new_nonce);
    Ok(())
}

pub fn verify_telemetry_timestamp(payload: &MeshPulsePayload) -> Result<(), MeshError> {
    if payload.timestamp == 0 {
        return Err(MeshError::InvalidPayload(
            "Missing or zero timestamp field".to_string(),
        ));
    }

    let now = Utc::now();
    let payload_time = DateTime::from_timestamp(payload.timestamp as i64, 0).ok_or_else(|| {
        MeshError::InvalidPayload("Malformed timestamp field".to_string())
    })?;

    let delta = now.signed_duration_since(payload_time).num_seconds().abs();

    if delta > MAX_CLOCK_SKEW_SECONDS {
        return Err(MeshError::InvalidPayload(format!(
            "Telemetry expired. Clock skew detected: {delta} seconds. Limit: {MAX_CLOCK_SKEW_SECONDS}"
        )));
    }
    Ok(())
}

pub fn validate_telemetry(p: &MeshPulsePayload) -> bool {
    if p.public_key_hex.is_none() || p.signature_hex.is_none() {
        eprintln!(
            "❌ [POLICY ENFORCEMENT] Dropped unauthenticated legacy browser payload from Node FA-{}",
            p.agent_id
        );
        return false;
    }

    if !valid_agent_id(p.agent_id)
        || (p.report_target != 0 && !valid_agent_id(p.report_target))
        || p.active_mesh_neighbors.len() > 8
        || !p
            .active_mesh_neighbors
            .iter()
            .all(|&n| valid_agent_id(n))
    {
        return false;
    }

    matches!(
        p.status.as_str(),
        "MESH_HEARTBEAT" | "ALERT_MFA_NODE_DROPPED" | "ALERT_BALANCE_DEPLETED"
    )
}

/// Cryptographically validates the telemetry payload signature against the provided public key.
pub fn verify_telemetry_signature(payload: &MeshPulsePayload) -> Result<(), &'static str> {
    let pubkey_hex = payload
        .public_key_hex
        .as_ref()
        .ok_or("Missing public_key_hex")?;
    let signature_hex = payload
        .signature_hex
        .as_ref()
        .ok_or("Missing signature_hex")?;

    let pubkey_bytes = hex::decode(pubkey_hex).map_err(|_| "Invalid hex in public_key_hex")?;
    let signature_bytes =
        hex::decode(signature_hex).map_err(|_| "Invalid hex in signature_hex")?;

    let canonical_message = telemetry_canonical_message(payload);

    let mut hasher = Sha256::new();
    hasher.update(canonical_message.as_bytes());
    let hashed_msg = hasher.finalize();

    let secp = Secp256k1::verification_only();
    let message = Message::from_digest_slice(&hashed_msg)
        .map_err(|_| "Failed to instantiate message digest")?;

    let public_key =
        PublicKey::from_slice(&pubkey_bytes).map_err(|_| "Invalid Secp256k1 public key format")?;

    let signature = Signature::from_compact(&signature_bytes)
        .map_err(|_| "Invalid compact signature format (expected 64 bytes)")?;

    secp.verify_ecdsa(&message, &signature, &public_key)
        .map_err(|_| "Cryptographic signature mismatch! Authorization denied.")
}

pub async fn record_alert_dedupe(state: &AppState, key: (u16, u16)) -> bool {
    let mut dedupe = state.alert_dedupe.write().await;
    if !dedupe.insert(key) {
        return false;
    }
    let mut order = state.alert_order.write().await;
    order.push_back(key);
    while order.len() > DEDUPE_CAP {
        if let Some(old) = order.pop_front() {
            dedupe.remove(&old);
        }
    }
    true
}

pub fn validate_route(p: &RouteRequestPayload, max_bound: u16) -> bool {
    p.source >= 1
        && p.destination >= 1
        && p.source <= max_bound
        && p.destination <= max_bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::{mesh_unix_timestamp_secs, telemetry_canonical_message};
    use secp256k1::SecretKey;

    fn sample_nonce_payload(agent_id: u16, nonce: u64) -> MeshPulsePayload {
        MeshPulsePayload {
            agent_id,
            timestamp: 1,
            nonce,
            local_capacity_shannons: 0,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![],
            report_target: agent_id,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        }
    }

    #[test]
    fn test_verify_telemetry_sequence_rejects_replay() {
        let agent_id = 9_001;
        verify_telemetry_sequence(&sample_nonce_payload(agent_id, 10)).expect("first nonce");
        let err = verify_telemetry_sequence(&sample_nonce_payload(agent_id, 10)).unwrap_err();
        assert!(err.to_string().contains("REPLAY FAULT"));
        assert!(verify_telemetry_sequence(&sample_nonce_payload(agent_id, 11)).is_ok());
    }

    #[test]
    fn test_validate_route_accepts_in_bounds_nodes() {
        let payload = RouteRequestPayload {
            source: 1,
            destination: 3,
            amount_shannons: 1000,
            active_network_limit: None,
            execute: None,
            target_asset: None,
            route_metadata: None,
        };
        assert!(validate_route(&payload, 1024));
    }

    #[test]
    fn test_validate_route_rejects_out_of_bounds() {
        let payload = RouteRequestPayload {
            source: 0,
            destination: 3,
            amount_shannons: 1000,
            active_network_limit: None,
            execute: None,
            target_asset: None,
            route_metadata: None,
        };
        assert!(!validate_route(&payload, 1024));
    }

    #[test]
    fn test_verify_telemetry_signature_accepts_valid_payload() {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[7u8; 32]).expect("valid test key");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let payload = MeshPulsePayload {
            agent_id: 44,
            timestamp: mesh_unix_timestamp_secs(),
            nonce: 1,
            local_capacity_shannons: 1_000,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![45],
            report_target: 44,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };

        let canonical = telemetry_canonical_message(&payload);
        let digest = Sha256::digest(canonical.as_bytes());
        let message = Message::from_digest_slice(&digest).unwrap();
        let signature = secp.sign_ecdsa(&message, &secret_key);

        let signed = MeshPulsePayload {
            public_key_hex: Some(hex::encode(public_key.serialize())),
            signature_hex: Some(hex::encode(signature.serialize_compact())),
            ..payload
        };

        assert!(verify_telemetry_signature(&signed).is_ok());
    }

    #[test]
    fn test_verify_telemetry_signature_rejects_tampered_payload() {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[9u8; 32]).expect("valid test key");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let payload = MeshPulsePayload {
            agent_id: 1,
            timestamp: mesh_unix_timestamp_secs(),
            nonce: 2,
            local_capacity_shannons: 500,
            public_key_hex: None,
            signature_hex: None,
            status: "ALERT_MFA_NODE_DROPPED".to_string(),
            active_mesh_neighbors: vec![2],
            report_target: 3,
            attempt: 1,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };

        let canonical = telemetry_canonical_message(&payload);
        let digest = Sha256::digest(canonical.as_bytes());
        let message = Message::from_digest_slice(&digest).unwrap();
        let signature = secp.sign_ecdsa(&message, &secret_key);

        let mut signed = MeshPulsePayload {
            public_key_hex: Some(hex::encode(public_key.serialize())),
            signature_hex: Some(hex::encode(signature.serialize_compact())),
            ..payload
        };
        signed.nonce = 99;

        assert!(verify_telemetry_signature(&signed).is_err());
    }

    #[test]
    fn test_verify_telemetry_timestamp_accepts_fresh_payload() {
        let payload = MeshPulsePayload {
            agent_id: 1,
            timestamp: mesh_unix_timestamp_secs(),
            nonce: 1,
            local_capacity_shannons: 0,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![2],
            report_target: 1,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };
        assert!(verify_telemetry_timestamp(&payload).is_ok());
    }

    #[test]
    fn test_verify_telemetry_timestamp_rejects_stale_payload() {
        let payload = MeshPulsePayload {
            agent_id: 1,
            timestamp: mesh_unix_timestamp_secs().saturating_sub(60),
            nonce: 1,
            local_capacity_shannons: 0,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![2],
            report_target: 1,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        };
        assert!(verify_telemetry_timestamp(&payload).is_err());
    }
}
