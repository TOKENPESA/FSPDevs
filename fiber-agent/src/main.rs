//! Single Fiber Agent sidecar (one AGENT_ID). For all 1024 agents use `mesh-fleet-daemon`.
//!
//! Dynamic MFA onboarding:
//! - Set `MFA_AUTO_REGISTER=true` (or omit `AGENT_ID`) to call `POST /api/register`
//!   and persist FA-N + HMAC secret under `FIBER_AGENT_STATE_DIR/agent_identity.db`.
//! - Set `AGENT_ID` + `MFA_AGENT_WS_TOKEN` with `MFA_AUTO_REGISTER=false` for legacy hub nodes.

use fiber_agent_sidecar::{resolve_runtime_identity, run_agent_sidecar, SidecarConfig};

#[tokio::main]
async fn main() {
    let mut config = SidecarConfig::from_env();
    config.quiet = false;

    let identity = match resolve_runtime_identity(&mut config).await {
        Ok(identity) => identity,
        Err(err) => {
            eprintln!("❌ [IDENTITY] {err}");
            return;
        }
    };

    run_agent_sidecar(identity.agent_id, config).await;
}
