//! Peer module packet signing and verification (mesh P2P + OOB fallback).

use mesh_core::network::PeerModulePacket;
use mesh_core::MeshPubkeyRegistry;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

pub fn sign_peer_module_packet(
    mut packet: PeerModulePacket,
    secret_key: &SecretKey,
) -> Result<PeerModulePacket, String> {
    let secp = Secp256k1::signing_only();
    let signing_bytes = packet.signing_bytes()?;

    let mut hasher = Sha256::new();
    hasher.update(&signing_bytes);
    let digest = hasher.finalize();

    let message = Message::from_digest_slice(&digest)
        .map_err(|e| format!("signing digest invalid: {e}"))?;
    let signature = secp.sign_ecdsa(&message, secret_key);

    packet.signature = Some(hex::encode(signature.serialize_compact()));
    Ok(packet)
}

pub fn verify_peer_module_packet_signature(packet: &PeerModulePacket) -> Result<(), String> {
    let signature_hex = packet
        .signature
        .as_deref()
        .ok_or_else(|| "peer packet missing signature".to_string())?;

    let registry = MeshPubkeyRegistry::load();
    let pubkey_hex = registry.resolve_sidecar(packet.source_agent_id);
    let pubkey_bytes = hex::decode(pubkey_hex.trim())
        .map_err(|e| format!("source FA-{} pubkey decode failed: {e}", packet.source_agent_id))?;

    let public_key = PublicKey::from_slice(&pubkey_bytes)
        .map_err(|e| format!("source FA-{} pubkey invalid: {e}", packet.source_agent_id))?;

    let signature_bytes = hex::decode(signature_hex.trim())
        .map_err(|e| format!("signature hex decode failed: {e}"))?;
    let signature = Signature::from_compact(&signature_bytes)
        .map_err(|e| format!("signature compact decode failed: {e}"))?;

    let signing_bytes = packet.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(&signing_bytes);
    let digest = hasher.finalize();

    let message = Message::from_digest_slice(&digest)
        .map_err(|e| format!("verification digest invalid: {e}"))?;

    let secp = Secp256k1::verification_only();
    secp.verify_ecdsa(&message, &signature, &public_key)
        .map_err(|e| format!("peer packet signature rejected: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::dev_agent_signing_key_bytes;
    use serde_json::json;
    use secp256k1::SecretKey;

    #[test]
    fn signed_peer_packet_verifies() {
        let secret = SecretKey::from_slice(&dev_agent_signing_key_bytes(44))
            .expect("dev key");
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 12,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({ "loan_id": "loan-9f3c" }),
            signature: None,
        };

        let signed = sign_peer_module_packet(packet, &secret).expect("sign");
        verify_peer_module_packet_signature(&signed).expect("verify");
    }

    #[test]
    fn signed_packet_survives_oob_uri_round_trip() {
        let secret = SecretKey::from_slice(&dev_agent_signing_key_bytes(44)).expect("dev key");
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 45,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({
                "loan_id": "loan-local",
                "guarantor_member_id": "37708400-64a4-52f6-8d4b-9e3d454c9003",
                "principal_shannons": 1_900_000u64
            }),
            signature: None,
        };

        let signed = sign_peer_module_packet(packet, &secret).expect("sign");
        let uri = signed.to_fallback_uri().expect("encode uri");
        let decoded = PeerModulePacket::from_fallback_uri(&uri).expect("decode uri");

        verify_peer_module_packet_signature(&decoded).expect("verify after round trip");
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let secret = SecretKey::from_slice(&dev_agent_signing_key_bytes(44)).expect("dev key");
        let packet = PeerModulePacket {
            source_agent_id: 44,
            target_agent_id: 45,
            target_module: "dicoba".to_string(),
            method: "request_guarantor_signature".to_string(),
            payload: json!({ "principal_shannons": 1_000u64 }),
            signature: None,
        };

        let mut signed = sign_peer_module_packet(packet, &secret).expect("sign");
        signed.payload = json!({ "principal_shannons": 9_999_999u64 });

        assert!(verify_peer_module_packet_signature(&signed).is_err());
    }
}
