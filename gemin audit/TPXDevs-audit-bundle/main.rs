use axum::{
    extract::{
        ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

const RING_SIZE: u16 = 1024;
const CHANNEL_LIQUIDITY: u64 = 10_000_000_000_000;
const TELEMETRY_QUEUE: usize = 512;
const BROADCAST_CAP: usize = 128;
const PEER_TX_CAP: usize = 16;
const MAX_BODY_BYTES: usize = 16 * 1024;
const DEDUPE_CAP: usize = 2048;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MeshPulsePayload {
    pub status: String,
    #[serde(alias = "reporter")]
    pub agent: u16,
    pub active_mesh_neighbors: Vec<u16>,
    #[serde(alias = "target")]
    pub report_target: u16,
    pub attempt: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxRouteRequest {
    pub source: u16,
    pub destination: u16,
    pub amount_shannons: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteResponse {
    pub status: String,
    pub path: Vec<u16>,
    pub execution_latency_ms: u128,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigUpdatePayload {
    pub command: String,
    pub target_peer_id: u16,
    pub alternative_peer_id: u16,
}

#[derive(Copy, Clone, Debug)]
pub struct MeshEdge {
    pub target_node: u16,
    pub is_active: bool,
}

pub struct CompleteMeshGraph {
    pub adjacency_map: HashMap<u16, [MeshEdge; 3]>,
    pub edge_count: u8,
    pub offline_registry: HashSet<u16>,
}

impl CompleteMeshGraph {
    pub fn new(total_nodes: u16) -> Self {
        let mut adjacency_map = HashMap::with_capacity(total_nodes as usize);

        for i in 1..=total_nodes {
            let ring = if i == total_nodes { 1 } else { i + 1 };
            let skip = if i >= total_nodes - 1 { 1 } else { i + 2 };
            let chord = (i + (total_nodes / 2)) % total_nodes + 1;
            adjacency_map.insert(
                i,
                [
                    MeshEdge {
                        target_node: ring,
                        is_active: true,
                    },
                    MeshEdge {
                        target_node: skip,
                        is_active: true,
                    },
                    MeshEdge {
                        target_node: chord,
                        is_active: true,
                    },
                ],
            );
        }

        CompleteMeshGraph {
            adjacency_map,
            edge_count: 3,
            offline_registry: HashSet::new(),
        }
    }

    pub fn isolate_faulty_node(&mut self, dead_node: u16) {
        self.offline_registry.insert(dead_node);
        for channels in self.adjacency_map.values_mut() {
            for channel in channels.iter_mut() {
                if channel.target_node == dead_node {
                    channel.is_active = false;
                }
            }
        }
    }

    pub fn pick_healing_peer(&self, agent: u16, dead: u16) -> u16 {
        if let Some(channels) = self.adjacency_map.get(&agent) {
            for ch in channels {
                if ch.is_active
                    && ch.target_node != dead
                    && !self.offline_registry.contains(&ch.target_node)
                {
                    return ch.target_node;
                }
            }
        }
        (dead + 3) % RING_SIZE + 1
    }

    pub fn compute_multi_hop_route(&self, start: u16, end: u16, amount: u64) -> Option<Vec<u16>> {
        if !Self::valid_node(start) || !Self::valid_node(end) {
            return None;
        }
        if amount > CHANNEL_LIQUIDITY {
            return None;
        }

        let mut distances = [u32::MAX; (RING_SIZE + 1) as usize];
        let mut predecessors = [0u16; (RING_SIZE + 1) as usize];
        let mut visited = [false; (RING_SIZE + 1) as usize];
        let mut heap = BinaryHeap::new();

        distances[start as usize] = 0;
        heap.push(NodeHopState {
            node_id: start,
            cost: 0,
        });

        while let Some(NodeHopState { node_id, cost }) = heap.pop() {
            if visited[node_id as usize] {
                continue;
            }
            if cost > distances[node_id as usize] {
                continue;
            }
            visited[node_id as usize] = true;

            if node_id == end {
                let mut path = Vec::with_capacity(32);
                let mut current = end;
                while current != start {
                    path.push(current);
                    current = predecessors[current as usize];
                }
                path.push(start);
                path.reverse();
                return Some(path);
            }

            if let Some(channels) = self.adjacency_map.get(&node_id) {
                for channel in channels.iter().take(self.edge_count as usize) {
                    if !channel.is_active || self.offline_registry.contains(&channel.target_node) {
                        continue;
                    }

                    let next = channel.target_node;
                    let next_cost = cost + 1;
                    if next_cost < distances[next as usize] {
                        distances[next as usize] = next_cost;
                        predecessors[next as usize] = node_id;
                        heap.push(NodeHopState {
                            node_id: next,
                            cost: next_cost,
                        });
                    }
                }
            }
        }
        None
    }

    fn valid_node(id: u16) -> bool {
        (1..=RING_SIZE).contains(&id)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct NodeHopState {
    node_id: u16,
    cost: u32,
}

impl Ord for NodeHopState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.node_id.cmp(&other.node_id))
    }
}

impl PartialOrd for NodeHopState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

type PeerRegistry = Arc<RwLock<HashMap<u16, mpsc::Sender<AxumMessage>>>>;
type SharedGraph = Arc<RwLock<CompleteMeshGraph>>;

struct AppState {
    tx_queue: mpsc::Sender<MeshPulsePayload>,
    peers: PeerRegistry,
    graph: SharedGraph,
    ui_broadcast: broadcast::Sender<String>,
    alert_dedupe: RwLock<HashSet<(u16, u16)>>,
    alert_order: RwLock<VecDeque<(u16, u16)>>,
}

fn valid_node(id: u16) -> bool {
    (1..=RING_SIZE).contains(&id)
}

fn validate_telemetry(p: &MeshPulsePayload) -> bool {
    valid_node(p.agent)
        && valid_node(p.report_target)
        && p.active_mesh_neighbors.len() <= 8
        && p.active_mesh_neighbors.iter().all(|&n| valid_node(n))
}

fn validate_route(p: &TxRouteRequest) -> bool {
    valid_node(p.source) && valid_node(p.destination)
}

async fn record_alert_dedupe(state: &AppState, key: (u16, u16)) -> bool {
    let mut dedupe = state.alert_dedupe.write().await;
    if !dedupe.insert(key) {
        return false;
    }
    let mut order = state.alert_order.write().await;
    order.push_back(key);
    while order.len() > DEDUPE_CAP {
        if let Some(old) = order.pop_front() {
            dedupe.remove(&old);
        }
    }
    true
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<MeshPulsePayload>(TELEMETRY_QUEUE);
    let (ui_broadcast, _) = broadcast::channel::<String>(BROADCAST_CAP);
    let peers: PeerRegistry = Arc::new(RwLock::new(HashMap::new()));
    let graph: SharedGraph = Arc::new(RwLock::new(CompleteMeshGraph::new(RING_SIZE)));

    let shared_state = Arc::new(AppState {
        tx_queue: tx,
        peers: peers.clone(),
        graph: graph.clone(),
        ui_broadcast: ui_broadcast.clone(),
        alert_dedupe: RwLock::new(HashSet::new()),
        alert_order: RwLock::new(VecDeque::new()),
    });

    tokio::spawn(background_processor_worker(
        rx,
        peers.clone(),
        graph.clone(),
        ui_broadcast.clone(),
        shared_state.clone(),
    ));

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE])
        .allow_origin(AllowOrigin::list([
            "http://127.0.0.1:8088".parse().unwrap(),
            "http://localhost:8088".parse().unwrap(),
            "http://127.0.0.1:8787".parse().unwrap(),
            "http://localhost:8787".parse().unwrap(),
        ]));

    let app = Router::new()
        .route("/", get(health_handler))
        .route("/telemetry", post(ingest_telemetry_handler))
        .route("/route", post(calculate_transaction_route_handler))
        .route("/ws/monitor", get(ui_monitor_ws_handler))
        .route("/ws/:agent_id", get(ws_handler))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(cors)
        .with_state(shared_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 1025));
    println!(
        "🚀 [MFA-1025] Mesh Network Engine operational at http://{}",
        addr
    );

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!(
                "❌ [MFA-1025] Failed to bind {} — stop any other MFA instance first ({err})",
                addr
            );
            return;
        }
    };

    if let Err(err) = axum::serve(listener, app).await {
        eprintln!("❌ [MFA-1025] Server exited: {err}");
    }
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "master_fiber_agent",
        "mode": "mesh",
        "nodes": RING_SIZE,
        "telemetry": "/telemetry",
        "route": "/route",
        "websocket": "/ws/:agent_id",
        "monitor": "/ws/monitor",
        "dashboard": "http://localhost:8088/",
        "demo": "http://localhost:8787/demo/"
    }))
}

async fn calculate_transaction_route_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TxRouteRequest>,
) -> (StatusCode, Json<RouteResponse>) {
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    if !validate_route(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(RouteResponse {
                status: "INVALID_NODE_ID".to_string(),
                path: Vec::new(),
                execution_latency_ms: 0,
            }),
        );
    }

    let graph_read = state.graph.read().await;
    let latency = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            - start_time
    };

    match graph_read.compute_multi_hop_route(
        payload.source,
        payload.destination,
        payload.amount_shannons,
    ) {
        Some(path) => (
            StatusCode::OK,
            Json(RouteResponse {
                status: "ROUTE_FOUND".to_string(),
                path,
                execution_latency_ms: latency(),
            }),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(RouteResponse {
                status: "MESH_UNREACHABLE".to_string(),
                path: Vec::new(),
                execution_latency_ms: latency(),
            }),
        ),
    }
}

async fn ingest_telemetry_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MeshPulsePayload>,
) -> StatusCode {
    if !validate_telemetry(&payload) {
        return StatusCode::BAD_REQUEST;
    }
    match state.tx_queue.try_send(payload) {
        Ok(_) => StatusCode::ACCEPTED,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn background_processor_worker(
    mut rx: mpsc::Receiver<MeshPulsePayload>,
    peers: PeerRegistry,
    graph: SharedGraph,
    ui_broadcast: broadcast::Sender<String>,
    state: Arc<AppState>,
) {
    while let Some(telemetry) = rx.recv().await {
        if telemetry.status != "ALERT_MFA_NODE_DROPPED" {
            continue;
        }

        let dedupe_key = (telemetry.agent, telemetry.report_target);
        if !record_alert_dedupe(&state, dedupe_key).await {
            continue;
        }

        let fallback_target = {
            let mut graph_write = graph.write().await;
            graph_write.isolate_faulty_node(telemetry.report_target);
            graph_write.pick_healing_peer(telemetry.agent, telemetry.report_target)
        };

        println!(
            "🧠 [MFA MESH BRAIN] Isolated FA-{}. Healing FA-{} → FA-{}",
            telemetry.report_target, telemetry.agent, fallback_target
        );

        let update_command = ConfigUpdatePayload {
            command: "MESH_CHANNEL_HOT_SWAP".to_string(),
            target_peer_id: telemetry.report_target,
            alternative_peer_id: fallback_target,
        };

        let serialized = match serde_json::to_string(&update_command) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let registry = peers.read().await;
        if let Some(agent_tx) = registry.get(&telemetry.agent) {
            let _ = agent_tx.send(AxumMessage::Text(serialized)).await;
            let ui_event = format!(
                "{{\"event\":\"MESH_HEAL\",\"node\":{},\"removed\":{},\"added\":{}}}",
                telemetry.agent, telemetry.report_target, fallback_target
            );
            let _ = ui_broadcast.send(ui_event);
        }
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<u16>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    if !valid_node(agent_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let peers = state.peers.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, agent_id, peers))
}

async fn handle_socket(socket: WebSocket, agent_id: u16, peers: PeerRegistry) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::channel::<AxumMessage>(PEER_TX_CAP);
    peers.write().await.insert(agent_id, tx);

    let mut send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if ws_tx.send(message).await.is_err() {
                break;
            }
        }
    });
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if matches!(msg, AxumMessage::Close(_)) {
                break;
            }
        }
    });
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
    peers.write().await.remove(&agent_id);
}

async fn ui_monitor_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    let ui_broadcast = state.ui_broadcast.clone();
    ws.on_upgrade(move |socket| handle_ui_monitor_socket(socket, ui_broadcast))
}

async fn handle_ui_monitor_socket(socket: WebSocket, broadcast_channel: broadcast::Sender<String>) {
    let (mut ws_tx, _) = socket.split();
    let mut rx = broadcast_channel.subscribe();
    while let Ok(msg) = rx.recv().await {
        if ws_tx.send(AxumMessage::Text(msg)).await.is_err() {
            break;
        }
    }
}
