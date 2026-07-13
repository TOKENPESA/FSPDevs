//! Periodic mesh graph snapshots — atomic file writes out of the hot routing path.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mesh_core::MeshError;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use crate::graph::{CompleteMeshGraph, GraphSnapshot};
use crate::mfa_storage::resolve_mfa_db_path;

const DEFAULT_SNAPSHOT_INTERVAL_SECS: u64 = 60;

pub fn resolve_graph_snapshot_path() -> PathBuf {
    if let Ok(path) = env::var("MFA_GRAPH_SNAPSHOT_PATH") {
        return PathBuf::from(path);
    }
    resolve_mfa_db_path()
        .parent()
        .map(|dir| dir.join("mesh-graph.snapshot.json"))
        .unwrap_or_else(|| PathBuf::from("mesh-graph.snapshot.json"))
}

pub fn graph_snapshot_interval_secs() -> u64 {
    env::var("MFA_GRAPH_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&secs| secs >= 5)
        .unwrap_or(DEFAULT_SNAPSHOT_INTERVAL_SECS)
}

pub struct GraphPersistenceManager {
    graph: Arc<RwLock<CompleteMeshGraph>>,
    storage_path: PathBuf,
    interval_secs: u64,
}

impl GraphPersistenceManager {
    pub fn new(graph: Arc<RwLock<CompleteMeshGraph>>, storage_path: PathBuf) -> Self {
        Self {
            graph,
            storage_path,
            interval_secs: graph_snapshot_interval_secs(),
        }
    }

    pub fn storage_path(&self) -> &Path {
        &self.storage_path
    }

    pub fn snapshot_interval_secs(&self) -> u64 {
        self.interval_secs
    }

    /// Spawns the background persistence loop to snapshot state out of the active hot-path.
    pub fn spawn_snapshot_worker(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(self.interval_secs)).await;
                if let Err(err) = self.save_graph_snapshot().await {
                    log::error!("graph snapshot failed: {err}");
                }
            }
        });
    }

    pub async fn save_graph_snapshot(&self) -> Result<(), MeshError> {
        let serialized_data = {
            let graph_guard = self.graph.read().await;
            serde_json::to_vec(&graph_guard.to_snapshot()).map_err(|err| {
                MeshError::StorageError(format!("graph serialization failed: {err}"))
            })?
        };

        if let Some(parent) = self.storage_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
                MeshError::StorageError(format!("create snapshot directory: {err}"))
            })?;
        }

        let temp_path = self.storage_path.with_extension("tmp");
        tokio::fs::write(&temp_path, serialized_data)
            .await
            .map_err(|err| MeshError::StorageError(format!("write snapshot temp file: {err}")))?;

        tokio::fs::rename(&temp_path, &self.storage_path)
            .await
            .map_err(|err| MeshError::StorageError(format!("commit snapshot file: {err}")))?;

        log::debug!(
            "mesh graph snapshot saved to {}",
            self.storage_path.display()
        );
        Ok(())
    }

    pub async fn load_graph_snapshot(&self) -> Result<CompleteMeshGraph, MeshError> {
        if !self.storage_path.exists() {
            return Err(MeshError::StorageError(
                "no graph snapshot file found".to_string(),
            ));
        }

        let data = tokio::fs::read(&self.storage_path)
            .await
            .map_err(|err| MeshError::StorageError(format!("read snapshot file: {err}")))?;

        let snapshot: GraphSnapshot = serde_json::from_slice(&data).map_err(|err| {
            MeshError::StorageError(format!("graph deserialization failed: {err}"))
        })?;

        Ok(CompleteMeshGraph::from_snapshot(snapshot))
    }

    /// Hydrates the live graph from disk when a snapshot exists (no-op on missing file).
    pub async fn try_hydrate_graph(&self, graph: &Arc<RwLock<CompleteMeshGraph>>) {
        match self.load_graph_snapshot().await {
            Ok(restored) => {
                let mut guard = graph.write().await;
                *guard = restored;
                log::info!(
                    "restored mesh graph from snapshot {}",
                    self.storage_path.display()
                );
            }
            Err(MeshError::StorageError(msg)) if msg.contains("no graph snapshot") => {
                log::info!(
                    "no mesh graph snapshot at {} — starting fresh",
                    self.storage_path.display()
                );
            }
            Err(err) => {
                log::warn!(
                    "mesh graph snapshot load skipped ({err}) — using in-memory seed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::RING_SIZE;

    #[tokio::test]
    async fn snapshot_round_trip_preserves_topology() {
        let graph = Arc::new(RwLock::new(CompleteMeshGraph::with_lattice(8)));
        let path = std::env::temp_dir().join(format!(
            "mfa-graph-snapshot-{}.json",
            uuid::Uuid::new_v4()
        ));

        let manager = GraphPersistenceManager::new(graph.clone(), path.clone());
        manager.save_graph_snapshot().await.expect("save");

        let restored = manager.load_graph_snapshot().await.expect("load");
        assert!(!restored.adjacency_map.is_empty());
        assert_eq!(restored.get_version(), graph.read().await.get_version());

        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn try_hydrate_graph_replaces_live_state() {
        let graph = Arc::new(RwLock::new(CompleteMeshGraph::new()));
        let path = std::env::temp_dir().join(format!(
            "mfa-graph-hydrate-{}.json",
            uuid::Uuid::new_v4()
        ));

        let seed = GraphPersistenceManager::new(
            Arc::new(RwLock::new(CompleteMeshGraph::with_lattice(RING_SIZE))),
            path.clone(),
        );
        seed.save_graph_snapshot().await.expect("seed save");

        let manager = GraphPersistenceManager::new(graph.clone(), path.clone());
        manager.try_hydrate_graph(&graph).await;

        let guard = graph.read().await;
        assert!(!guard.adjacency_map.is_empty());

        let _ = tokio::fs::remove_file(path).await;
    }
}
