//! Asset-aware Dijkstra pathfinding over the live multi-asset mesh graph.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use mesh_core::types::L2Asset;

use crate::graph::{CompleteMeshGraph, LiveMeshEdge};
use crate::plugin_registry::PluginRegistry;

/// Sentinel cost — edge pruned when asset-specific liquidity is insufficient.
pub const INFINITE_EDGE_COST: u64 = u64::MAX;

#[derive(Debug, Clone)]
pub struct RouteQuery {
    pub source: u16,
    pub destination: u16,
    pub required_amount: u64,
    pub target_asset: L2Asset,
    pub network_limit: Option<u16>,
}

#[derive(Clone, Eq, PartialEq)]
struct NodeScore {
    node: u16,
    cost: u64,
}

impl Ord for NodeScore {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for NodeScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Returns outbound liquidity for `target_asset` on edge `(u → v)`.
#[inline]
pub fn edge_asset_capacity(edge: &LiveMeshEdge, target_asset: &L2Asset) -> u64 {
    edge.capacity_for(target_asset)
}

/// Hop weight: latency proxy when liquidity suffices, ∞ otherwise.
#[inline]
pub fn edge_weight(
    edge: &LiveMeshEdge,
    required_amount: u64,
    target_asset: &L2Asset,
    accumulated_latency: u64,
) -> u64 {
    if edge_asset_capacity(edge, target_asset) < required_amount {
        return INFINITE_EDGE_COST;
    }

    let hop_liquidity_premium =
        (required_amount.saturating_mul(edge.fee_proportional)) / 1_000_000;
    let total_fee = edge.fee_base.saturating_add(hop_liquidity_premium);
    accumulated_latency
        .saturating_add(required_amount)
        .saturating_add(total_fee)
}

/// Asset-filtered Dijkstra shortest path.
///
/// Edges lacking `required_amount` of `target_asset` are pruned (weight = ∞).
/// Policy plugins may adjust finite edge weights without awaiting (spread injection).
pub fn find_asset_aware_route(
    graph: &CompleteMeshGraph,
    query: RouteQuery,
    plugins: Option<&PluginRegistry>,
) -> Option<(Vec<u16>, u64)> {
    if query.required_amount == 0 || query.source == query.destination {
        return None;
    }

    let RouteQuery {
        source,
        destination,
        required_amount,
        target_asset,
        network_limit,
    } = query;

    let mut distances = HashMap::new();
    let mut parents = HashMap::new();
    let mut heap = BinaryHeap::new();

    distances.insert(source, 0u64);
    heap.push(NodeScore {
        node: source,
        cost: 0,
    });

    while let Some(NodeScore { node, cost }) = heap.pop() {
        if node == destination {
            let mut path = Vec::new();
            let mut curr = destination;
            while curr != source {
                path.push(curr);
                curr = *parents.get(&curr)?;
            }
            path.push(source);
            path.reverse();
            return Some((path, cost));
        }

        if distances.get(&node).copied().unwrap_or(INFINITE_EDGE_COST) < cost {
            continue;
        }

        let Some(edges) = graph.adjacency_map.get(&node) else {
            continue;
        };

        for edge in edges {
            if !edge.is_active || graph.is_node_offline(edge.peer_id) {
                continue;
            }
            if network_limit.is_some_and(|limit| edge.peer_id > limit) {
                continue;
            }

            let base_cost = edge_weight(edge, required_amount, &target_asset, cost);
            if base_cost == INFINITE_EDGE_COST {
                continue;
            }

            let base_u32 = u32::try_from(base_cost).unwrap_or(u32::MAX);
            let adjusted_u32 = plugins
                .map(|registry| {
                    registry.adjust_edge_weight(node, edge.peer_id, &target_asset, base_u32)
                })
                .unwrap_or(base_u32);
            let next_cost = u64::from(adjusted_u32);
            if next_cost == INFINITE_EDGE_COST {
                continue;
            }

            let current_best = distances
                .get(&edge.peer_id)
                .copied()
                .unwrap_or(INFINITE_EDGE_COST);
            if next_cost < current_best {
                distances.insert(edge.peer_id, next_cost);
                parents.insert(edge.peer_id, node);
                heap.push(NodeScore {
                    node: edge.peer_id,
                    cost: next_cost,
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mesh_core::types::L2Asset;

    use super::*;
    use crate::graph::{CompleteMeshGraph, LiveMeshEdge};
    use crate::plugin_registry::PluginRegistry;

    fn rgb_edge(source_peer: u16, rgb_hash: &str, rgb_amount: u64, ckb_amount: u64) -> LiveMeshEdge {
        let mut caps = HashMap::new();
        caps.insert(L2Asset::CkbNative, ckb_amount);
        caps.insert(
            L2Asset::RgbPlusPlus(rgb_hash.to_string()),
            rgb_amount,
        );
        LiveMeshEdge {
            channel_id: format!("rgb-{source_peer}"),
            peer_id: source_peer,
            peer_pubkey: String::new(),
            capacity_shannons: ckb_amount,
            asset_capacities: caps,
            fee_base: 0,
            fee_proportional: 0,
            is_active: true,
            last_update_timestamp: 0,
        }
    }

    #[test]
    fn asset_aware_route_prunes_insufficient_rgb_liquidity() {
        let mut graph = CompleteMeshGraph::new();
        graph.adjacency_map.insert(
            1,
            vec![rgb_edge(2, "0xstock", 1_000_000, 50_000_000)],
        );

        let ok = find_asset_aware_route(
            &graph,
            RouteQuery {
                source: 1,
                destination: 2,
                required_amount: 500_000,
                target_asset: L2Asset::RgbPlusPlus("0xstock".to_string()),
                network_limit: None,
            },
            None,
        );
        assert!(ok.is_some());

        let fail = find_asset_aware_route(
            &graph,
            RouteQuery {
                source: 1,
                destination: 2,
                required_amount: 2_000_000,
                target_asset: L2Asset::RgbPlusPlus("0xstock".to_string()),
                network_limit: None,
            },
            None,
        );
        assert!(fail.is_none());
    }

    #[test]
    fn asset_aware_route_allows_ckb_when_rgb_depleted() {
        let mut graph = CompleteMeshGraph::new();
        graph.adjacency_map.insert(
            1,
            vec![rgb_edge(2, "0xstock", 0, 50_000_000)],
        );

        let route = find_asset_aware_route(
            &graph,
            RouteQuery {
                source: 1,
                destination: 2,
                required_amount: 1_000_000,
                target_asset: L2Asset::CkbNative,
                network_limit: None,
            },
            None,
        );
        assert!(route.is_some());
    }

    #[test]
    fn asset_aware_route_applies_lume_pricing_spread() {
        let mut graph = CompleteMeshGraph::new();
        graph.adjacency_map.insert(
            1,
            vec![rgb_edge(2, "0xstock", 5_000_000, 50_000_000)],
        );

        let registry = PluginRegistry::bootstrap_default(1_000_000);
        let (_, base_cost) = find_asset_aware_route(
            &graph,
            RouteQuery {
                source: 1,
                destination: 2,
                required_amount: 500_000,
                target_asset: L2Asset::RgbPlusPlus("0xstock".to_string()),
                network_limit: None,
            },
            None,
        )
        .expect("base route");
        let (_, spread_cost) = find_asset_aware_route(
            &graph,
            RouteQuery {
                source: 1,
                destination: 2,
                required_amount: 500_000,
                target_asset: L2Asset::RgbPlusPlus("0xstock".to_string()),
                network_limit: None,
            },
            Some(&registry),
        )
        .expect("spread route");
        assert!(spread_cost > base_cost);
    }
}
