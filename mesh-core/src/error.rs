use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MeshError {
    #[error("agent id out of range: {0} (must be 1..={RING_SIZE})")]
    AgentIdOutOfRange(u16),
    #[error("invalid secret key: {0}")]
    InvalidSecretKey(String),
    #[error("secret key hex decode failed: {0}")]
    SecretKeyHexDecode(String),
    #[error("FIBER_AGENT_SECRET_KEY_HEX must be 32 bytes, got {0}")]
    SecretKeyWrongLength(usize),
    #[error("FIBER_AGENT_SECRET_KEY_HEX is required (set FIBER_AGENT_ALLOW_DEV_KEYS=true for local dev keys)")]
    SecretKeyRequired,
    #[error("invalid telemetry payload: {0}")]
    InvalidPayload(String),
    #[error("network error: {0}")]
    NetworkError(String),
}

use crate::constants::RING_SIZE;
