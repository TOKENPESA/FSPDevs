//! Native MFA plugin catalog metadata.

pub const KNOWN_PLUGIN_IDS: &[&str] = &[
    "lume_pricing",
    "sovereign_compliance",
    "automated_refueling",
    "clearinghouse_swap",
];

pub const PLUGIN_CATALOG: &[(&str, &str, &str, &str)] = &[
    (
        "lume_pricing",
        "LumePricing",
        "policy",
        "RGB++/xUDT spread adjustments on routing edge weights",
    ),
    (
        "sovereign_compliance",
        "SovereignCompliance",
        "policy",
        "RWA DID clearance gate before route approval",
    ),
    (
        "automated_refueling",
        "AutomatedRefueling",
        "policy",
        "Treasury copilot heartbeat refuel suggestions",
    ),
    (
        "clearinghouse_swap",
        "ClearinghouseSwap",
        "clearing",
        "Regional float crisis + multi-asset cross-clearing",
    ),
];

pub fn normalize_plugin_id(name: &str) -> Option<&'static str> {
    let key = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if let Some(id) = KNOWN_PLUGIN_IDS.iter().copied().find(|id| *id == key) {
        return Some(id);
    }
    PLUGIN_CATALOG.iter().find_map(|(id, label, _, _)| {
        if label.eq_ignore_ascii_case(name.trim()) {
            Some(*id)
        } else {
            None
        }
    })
}

pub fn catalog_entries() -> Vec<serde_json::Value> {
    PLUGIN_CATALOG
        .iter()
        .map(|(id, label, kind, description)| {
            serde_json::json!({
                "module_id": id,
                "module_name": label,
                "kind": kind,
                "description": description,
            })
        })
        .collect()
}
