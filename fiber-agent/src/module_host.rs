use std::env;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::fnn_client::FiberNodeRpc;
use crate::module_profile::SidecarProfile;
use crate::modules::registry::{DynamicModuleRegistry, HotReloader};
use crate::peer_packet::verify_peer_module_packet_signature;
use crate::storage::AgentDb;
use mesh_core::network::PeerModulePacket;

/// Dev/testing escape hatch: accept unsigned or unverifiable OOB packets.
/// Defaults to `false` — signatures are enforced.
fn oob_allow_unsigned() -> bool {
    env::var("FSP_OOB_ALLOW_UNSIGNED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub struct SidecarHost {
    pub agent_id: u16,
    pub fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
    pub db: Arc<AgentDb>,
    profile: SidecarProfile,
    pub registry: DynamicModuleRegistry,
    hot_reloader: Option<Arc<HotReloader>>,
    /// Channel for modules to request outbound network transmission
    pub outbound_tx: mpsc::Sender<PeerModulePacket>,
    outbound_rx: Option<mpsc::Receiver<PeerModulePacket>>,
}

impl SidecarHost {
    pub fn new(
        agent_id: u16,
        fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,
        db: Arc<AgentDb>,
        profile: SidecarProfile,
    ) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            agent_id,
            fnn_client,
            db,
            profile,
            registry: DynamicModuleRegistry::new(tx.clone()),
            hot_reloader: None,
            outbound_tx: tx,
            outbound_rx: Some(rx),
        }
    }

    pub fn attach_hot_reloader(&mut self, reloader: HotReloader) {
        self.registry = reloader.registry.clone();
        self.hot_reloader = Some(Arc::new(reloader));
    }

    pub fn hot_reloader(&self) -> Option<Arc<HotReloader>> {
        self.hot_reloader.clone()
    }

    pub fn profile(&self) -> &SidecarProfile {
        &self.profile
    }

    pub async fn is_module_mounted(&self, module_id: &str) -> bool {
        self.registry.is_mounted(module_id).await
    }

    /// Starts all background tasks for loaded modules
    pub async fn boot_background_runtimes(&self) {
        for name in self.registry.mounted_names().await {
            log::info!("⚙️ [SIDECAR HOST] Background runtime slot reserved for: {name}");
        }
    }

    /// Routes an incoming RPC command from the Tauri UI to the specific module
    pub async fn route_command(
        &self,
        target_module: &str,
        method: &str,
        payload: Value,
    ) -> Result<Value, String> {
        self.registry
            .route_command(target_module, method, payload)
            .await
    }

    /// Routes inbound peer messages from the MFA down to the specific module
    pub async fn route_peer_message(&self, packet: PeerModulePacket) -> Result<(), String> {
        self.registry.route_peer_message(packet).await
    }

    /// Extracts the receiver so the background WS loop can push these to the MFA
    pub fn take_outbound_receiver(&mut self) -> Option<mpsc::Receiver<PeerModulePacket>> {
        self.outbound_rx.take()
    }

    /// Builds a signed `fsp://oob?data=…` URI via the mounted module.
    pub fn generate_oob_fallback(
        &self,
        target_module: &str,
        target_agent: u16,
        method: &str,
        payload: Value,
    ) -> Result<String, String> {
        self.registry
            .generate_oob_fallback(self.agent_id, target_module, target_agent, method, payload)
    }

    /// Universal entry point for any Out-of-Band physical fallback payload.
    pub async fn process_oob_fallback(&self, uri_string: &str) -> Result<(), String> {
        let packet = PeerModulePacket::from_fallback_uri(uri_string)?;

        if packet.target_agent_id != self.agent_id {
            return Err(format!(
                "OOB packet is addressed to FA-{} but this sidecar is FA-{}.",
                packet.target_agent_id, self.agent_id
            ));
        }

        let _signing_bytes = packet
            .signing_bytes()
            .map_err(|e| format!("OOB signing payload invalid: {e}"))?;
        if packet.signature.is_none() {
            return Err("OOB payload missing cryptographic signature".to_string());
        }

        match verify_peer_module_packet_signature(&packet) {
            Ok(()) => {
                log::info!(
                    "🔐 [OOB FALLBACK] Verified signature from FA-{} for module {}",
                    packet.source_agent_id, packet.target_module
                );
            }
            Err(err) => {
                if oob_allow_unsigned() {
                    log::warn!(
                        "⚠️ [OOB FALLBACK] Accepting unverified packet from FA-{} (FSP_OOB_ALLOW_UNSIGNED set): {err}",
                        packet.source_agent_id
                    );
                } else {
                    return Err(format!(
                        "OOB packet rejected — signature check failed for FA-{}: {err}",
                        packet.source_agent_id
                    ));
                }
            }
        }

        self.route_peer_message(packet).await
    }

    pub async fn registered_module_names(&self) -> Vec<String> {
        self.registry.mounted_names().await
    }
}
