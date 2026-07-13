//! Canonical module catalog — single source for RPC allowlists and profile validation.

/// Every module that may be mounted on a sidecar.
pub const KNOWN_MODULE_IDS: &[&str] = &[
    "dicoba",
    "fiat_bridge",
    "telco_b2c_sweep",
    "lume_yielding",
    "securities_compliance",
    "fiber_agent_swarm",
];

/// Static catalog metadata for the module app store UI.
pub const MODULE_CATALOG: &[(&str, &str, &str)] = &[
    ("dicoba", "DICOBA", "JunguKuu savings vault and micro-contributions"),
    ("fiat_bridge", "FiatBridge", "Mobile-money float bridge and cash-in/out"),
    (
        "telco_b2c_sweep",
        "TelcoB2cSweep",
        "Carrier B2C float sweep automation",
    ),
    (
        "lume_yielding",
        "LumeYielding",
        "RGB++ local order book and yield bids",
    ),
    (
        "securities_compliance",
        "SecuritiesCompliance",
        "RWA DID verification and trade authorization",
    ),
    (
        "fiber_agent_swarm",
        "FiberAgentSwarm",
        "Autonomous market-maker swarm rebalancer",
    ),
];

pub fn normalize_module_id(name: &str) -> Option<&'static str> {
    let key = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if let Some(id) = KNOWN_MODULE_IDS.iter().copied().find(|id| *id == key) {
        return Some(id);
    }
    MODULE_CATALOG.iter().find_map(|(id, label, _)| {
        if label.eq_ignore_ascii_case(name.trim()) {
            Some(*id)
        } else {
            None
        }
    })
}

pub fn catalog_entries() -> Vec<serde_json::Value> {
    MODULE_CATALOG
        .iter()
        .map(|(id, label, description)| {
            serde_json::json!({
                "module_id": id,
                "module_name": label,
                "description": description,
                "rpc_methods": allowed_methods(id).unwrap_or(&[]),
            })
        })
        .collect()
}

/// Allowed RPC methods per module (defense-in-depth with host mount checks).
pub const MODULE_RPC_ALLOWLIST: &[(&str, &[&str])] = &[
    (
        "dicoba",
        &[
            "stream_micro_contribution",
            "get_vault_context",
            "list_vault_contributors",
            "list_member_vaults",
            "stream_weekly_contribution",
            "get_credit_profile",
            "request_loan",
        ],
    ),
    (
        "fiat_bridge",
        &[
            "calculate_invoice_preview",
            "process_cash_in",
            "dispatch_float_crisis_clearing",
        ],
    ),
    (
        "telco_b2c_sweep",
        &["get_float_status", "trigger_manual_sweep"],
    ),
    (
        "lume_yielding",
        &[
            "get_order_book_depth",
            "submit_local_bid",
            "submit_local_ask",
        ],
    ),
    (
        "securities_compliance",
        &["authorize_rwa_trade", "verify_counterparty_did"],
    ),
    (
        "fiber_agent_swarm",
        &["get_swarm_status", "force_rebalance"],
    ),
];

pub fn is_known_module_id(id: &str) -> bool {
    KNOWN_MODULE_IDS.contains(&id)
}

pub fn allowed_methods(module_id: &str) -> Option<&'static [&'static str]> {
    MODULE_RPC_ALLOWLIST
        .iter()
        .find(|(name, _)| *name == module_id)
        .map(|(_, methods)| *methods)
}

pub fn is_allowed_method(module_id: &str, method: &str) -> bool {
    allowed_methods(module_id)
        .map(|methods| methods.contains(&method))
        .unwrap_or(false)
}

/// Peer-message methods permitted in OOB fallback URIs (`fsp://oob?data=…`).
pub const MODULE_OOB_PEER_ALLOWLIST: &[(&str, &[&str])] = &[
    ("dicoba", &["request_guarantor_signature"]),
    (
        "fiat_bridge",
        &["dispatch_float_crisis_clearing", "process_cash_in"],
    ),
    (
        "lume_yielding",
        &["submit_rgb_bid", "submit_rgb_ask", "telemetry_stream"],
    ),
    (
        "securities_compliance",
        &["request_rwa_transfer", "authorize_rwa_trade"],
    ),
];

pub fn is_allowed_oob_peer_method(module_id: &str, method: &str) -> bool {
    MODULE_OOB_PEER_ALLOWLIST
        .iter()
        .find(|(name, _)| *name == module_id)
        .map(|(_, methods)| methods.contains(&method))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_all_known_modules() {
        assert_eq!(KNOWN_MODULE_IDS.len(), MODULE_RPC_ALLOWLIST.len());
        for id in KNOWN_MODULE_IDS {
            assert!(allowed_methods(id).is_some());
        }
    }

    #[test]
    fn unknown_module_has_no_methods() {
        assert!(!is_allowed_method("unknown", "drop_table"));
    }
}
