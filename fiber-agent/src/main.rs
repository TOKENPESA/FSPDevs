//! Single Fiber Agent sidecar (one AGENT_ID). For all 1024 agents use `mesh-fleet-daemon`.

use fiber_agent_sidecar::{parse_agent_id, run_agent_sidecar, SidecarConfig};

#[tokio::main]
async fn main() {
    let agent_id = match parse_agent_id() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("❌ [CONFIG] {e}");
            return;
        }
    };

    let mut config = SidecarConfig::from_env();
    config.quiet = false;
    run_agent_sidecar(agent_id, config).await;
}
