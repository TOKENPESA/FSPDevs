use mesh_core::types::{AssetCapacity, L2Asset};
use mesh_core::{chord_peer, valid_agent_id, CHANNEL_LIQUIDITY, MeshError, RING_SIZE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

/// Per-edge liquidity partitioned by exact `L2Asset` identity (CKB, RGB++, xUDT).
pub type AssetCapacityMap = HashMap<L2Asset, u64>;

// ================================================================================
// 1. FIBER GOSSIP NETWORK DTOs (Data Transfer Objects)
// ================================================================================

#[allow(dead_code)] // Public API for upcoming FNN gossip sync handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnnNodeAnnouncement {
    pub node_pubkey: String,
    pub alias: String,
    pub addresses: Vec<String>,
    pub timestamp: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnnChannelAnnouncement {
    pub channel_id: String,
    pub node_1_pubkey: String,
    pub node_2_pubkey: String,
    pub total_capacity_shannons: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnnChannelUpdate {
    pub channel_id: String,
    pub target_node_pubkey: String,
    pub fee_base_shannons: u64,
    pub fee_proportional_millionths: u64,
    pub is_enabled: bool,
    pub local_balance_shannons: u64,
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_balances: Option<Vec<AssetCapacity>>,
}

// ================================================================================
// 2. LIVE NETWORK GRAPH LAYOUT DEFINITION
// ================================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveMeshEdge {
    pub channel_id: String,
    pub peer_id: u16,
    pub peer_pubkey: String,
    /// CKB-native scalar mirror for legacy telemetry and fee routing.
    pub capacity_shannons: u64,
    /// Multi-indexed outbound liquidity keyed by exact `L2Asset` variant.
    pub asset_capacities: AssetCapacityMap,
    pub fee_base: u64,
    pub fee_proportional: u64,
    pub is_active: bool,
    pub last_update_timestamp: u64,
}

impl LiveMeshEdge {
    pub fn capacity_for(&self, asset: &L2Asset) -> u64 {
        self.asset_capacities
            .get(asset)
            .copied()
            .unwrap_or({
                if matches!(asset, L2Asset::CkbNative) {
                    self.capacity_shannons
                } else {
                    0
                }
            })
    }

    pub fn set_asset_capacity(&mut self, asset: L2Asset, amount: u64) {
        if amount == 0 {
            self.asset_capacities.remove(&asset);
        } else {
            self.asset_capacities.insert(asset.clone(), amount);
        }
        if matches!(asset, L2Asset::CkbNative) {
            self.capacity_shannons = amount;
        }
    }

    pub fn apply_capacity_snapshot(&mut self, capacities: &[AssetCapacity]) {
        for cap in capacities {
            self.set_asset_capacity(cap.asset.clone(), cap.amount_atomic);
        }
    }
}

pub struct CompleteMeshGraph {
    pub adjacency_map: HashMap<u16, Vec<LiveMeshEdge>>,
    pub pubkey_to_agent_id: HashMap<String, u16>,
    pub agent_id_to_pubkey: HashMap<u16, String>,
    pub topology_version: AtomicU64,
    pub known_channels: HashMap<String, (String, String, u64)>,
    /// Agents marked offline by healing / fault isolation.
    offline_registry: HashSet<u16>,
}

impl Default for CompleteMeshGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CompleteMeshGraph {
    /// Empty graph for live FNN gossip ingestion (no synthetic lattice).
    pub fn new() -> Self {
        Self {
            adjacency_map: HashMap::new(),
            pubkey_to_agent_id: HashMap::new(),
            agent_id_to_pubkey: HashMap::new(),
            topology_version: AtomicU64::new(0),
            known_channels: HashMap::new(),
            offline_registry: HashSet::new(),
        }
    }

    /// Simulation lattice (ring + skip + chord) for FA 1..=total_nodes.
    pub fn with_lattice(total_nodes: u16) -> Self {
        let mut graph = Self::new();
        graph.seed_lattice_topology(total_nodes);
        graph
    }

    fn seed_lattice_topology(&mut self, total_nodes: u16) {
        for i in 1..=total_nodes {
            let ring = if i == total_nodes { 1 } else { i + 1 };
            let skip = if i >= total_nodes.saturating_sub(1) {
                1
            } else {
                i + 2
            };
            let chord = chord_peer(i, total_nodes);
            self.adjacency_map.insert(
                i,
                vec![
                    sim_edge(i, ring),
                    sim_edge(i, skip),
                    sim_edge(i, chord),
                ],
            );
        }
    }

    pub fn is_node_offline(&self, agent: u16) -> bool {
        self.offline_registry.contains(&agent)
    }

    #[inline]
    pub fn bump_topology_version(&self) {
        self.topology_version
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    pub fn get_version(&self) -> u64 {
        self.topology_version.load(AtomicOrdering::Relaxed)
    }

    pub fn isolate_faulty_node(&mut self, dead_node: u16) {
        self.offline_registry.insert(dead_node);
        for channels in self.adjacency_map.values_mut() {
            for channel in channels.iter_mut() {
                if channel.peer_id == dead_node {
                    channel.is_active = false;
                }
            }
        }
        self.bump_topology_version();
    }

    pub fn pick_healing_peer(&self, agent: u16, dead: u16) -> u16 {
        if let Some(channels) = self.adjacency_map.get(&agent) {
            for ch in channels {
                if ch.is_active
                    && ch.peer_id != dead
                    && !self.offline_registry.contains(&ch.peer_id)
                {
                    return ch.peer_id;
                }
            }
        }
        (dead + 3) % RING_SIZE + 1
    }

    pub fn apply_heartbeat_liveness(&mut self, agent: u16, live_neighbors: &[u16]) {
        if !valid_agent_id(agent) {
            return;
        }

        self.offline_registry.remove(&agent);
        let live: HashSet<u16> = live_neighbors.iter().copied().collect();
        if let Some(edges) = self.adjacency_map.get_mut(&agent) {
            for edge in edges.iter_mut() {
                if live.contains(&edge.peer_id) {
                    edge.is_active = true;
                }
            }
        }
        self.bump_topology_version();
    }

    /// Mirrors live outbound CKB liquidity from sidecar heartbeats into routing edges.
    pub fn apply_heartbeat_liquidity(&mut self, agent: u16, outbound_shannons: u64) {
        if !valid_agent_id(agent) || outbound_shannons == 0 {
            return;
        }
        if let Some(edges) = self.adjacency_map.get_mut(&agent) {
            for edge in edges.iter_mut().filter(|e| e.is_active) {
                edge.set_asset_capacity(L2Asset::CkbNative, outbound_shannons);
            }
        }
        self.bump_topology_version();
    }

    /// Updates per-asset outbound balances from multi-asset heartbeat telemetry.
    pub fn apply_heartbeat_multi_asset_liquidity(
        &mut self,
        agent: u16,
        capacities: &[AssetCapacity],
    ) {
        if !valid_agent_id(agent) || capacities.is_empty() {
            return;
        }
        if let Some(edges) = self.adjacency_map.get_mut(&agent) {
            for edge in edges.iter_mut().filter(|e| e.is_active) {
                edge.apply_capacity_snapshot(capacities);
            }
        }
        self.bump_topology_version();
    }

    /// Applies post-settlement liquidity pulse for a single asset on a directed edge.
    pub fn apply_settlement_asset_pulse(
        &mut self,
        source: u16,
        peer_id: u16,
        asset: L2Asset,
        amount_atomic: u64,
    ) {
        if !valid_agent_id(source) {
            return;
        }
        if let Some(edges) = self.adjacency_map.get_mut(&source) {
            for edge in edges
                .iter_mut()
                .filter(|e| e.is_active && e.peer_id == peer_id)
            {
                edge.set_asset_capacity(asset.clone(), amount_atomic);
            }
        }
        self.bump_topology_version();
    }

    /// Minimum active outbound capacity for an agent and specific asset.
    pub fn min_active_outbound_capacity_for(&self, agent: u16, asset: &L2Asset) -> Option<u64> {
        self.adjacency_map.get(&agent).map(|edges| {
            edges
                .iter()
                .filter(|e| e.is_active)
                .map(|e| e.capacity_for(asset))
                .min()
                .unwrap_or(0)
        })
    }

    /// Legacy scalar floor — CKB-native minimum across active edges.
    pub fn min_active_outbound_capacity(&self, agent: u16) -> Option<u64> {
        self.min_active_outbound_capacity_for(agent, &L2Asset::CkbNative)
    }

    /// Legacy route API — CKB-native asset routing via asset-aware engine.
    pub fn compute_multi_hop_route(
        &self,
        start: u16,
        end: u16,
        amount: u64,
        network_limit: u16,
    ) -> Option<Vec<u16>> {
        self.find_route(start, end, amount, Some(network_limit))
            .map(|(path, _)| path)
    }

    /// Asset-aware multi-hop route for RGB++, xUDT, or CKB transfers.
    pub fn compute_asset_route(
        &self,
        start: u16,
        end: u16,
        amount: u64,
        target_asset: L2Asset,
        network_limit: Option<u16>,
        plugins: Option<&crate::plugin_registry::PluginRegistry>,
    ) -> Option<(Vec<u16>, u64)> {
        if start == 0 || end == 0 || amount == 0 {
            return None;
        }
        if let Some(limit) = network_limit {
            if start > limit || end > limit {
                return None;
            }
        }
        crate::routing::find_asset_aware_route(
            self,
            crate::routing::RouteQuery {
                source: start,
                destination: end,
                required_amount: amount,
                target_asset,
                network_limit,
            },
            plugins,
        )
    }
}

fn sim_edge(source: u16, peer: u16) -> LiveMeshEdge {
    let mut asset_capacities = AssetCapacityMap::new();
    asset_capacities.insert(L2Asset::CkbNative, CHANNEL_LIQUIDITY);
    LiveMeshEdge {
        channel_id: format!("sim-{source}-{peer}"),
        peer_id: peer,
        peer_pubkey: String::new(),
        capacity_shannons: CHANNEL_LIQUIDITY,
        asset_capacities,
        fee_base: 0,
        fee_proportional: 0,
        is_active: true,
        last_update_timestamp: 0,
    }
}

// ================================================================================
// 3. GOSSIP TELEMETRY INGESTION ENGINE
// ================================================================================

#[allow(dead_code)]
impl CompleteMeshGraph {
    pub fn register_gossip_node(&mut self, node: FnnNodeAnnouncement, assigned_agent_id: u16) {
        self.pubkey_to_agent_id
            .insert(node.node_pubkey.clone(), assigned_agent_id);
        self.agent_id_to_pubkey
            .insert(assigned_agent_id, node.node_pubkey);
        self.bump_topology_version();
    }

    pub fn ingest_channel_announcement(
        &mut self,
        ann: FnnChannelAnnouncement,
    ) -> Result<(), MeshError> {
        self.known_channels.insert(
            ann.channel_id.clone(),
            (
                ann.node_1_pubkey.clone(),
                ann.node_2_pubkey.clone(),
                ann.total_capacity_shannons,
            ),
        );
        self.bump_topology_version();
        Ok(())
    }
}

impl CompleteMeshGraph {
    pub fn ingest_channel_update(&mut self, update: FnnChannelUpdate) -> Result<(), MeshError> {
        let (node_1, node_2, _total_cap) = self
            .known_channels
            .get(&update.channel_id)
            .ok_or_else(|| {
                MeshError::InvalidPayload(format!("Unknown channel_id: {}", update.channel_id))
            })?
            .clone();

        let source_pubkey = &update.target_node_pubkey;
        let peer_pubkey = if source_pubkey == &node_1 {
            node_2
        } else {
            node_1
        };

        let source_id = *self.pubkey_to_agent_id.get(source_pubkey).ok_or_else(|| {
            MeshError::InvalidPayload(format!("Source pubkey unmapped: {source_pubkey}"))
        })?;
        let peer_id = *self.pubkey_to_agent_id.get(&peer_pubkey).ok_or_else(|| {
            MeshError::InvalidPayload(format!("Peer pubkey unmapped: {peer_pubkey}"))
        })?;

        let edges = self.adjacency_map.entry(source_id).or_default();

        if let Some(edge) = edges
            .iter_mut()
            .find(|e| e.channel_id == update.channel_id)
        {
            if update.timestamp >= edge.last_update_timestamp {
                edge.set_asset_capacity(L2Asset::CkbNative, update.local_balance_shannons);
                if let Some(ref balances) = update.asset_balances {
                    edge.apply_capacity_snapshot(balances);
                }
                edge.fee_base = update.fee_base_shannons;
                edge.fee_proportional = update.fee_proportional_millionths;
                edge.is_active = update.is_enabled;
                edge.peer_id = peer_id;
                edge.peer_pubkey = peer_pubkey.clone();
                edge.last_update_timestamp = update.timestamp;
            }
        } else {
            let mut asset_capacities = AssetCapacityMap::new();
            asset_capacities.insert(L2Asset::CkbNative, update.local_balance_shannons);
            if let Some(ref balances) = update.asset_balances {
                for cap in balances {
                    asset_capacities.insert(cap.asset.clone(), cap.amount_atomic);
                }
            }
            edges.push(LiveMeshEdge {
                channel_id: update.channel_id,
                peer_id,
                peer_pubkey,
                capacity_shannons: update.local_balance_shannons,
                asset_capacities,
                fee_base: update.fee_base_shannons,
                fee_proportional: update.fee_proportional_millionths,
                is_active: update.is_enabled,
                last_update_timestamp: update.timestamp,
            });
        }

        self.bump_topology_version();
        Ok(())
    }
}

// ================================================================================
// 4. LIVE CAPACITY & FEE-AWARE ROUTING (delegates to asset-aware engine)
// ================================================================================

impl CompleteMeshGraph {
    /// CKB-native shortest path — backward-compatible wrapper.
    pub fn find_route(
        &self,
        source: u16,
        destination: u16,
        amount_shannons: u64,
        network_limit: Option<u16>,
    ) -> Option<(Vec<u16>, u64)> {
        self.compute_asset_route(
            source,
            destination,
            amount_shannons,
            L2Asset::CkbNative,
            network_limit,
            None,
        )
    }
}

/// Serializable mesh graph snapshot (hot-path graph uses atomics that cannot be serde'd directly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub adjacency_map: HashMap<u16, Vec<LiveMeshEdge>>,
    pub pubkey_to_agent_id: HashMap<String, u16>,
    pub agent_id_to_pubkey: HashMap<u16, String>,
    pub topology_version: u64,
    pub known_channels: HashMap<String, (String, String, u64)>,
    pub offline_agents: Vec<u16>,
}

impl CompleteMeshGraph {
    pub fn to_snapshot(&self) -> GraphSnapshot {
        GraphSnapshot {
            adjacency_map: self.adjacency_map.clone(),
            pubkey_to_agent_id: self.pubkey_to_agent_id.clone(),
            agent_id_to_pubkey: self.agent_id_to_pubkey.clone(),
            topology_version: self.get_version(),
            known_channels: self.known_channels.clone(),
            offline_agents: self.offline_registry.iter().copied().collect(),
        }
    }

    pub fn from_snapshot(snapshot: GraphSnapshot) -> Self {
        Self {
            adjacency_map: snapshot.adjacency_map,
            pubkey_to_agent_id: snapshot.pubkey_to_agent_id,
            agent_id_to_pubkey: snapshot.agent_id_to_pubkey,
            topology_version: AtomicU64::new(snapshot.topology_version),
            known_channels: snapshot.known_channels,
            offline_registry: snapshot.offline_agents.into_iter().collect(),
        }
    }

    pub fn restore_from_snapshot(&mut self, snapshot: GraphSnapshot) {
        *self = Self::from_snapshot(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dijkstra_finds_optimal_path() {
        let graph = CompleteMeshGraph::with_lattice(1024);
        let path = graph.compute_multi_hop_route(1, 3, 1000, 1024).unwrap();
        assert_eq!(path, vec![1, 3]);
    }

    #[test]
    fn test_routing_avoids_isolated_nodes() {
        let mut graph = CompleteMeshGraph::with_lattice(1024);
        graph.isolate_faulty_node(3);
        let path = graph.compute_multi_hop_route(1, 3, 1000, 1024);
        assert!(path.is_none() || !path.unwrap().contains(&3));
    }

    #[test]
    fn test_heartbeat_reactivates_reported_neighbors() {
        let mut graph = CompleteMeshGraph::with_lattice(1024);
        graph.isolate_faulty_node(45);
        graph.apply_heartbeat_liveness(44, &[45]);

        let edge = graph
            .adjacency_map
            .get(&44)
            .and_then(|edges| edges.iter().find(|e| e.peer_id == 45))
            .expect("ring edge FA-44 → FA-45");
        assert!(edge.is_active);
        assert!(!graph.offline_registry.contains(&44));
    }

    #[test]
    fn test_gossip_channel_update_maps_pubkeys_to_agents() {
        let mut graph = CompleteMeshGraph::with_lattice(8);
        graph.register_gossip_node(
            FnnNodeAnnouncement {
                node_pubkey: "pk-a".into(),
                alias: "FA-1".into(),
                addresses: vec![],
                timestamp: 1,
            },
            1,
        );
        graph.register_gossip_node(
            FnnNodeAnnouncement {
                node_pubkey: "pk-b".into(),
                alias: "FA-2".into(),
                addresses: vec![],
                timestamp: 1,
            },
            2,
        );

        graph
            .ingest_channel_announcement(FnnChannelAnnouncement {
                channel_id: "chan-1".into(),
                node_1_pubkey: "pk-a".into(),
                node_2_pubkey: "pk-b".into(),
                total_capacity_shannons: 1_000_000,
            })
            .expect("announce channel");

        let version_before = graph.get_version();
        graph
            .ingest_channel_update(FnnChannelUpdate {
                channel_id: "chan-1".into(),
                target_node_pubkey: "pk-a".into(),
                fee_base_shannons: 10,
                fee_proportional_millionths: 100,
                is_enabled: true,
                local_balance_shannons: 500_000,
                timestamp: 42,
                asset_balances: None,
            })
            .expect("apply update");

        assert!(graph.get_version() > version_before);
        let edge = graph
            .adjacency_map
            .get(&1)
            .and_then(|edges| edges.iter().find(|e| e.channel_id == "chan-1"))
            .expect("live edge");
        assert_eq!(edge.peer_id, 2);
        assert_eq!(edge.capacity_shannons, 500_000);
    }

    #[test]
    fn test_find_route_includes_fee_in_total_cost() {
        let mut graph = CompleteMeshGraph::with_lattice(4);
        graph.adjacency_map.insert(
            1,
            vec![LiveMeshEdge {
                channel_id: "fee-edge".into(),
                peer_id: 2,
                peer_pubkey: String::new(),
                capacity_shannons: CHANNEL_LIQUIDITY,
                asset_capacities: {
                    let mut m = AssetCapacityMap::new();
                    m.insert(L2Asset::CkbNative, CHANNEL_LIQUIDITY);
                    m
                },
                fee_base: 100,
                fee_proportional: 0,
                is_active: true,
                last_update_timestamp: 0,
            }],
        );

        let (_, cost) = graph.find_route(1, 2, 1000, None).expect("route");
        assert_eq!(cost, 1100);
    }

    #[test]
    fn test_live_gossip_ingestion_and_routing() {
        let mut graph = CompleteMeshGraph::new();

        graph.register_gossip_node(
            FnnNodeAnnouncement {
                node_pubkey: "03aaa".to_string(),
                alias: "FA-1".to_string(),
                addresses: vec![],
                timestamp: 100,
            },
            1,
        );

        graph.register_gossip_node(
            FnnNodeAnnouncement {
                node_pubkey: "03bbb".to_string(),
                alias: "FA-2".to_string(),
                addresses: vec![],
                timestamp: 100,
            },
            2,
        );

        graph
            .ingest_channel_announcement(FnnChannelAnnouncement {
                channel_id: "chan-1".to_string(),
                node_1_pubkey: "03aaa".to_string(),
                node_2_pubkey: "03bbb".to_string(),
                total_capacity_shannons: 50_000_000,
            })
            .unwrap();

        graph
            .ingest_channel_update(FnnChannelUpdate {
                channel_id: "chan-1".to_string(),
                target_node_pubkey: "03aaa".to_string(),
                fee_base_shannons: 1000,
                fee_proportional_millionths: 10,
                is_enabled: true,
                local_balance_shannons: 10_000_000,
                timestamp: 200,
                asset_balances: None,
            })
            .unwrap();

        let route_success = graph.find_route(1, 2, 5_000_000, None);
        assert!(route_success.is_some());

        let route_fail = graph.find_route(1, 2, 15_000_000, None);
        assert!(route_fail.is_none());
    }

    #[test]
    fn heartbeat_multi_asset_updates_partitioned_capacities() {
        let mut graph = CompleteMeshGraph::with_lattice(8);
        let capacities = vec![
            mesh_core::types::AssetCapacity::rgb_plus_plus("0xrgb", 2_000_000, None),
            mesh_core::types::AssetCapacity::new(L2Asset::UDT("0xudt".to_string()), 3_000_000),
        ];
        graph.apply_heartbeat_multi_asset_liquidity(1, &capacities);
        let edge = graph
            .adjacency_map
            .get(&1)
            .and_then(|edges| edges.first())
            .expect("edge");
        assert_eq!(
            edge.capacity_for(&L2Asset::RgbPlusPlus("0xrgb".to_string())),
            2_000_000
        );
        assert_eq!(
            edge.capacity_for(&L2Asset::UDT("0xudt".to_string())),
            3_000_000
        );
    }
}
