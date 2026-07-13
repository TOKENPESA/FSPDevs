use std::sync::Arc;



use mesh_core::types::FiatProvider;

use serde_json::Value;

use uuid::Uuid;



use crate::fnn_client::FiberNodeRpc;

use crate::module_host::SidecarHost;

use crate::module_profile::{load_sidecar_profile, parse_fiat_provider, SidecarProfile};

use crate::module_system::SidecarModule;

use crate::modules::dicoba_module::DicobaModule;

use crate::modules::fiber_agent_swarm::AutonomousMarketMakerModule;

use crate::modules::fiat_bridge_module::FiatBridgeModule;

use crate::modules::lume_yielding::LumeYieldingModule;

use crate::modules::registry::{DynamicModuleRegistry, HotReloader};

use crate::modules::securities_compliance::SecuritiesComplianceModule;

use crate::modules::telco_sweep::TelcoB2cFiatSweepModule;

use crate::storage::AgentDb;



pub struct SidecarBootContext {

    pub agent_id: u16,

    pub fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>,

    pub db: Arc<AgentDb>,

    pub member_id: Uuid,

    pub profile: SidecarProfile,

}



impl SidecarBootContext {

    pub fn load(agent_id: u16, fnn_client: Arc<dyn FiberNodeRpc + Send + Sync>, db: Arc<AgentDb>, member_id: Uuid) -> Result<Self, String> {

        let profile = load_sidecar_profile(agent_id)?;

        Ok(Self {

            agent_id,

            fnn_client,

            db,

            member_id,

            profile,

        })

    }

}



pub async fn boot_sidecar_host(ctx: SidecarBootContext) -> Result<SidecarHost, String> {

    let ctx = Arc::new(ctx);

    let mut host = SidecarHost::new(

        ctx.agent_id,

        ctx.fnn_client.clone(),

        ctx.db.clone(),

        ctx.profile.clone(),

    );



    let reloader = HotReloader::new(

        DynamicModuleRegistry::new(host.outbound_tx.clone()),

        ctx.db.clone(),

        ctx.clone(),

    );

    reloader.hydrate_from_storage().await?;



    host.attach_hot_reloader(reloader);

    host.boot_background_runtimes().await;

    Ok(host)

}



pub fn build_module_with_config(

    module_id: &str,

    ctx: &SidecarBootContext,

    config: &Value,

) -> Result<Box<dyn SidecarModule>, String> {

    match module_id {

        "dicoba" => Ok(Box::new(DicobaModule::new(

            ctx.agent_id,

            ctx.db.clone(),

            ctx.fnn_client.clone(),

            ctx.member_id,

        ))),

        "fiat_bridge" => {
            let cfg = &ctx.profile.modules.fiat_bridge;
            let provider = if let Some(raw) = config.get("provider").and_then(Value::as_str) {
                parse_fiat_provider(raw)?
            } else if let Some(raw) = cfg.provider.as_deref() {
                parse_fiat_provider(raw)?
            } else {
                FiatProvider::Mpesa
            };

            let msisdn = config

                .get("msisdn")

                .and_then(Value::as_str)

                .map(str::to_string)

                .or_else(|| cfg.msisdn.clone())

                .unwrap_or_else(|| "255700000000".to_string());

            let msisdn = crate::module_profile::validate_msisdn(&msisdn)?;

            let critical_floor = config

                .get("critical_fiat_floor")

                .and_then(Value::as_f64)

                .or(cfg.critical_fiat_floor)

                .unwrap_or(50_000.0);

            Ok(Box::new(FiatBridgeModule::with_config(

                ctx.db.clone(),

                ctx.fnn_client.clone(),

                ctx.agent_id,

                provider,

                msisdn,

                critical_floor,

            )))

        }

        "telco_b2c_sweep" => Ok(Box::new(TelcoB2cFiatSweepModule::new(

            ctx.agent_id,

            ctx.db.clone(),

        ))),

        "lume_yielding" => Ok(Box::new(LumeYieldingModule::new(ctx.agent_id))),

        "securities_compliance" => Ok(Box::new(SecuritiesComplianceModule::new(ctx.agent_id))),

        "fiber_agent_swarm" => Ok(Box::new(AutonomousMarketMakerModule::new(

            ctx.agent_id,

            ctx.db.clone(),

            ctx.fnn_client.clone(),

        ))),

        other => Err(format!("unknown module id '{other}'")),

    }

}


