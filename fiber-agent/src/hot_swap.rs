//! Hub-directed channel hot-swap commands from MFA.

use crate::fnn_client::FiberNodeRpc;
use crate::mesh::{is_live_fiber_pubkey, resolve_open_channel_shannons, MeshPubkeyRegistry};
use crate::{ConfigUpdatePayload, MeshChannelState};

pub async fn execute_hot_swap(
    fnn: &tokio::sync::Mutex<Box<dyn FiberNodeRpc + Send + Sync>>,
    pubkey_cache: &tokio::sync::RwLock<std::collections::HashMap<u16, String>>,
    registry: &MeshPubkeyRegistry,
    cmd: &ConfigUpdatePayload,
) {
    if cmd.command != "MESH_CHANNEL_HOT_SWAP" {
        return;
    }

    let funding = resolve_open_channel_shannons();

    let target_pubkey = {
        let cache = pubkey_cache.read().await;
        cache
            .get(&cmd.target_peer_id)
            .cloned()
            .unwrap_or_else(|| registry.resolve_sidecar(cmd.target_peer_id))
    };

    let alt_pubkey = {
        let cache = pubkey_cache.read().await;
        cache
            .get(&cmd.alternative_peer_id)
            .cloned()
            .unwrap_or_else(|| registry.resolve_sidecar(cmd.alternative_peer_id))
    };

    let backend = fnn.lock().await;

    if is_live_fiber_pubkey(&target_pubkey) {
        if let Err(e) = backend.close_channel(&target_pubkey, false).await {
            eprintln!(
                "⚠️ [HOT-SWAP] shutdown_channel toward FA-{} skipped: {e}",
                cmd.target_peer_id
            );
        }
    }

    match backend.open_channel(&alt_pubkey, funding, None).await {
        Ok(()) => println!(
            "🛠️ [HOT-SWAP] Opened/reactivated FNN channel toward FA-{} (pubkey tail: …{})",
            cmd.alternative_peer_id,
            alt_pubkey
                .chars()
                .rev()
                .take(8)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ),
        Err(e) => eprintln!(
            "❌ [HOT-SWAP] open_channel failed for FA-{}: {e}",
            cmd.alternative_peer_id
        ),
    }
}

pub fn refresh_pubkey_cache(
    channels: &[MeshChannelState],
    cache: &mut std::collections::HashMap<u16, String>,
) {
    for ch in channels {
        if let Some(ref pk) = ch.peer_pubkey {
            cache.insert(ch.peer_id, pk.clone());
        }
    }
}
