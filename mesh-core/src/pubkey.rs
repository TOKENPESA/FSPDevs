use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::constants::{DEV_KEY_MARKER_BYTE, RING_SIZE};
use crate::error::MeshError;

pub fn shannons_to_hex(amount: u64) -> String {
    format!("0x{amount:x}")
}

pub fn is_live_fiber_pubkey(pubkey: &str) -> bool {
    let key = pubkey.strip_prefix("0x").unwrap_or(pubkey);
    key.len() >= 66 && key.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn normalize_pubkey_hex(pubkey: &str) -> String {
    pubkey
        .trim()
        .strip_prefix("0x")
        .unwrap_or(pubkey)
        .to_ascii_lowercase()
}

/// Normalize pubkey for hub peer comparison (MFA hub RPC).
pub fn normalize_pubkey(pubkey: &str) -> String {
    normalize_pubkey_hex(pubkey)
}

/// Deterministic dev secret key bytes for FA-{id}.
pub fn dev_agent_signing_key_bytes(agent_id: u16) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[0] = (agent_id >> 8) as u8;
    key[1] = agent_id as u8;
    key[31] = DEV_KEY_MARKER_BYTE;
    key
}

pub fn agent_fnn_pubkey_result(agent_id: u16) -> Result<String, MeshError> {
    let key_bytes = dev_agent_signing_key_bytes(agent_id);
    let secret = SecretKey::from_slice(&key_bytes)
        .map_err(|e| MeshError::InvalidSecretKey(e.to_string()))?;
    let secp = Secp256k1::signing_only();
    let pubkey = PublicKey::from_secret_key(&secp, &secret);
    Ok(hex::encode(pubkey.serialize()))
}

/// Deterministic compressed secp256k1 Fiber node pubkey for FA-{id} (dev key scheme).
pub fn agent_fnn_pubkey(agent_id: u16) -> String {
    agent_fnn_pubkey_result(agent_id).unwrap_or_default()
}

/// MFA variant — returns `None` on failure instead of empty string.
pub fn agent_fnn_pubkey_opt(agent_id: u16) -> Option<String> {
    agent_fnn_pubkey_result(agent_id).ok()
}

/// Resolve FA id from a dev-derived FNN pubkey (or legacy `sim-peer-{id}`).
pub fn peer_id_from_agent_pubkey(peer_public_key: &str) -> Option<u16> {
    if let Some(id) = peer_public_key
        .strip_prefix("sim-peer-")
        .and_then(|s| s.parse().ok())
    {
        return Some(id);
    }

    let normalized = normalize_pubkey_hex(peer_public_key);
    for id in 1..=RING_SIZE {
        if let Ok(pk) = agent_fnn_pubkey_result(id) {
            if normalize_pubkey_hex(&pk) == normalized {
                return Some(id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_fnn_pubkey_is_secp256k1_hex() {
        let pk = agent_fnn_pubkey(44);
        assert!(pk.len() >= 66);
        assert!(pk.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(peer_id_from_agent_pubkey(&pk), Some(44));
    }

    #[test]
    fn peer_id_from_agent_pubkey_supports_legacy_sim_peer() {
        assert_eq!(peer_id_from_agent_pubkey("sim-peer-44"), Some(44));
    }

    #[test]
    fn payment_pubkey_accepts_live_secp256k1() {
        assert!(is_live_fiber_pubkey(
            "028012345678901234567890123456789012345678901234567890123456789012"
        ));
        assert!(!is_live_fiber_pubkey("sim-peer-32"));
        assert!(!is_live_fiber_pubkey("not-a-key"));
    }

    #[test]
    fn normalize_pubkey_strips_prefix() {
        assert_eq!(
            normalize_pubkey("0xAbC123"),
            normalize_pubkey("abc123")
        );
    }
}
