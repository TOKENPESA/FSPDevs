use async_trait::async_trait;
use mesh_core::network::PeerModulePacket;
use serde_json::Value;
use tokio::sync::mpsc;

/// The standard interface for all FSP Sidecar modules (DICOBA, IoT, AI Agents)
#[async_trait]
pub trait SidecarModule: Send + Sync {
    fn module_name(&self) -> &'static str;

    /// Required for the fallback generator to self-identify
    fn local_agent_id(&self) -> u16;

    async fn initialize(&mut self) -> Result<(), String>;

    /// Handles requests directly from the local UI
    async fn handle_rpc_command(&self, method: &str, payload: Value) -> Result<Value, String>;

    /// Handles inbound requests from remote peers via the MFA
    async fn handle_peer_message(
        &self,
        _source_agent_id: u16,
        _method: &str,
        _payload: Value,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Connects the module to the host's outbound network pipeline
    fn set_outbound_channel(&mut self, _tx: mpsc::Sender<PeerModulePacket>) {}

    /// Builds the (unsigned) peer packet for an OOB fallback request. The host
    /// signs this before encoding so every emitted URI carries a proof.
    fn build_fallback_packet(
        &self,
        target_agent: u16,
        method: &str,
        payload: Value,
    ) -> PeerModulePacket {
        PeerModulePacket {
            source_agent_id: self.local_agent_id(),
            target_agent_id: target_agent,
            target_module: self.module_name().to_string(),
            method: method.to_string(),
            payload,
            signature: None,
        }
    }

    /// Standardized method to generate an OOB QR payload when the MFA is unreachable.
    /// Prefer `SidecarHost::generate_oob_fallback`, which signs the packet first.
    fn generate_fallback_request(
        &self,
        target_agent: u16,
        method: &str,
        payload: Value,
    ) -> Result<String, String> {
        self.build_fallback_packet(target_agent, method, payload)
            .to_fallback_uri()
    }
}
