use std::collections::HashMap;
use std::env;
use std::fs;

use crate::pubkey::{agent_fnn_pubkey, is_live_fiber_pubkey};

/// Maps FA mesh IDs to Fiber secp256k1 pubkeys (JSON env or file).
#[derive(Debug, Clone, Default)]
pub struct MeshPubkeyRegistry {
    map: HashMap<u16, String>,
}

impl MeshPubkeyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_map(map: HashMap<u16, String>) -> Self {
        Self { map }
    }

    pub fn load() -> Self {
        let mut registry = Self::new();
        registry.load_from_env();
        registry
    }

    pub fn load_from_env(&mut self) {
        if let Ok(path) = env::var("MESH_PUBKEY_REGISTRY_PATH") {
            if let Ok(raw) = fs::read_to_string(&path) {
                merge_registry_json(&mut self.map, &raw);
            }
        }
        if let Ok(raw) = env::var("MESH_PUBKEY_REGISTRY") {
            merge_registry_json(&mut self.map, &raw);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get(&self, agent_id: u16) -> Option<&str> {
        self.map.get(&agent_id).map(String::as_str)
    }

    /// Sidecar: registry entry or dev secp fallback.
    pub fn resolve_sidecar(&self, peer_id: u16) -> String {
        if let Some(pk) = self.map.get(&peer_id) {
            if is_live_fiber_pubkey(pk) {
                return pk.clone();
            }
        }
        agent_fnn_pubkey(peer_id)
    }

    /// MFA payment: heartbeat → registry → optional dev secp when sim payments enabled.
    pub fn resolve_for_payment(
        &self,
        agent_id: u16,
        heartbeat: &HashMap<u16, String>,
        sim_payments_enabled: bool,
    ) -> Option<String> {
        if let Some(pk) = heartbeat.get(&agent_id) {
            if is_live_fiber_pubkey(pk) {
                return Some(pk.clone());
            }
        }
        if let Some(pk) = self.map.get(&agent_id) {
            if is_live_fiber_pubkey(pk) {
                return Some(pk.clone());
            }
        }
        if sim_payments_enabled {
            return crate::pubkey::agent_fnn_pubkey_opt(agent_id);
        }
        None
    }
}

pub fn merge_registry_json(map: &mut HashMap<u16, String>, raw: &str) {
    if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(raw) {
        for (k, v) in parsed {
            if k.starts_with('_') {
                continue;
            }
            if let Ok(id) = k.parse::<u16>() {
                if is_live_fiber_pubkey(&v) {
                    map.insert(id, v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ignores_legacy_sim_peer_entries() {
        let pk33 = agent_fnn_pubkey(33);
        let raw = format!(r#"{{"32":"sim-peer-32","33":"{pk33}"}}"#);
        let mut map = HashMap::new();
        merge_registry_json(&mut map, &raw);
        assert!(!map.contains_key(&32));
        assert_eq!(map.get(&33), Some(&pk33));

        let reg = MeshPubkeyRegistry::from_map(map);
        assert_eq!(reg.resolve_sidecar(32), agent_fnn_pubkey(32));
        assert_ne!(reg.resolve_sidecar(32), "sim-peer-32");
    }

    #[test]
    fn resolve_for_payment_uses_dev_secp_pubkey() {
        let registry = MeshPubkeyRegistry::new();
        let heartbeat = HashMap::new();
        let pk = registry
            .resolve_for_payment(32, &heartbeat, true)
            .expect("dev secp pubkey");
        assert!(is_live_fiber_pubkey(&pk));
        assert!(pk.len() >= 66);
    }

    #[test]
    fn resolve_for_payment_prefers_heartbeat() {
        let live =
            "028012345678901234567890123456789012345678901234567890123456789012".to_string();
        let registry = MeshPubkeyRegistry::new();
        let heartbeat = [(32u16, live.clone())].into_iter().collect();
        let pk = registry
            .resolve_for_payment(32, &heartbeat, false)
            .expect("heartbeat pk");
        assert_eq!(pk, live);
    }
}
