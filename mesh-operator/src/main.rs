extern crate serde_json;

mod controller;
mod crd;

use std::sync::Arc;

use controller::{error_policy, reconcile, Context};
use crd::MeshFleet;
use futures::StreamExt;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 [INITIALIZING] Starting distributed Mesh-Fleet Kubernetes Operator engine...");

    let client = Client::try_default().await?;
    let fleets: Api<MeshFleet> = Api::all(client.clone());
    let context = Arc::new(Context { client: client.clone() });

    Controller::new(fleets, Config::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok((object_ref, _action)) => {
                    println!(
                        "✅ [OPERATOR] Synchronized record update for configuration: {:?}",
                        object_ref
                    );
                }
                Err(err) => {
                    eprintln!("❌ [OPERATOR ERROR] Synchronization pipeline fault: {err:?}");
                }
            }
        })
        .await;

    Ok(())
}
