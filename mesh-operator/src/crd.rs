use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Custom Resource specification tracking desired states for the Fiber Agent network grid
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "tpxdevs.infra",
    version = "v1alpha1",
    kind = "MeshFleet",
    plural = "meshfleets",
    status = "MeshFleetStatus",
    namespaced
)]
pub struct MeshFleetSpec {
    /// Total target scale of Fiber Agent units across the mesh topology grid (1 to 1024)
    pub replicas: i32,

    /// Image identifier tag specifying the exact build of fiber-agent-daemon to run
    pub agent_image: String,

    /// Image identifier tag specifying the companion FNN underlying node process
    pub fnn_image: String,

    /// Loopback or cluster URL for the Master Fiber Agent coordination gateway
    pub mfa_target_url: String,

    /// Base configuration allocation port offset mirroring the mesh_ports framework
    pub base_rpc_port: i32,
}

/// Operational status telemetry structure tracking current live allocations
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
pub struct MeshFleetStatus {
    pub current_replicas: i32,
    pub fully_armed_nodes: i32,
    pub active_port_range: String,
    pub sync_complete: bool,
}
