//! Agent signing keys and telemetry signatures.

use std::env;

use mesh_core::error::MeshError;
use mesh_core::{dev_agent_signing_key_bytes, telemetry_canonical_message, MeshPulsePayload};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

pub fn load_sidecar_identity_key() -> Result<SecretKey, MeshError> {
    if let Ok(key_hex) = env::var("FSP_AGENT_SECRET_KEY")
        .or_else(|_| env::var("LUME_AGENT_SECRET_KEY"))
    {
        let key_bytes = hex::decode(key_hex.trim()).map_err(|e| {
            MeshError::InvalidPayload(format!("Invalid hex layout: {e}"))
        })?;
        return SecretKey::from_slice(&key_bytes).map_err(|e| {
            MeshError::InvalidPayload(format!("Cryptographic structure failure: {e}"))
        });
    }

    if cfg!(debug_assertions) {
        println!(
            "⚠️ [SECURITY WARNING] Using localized fallback keys for local sandbox routing."
        );
        let fallback_bytes = dev_agent_signing_key_bytes(1);
        return SecretKey::from_slice(&fallback_bytes)
            .map_err(|e| MeshError::InvalidPayload(e.to_string()));
    }

    Err(MeshError::InvalidPayload(
        "Production identity variable FSP_AGENT_SECRET_KEY missing.".to_string(),
    ))
}

pub fn resolve_agent_signing_key(agent_id: u16) -> Result<[u8; 32], String> {
    if let Ok(hex_key) = env::var("FIBER_AGENT_SECRET_KEY_HEX") {
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

    if env::var("FSP_AGENT_SECRET_KEY")
        .or_else(|_| env::var("LUME_AGENT_SECRET_KEY"))
        .is_ok()
    {
        let key = load_sidecar_identity_key().map_err(|e| e.to_string())?;
        return Ok(key.secret_bytes());
    }

    if env::var("FIBER_AGENT_ALLOW_DEV_KEYS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(cfg!(debug_assertions))
    {
        return Ok(dev_agent_signing_key_bytes(agent_id));
    }

    let key = load_sidecar_identity_key().map_err(|e| e.to_string())?;
    Ok(key.secret_bytes())
}

pub fn resolve_agent_secret_key(agent_id: u16) -> Result<SecretKey, String> {
    if env::var("FIBER_AGENT_SECRET_KEY_HEX").is_ok()
        || env::var("FSP_AGENT_SECRET_KEY")
            .or_else(|_| env::var("LUME_AGENT_SECRET_KEY"))
            .is_ok()
    {
        let key_bytes = resolve_agent_signing_key(agent_id)?;
        return SecretKey::from_slice(&key_bytes).map_err(|e| format!("Invalid secret key: {e}"));
    }

    if env::var("FIBER_AGENT_ALLOW_DEV_KEYS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(cfg!(debug_assertions))
    {
        let key_bytes = dev_agent_signing_key_bytes(agent_id);
        return SecretKey::from_slice(&key_bytes).map_err(|e| format!("Invalid secret key: {e}"));
    }

    load_sidecar_identity_key().map_err(|e| e.to_string())
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
