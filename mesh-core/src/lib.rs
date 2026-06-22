//! Shared mesh lattice types, topology math, and pubkey utilities for TPXDevs.

pub mod constants;
pub mod error;
pub mod pubkey;
pub mod registry;
pub mod topology;
pub mod types;

pub use constants::*;
pub use error::MeshError;
pub use pubkey::{
    agent_fnn_pubkey, agent_fnn_pubkey_opt, agent_fnn_pubkey_result, dev_agent_signing_key_bytes,
    is_live_fiber_pubkey, normalize_pubkey, normalize_pubkey_hex, peer_id_from_agent_pubkey,
    shannons_to_hex,
};
pub use registry::{merge_registry_json, MeshPubkeyRegistry};
pub use topology::{
    chord_peer, mesh_neighbor_ids, mesh_unix_timestamp_secs, neighbors_canonical, ring_peer,
    skip_peer, telemetry_canonical_message, valid_agent_id,
};
pub use types::{
    MeshPulsePayload, MonitorEnvelope, MonitorEventData, MESH_MONITOR_SCHEMA_VERSION,
};
