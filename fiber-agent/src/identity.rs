//! Agent signing keys and telemetry signatures.

use mesh_core::{dev_agent_signing_key_bytes, telemetry_canonical_message, MeshPulsePayload};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

pub fn resolve_agent_signing_key(agent_id: u16) -> Result<[u8; 32], String> {
    if let Ok(hex_key) = std::env::var("FIBER_AGENT_SECRET_KEY_HEX") {
        let bytes = hex::decode(hex_key.trim())
            .map_err(|e| format!("FIBER_AGENT_SECRET_KEY_HEX decode failed: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!(
                "FIBER_AGENT_SECRET_KEY_HEX must be 32 bytes, got {}",
                bytes.len()
            ));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    if std::env::var("FIBER_AGENT_ALLOW_DEV_KEYS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
    {
        return Ok(dev_agent_signing_key_bytes(agent_id));
    }

    Err("FIBER_AGENT_SECRET_KEY_HEX is required (set FIBER_AGENT_ALLOW_DEV_KEYS=true for local dev keys)".into())
}

pub fn resolve_agent_secret_key(agent_id: u16) -> Result<SecretKey, String> {
    let key_bytes = resolve_agent_signing_key(agent_id)?;
    SecretKey::from_slice(&key_bytes).map_err(|e| format!("Invalid secret key: {e}"))
}

fn sign_telemetry_payload_with_neighbors(
    payload: &MeshPulsePayload,
    secret_key: &SecretKey,
) -> (String, String) {
    let secp = Secp256k1::signing_only();
    let canonical_message = telemetry_canonical_message(payload);

    let mut hasher = Sha256::new();
    hasher.update(canonical_message.as_bytes());
    let hashed_msg = hasher.finalize();

    let message = Message::from_digest_slice(&hashed_msg)
        .expect("SHA-256 digest is always 32 bytes");

    let signature = secp.sign_ecdsa(&message, secret_key);
    let pubkey = PublicKey::from_secret_key(&secp, secret_key);

    (
        hex::encode(pubkey.serialize()),
        hex::encode(signature.serialize_compact()),
    )
}

pub fn attach_telemetry_signature(
    mut payload: MeshPulsePayload,
    secret_key: &SecretKey,
) -> MeshPulsePayload {
    let (public_key_hex, signature_hex) =
        sign_telemetry_payload_with_neighbors(&payload, secret_key);
    payload.public_key_hex = Some(public_key_hex);
    payload.signature_hex = Some(signature_hex);
    payload
}
