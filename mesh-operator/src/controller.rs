use std::collections::BTreeMap;
use std::sync::Arc;

use k8s_openapi::api::core::v1::{Container, EnvVar, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::api::PostParams;
use kube::runtime::controller::Action;
use kube::{Api, Client, Resource, ResourceExt};

use crate::crd::MeshFleet;

pub struct Context {
    pub client: Client,
}

/// Dispatches localized adjustments to child pods based on changes to the custom manifest spec
pub async fn reconcile(fleet: Arc<MeshFleet>, ctx: Arc<Context>) -> Result<Action, kube::Error> {
    let client = ctx.client.clone();
    let namespace = fleet.namespace().unwrap_or_else(|| "default".to_string());

    let pod_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    let fleet_name = fleet.name_any();
    let target_replicas = fleet.spec.replicas.clamp(0, 1024);

    println!(
        "🛸 [OPERATOR] Reconciling MeshFleet '{}' | Target Scale: {}",
        fleet_name, target_replicas
    );

    let owner_ref = OwnerReference {
        api_version: MeshFleet::api_version(&()).to_string(),
        kind: MeshFleet::kind(&()).to_string(),
        name: fleet_name.clone(),
        uid: fleet.uid().unwrap_or_default(),
        controller: Some(true),
        block_owner_deletion: Some(true),
    };

    for agent_id in 1..=target_replicas {
        let pod_name = format!("{fleet_name}-fa-{agent_id}");

        let fnn_rpc_port = fleet.spec.base_rpc_port + (agent_id * 10);
        let fnn_p2p_port = fnn_rpc_port + 1;

        match pod_api.get_opt(&pod_name).await? {
            Some(_) => continue,
            None => {
                println!(
                    "🌱 [OPERATOR] Spawning container profile node for FA-{agent_id} [RPC Port: {fnn_rpc_port}]"
                );

                let new_pod = compile_agent_pod_spec(
                    &pod_name,
                    agent_id,
                    fnn_rpc_port,
                    fnn_p2p_port,
                    &fleet,
                    owner_ref.clone(),
                );
                pod_api
                    .create(&PostParams::default(), &new_pod)
                    .await?;
            }
        }
    }

    Ok(Action::requeue(std::time::Duration::from_secs(30)))
}

/// Triggers manual cleanup tasks or updates if a fatal resource constraint failure occurs
pub fn error_policy(_fleet: Arc<MeshFleet>, error: &kube::Error, _ctx: Arc<Context>) -> Action {
    eprintln!("❌ [OPERATOR CRITICAL] Reconciliation execution failure: {error:?}");
    Action::requeue(std::time::Duration::from_secs(10))
}

fn compile_agent_pod_spec(
    name: &str,
    agent_id: i32,
    rpc_port: i32,
    p2p_port: i32,
    fleet: &MeshFleet,
    owner: OwnerReference,
) -> Pod {
    let mut labels = BTreeMap::new();
    labels.insert("app".to_string(), "fiber-mesh".to_string());
    labels.insert("fleet".to_string(), fleet.name_any());
    labels.insert("agent-id".to_string(), agent_id.to_string());

    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels),
            owner_references: Some(vec![owner]),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![
                Container {
                    name: "sidecar-daemon".to_string(),
                    image: Some(fleet.spec.agent_image.clone()),
                    env: Some(vec![
                        EnvVar {
                            name: "AGENT_ID".to_string(),
                            value: Some(agent_id.to_string()),
                            ..Default::default()
                        },
                        EnvVar {
                            name: "MFA_RPC_URL".to_string(),
                            value: Some(fleet.spec.mfa_target_url.clone()),
                            ..Default::default()
                        },
                        EnvVar {
                            name: "FNN_RPC_URL".to_string(),
                            value: Some(format!("http://127.0.0.1:{rpc_port}")),
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                },
                Container {
                    name: "fnn-core".to_string(),
                    image: Some(fleet.spec.fnn_image.clone()),
                    env: Some(vec![
                        EnvVar {
                            name: "FNN_RPC_PORT".to_string(),
                            value: Some(rpc_port.to_string()),
                            ..Default::default()
                        },
                        EnvVar {
                            name: "FNN_P2P_PORT".to_string(),
                            value: Some(p2p_port.to_string()),
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::MeshFleetSpec;

    #[test]
    fn pod_spec_carries_agent_env_and_ports() {
        let fleet = MeshFleet {
            metadata: kube::api::ObjectMeta {
                name: Some("test-fleet".to_string()),
                ..Default::default()
            },
            spec: MeshFleetSpec {
                replicas: 4,
                agent_image: "fspdevs/fiber-agent:latest".to_string(),
                fnn_image: "fspdevs/fnn:latest".to_string(),
                mfa_target_url: "http://127.0.0.1:1025".to_string(),
                base_rpc_port: 18_000,
            },
            status: None,
        };

        let pod = compile_agent_pod_spec(
            "test-fleet-fa-3",
            3,
            18_030,
            18_031,
            &fleet,
            OwnerReference {
                api_version: "fspdevs.infra/v1alpha1".to_string(),
                kind: "MeshFleet".to_string(),
                name: "test-fleet".to_string(),
                uid: "uid".to_string(),
                controller: Some(true),
                block_owner_deletion: Some(true),
            },
        );

        let spec = pod.spec.expect("pod spec");
        assert_eq!(spec.containers.len(), 2);

        let sidecar = &spec.containers[0];
        assert_eq!(sidecar.name, "sidecar-daemon");
        let env: BTreeMap<_, _> = sidecar
            .env
            .as_ref()
            .unwrap()
            .iter()
            .map(|e| (e.name.as_str(), e.value.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(env.get("AGENT_ID"), Some(&"3"));
        assert_eq!(env.get("MFA_RPC_URL"), Some(&"http://127.0.0.1:1025"));
        assert_eq!(env.get("FNN_RPC_URL"), Some(&"http://127.0.0.1:18030"));

        let fnn = &spec.containers[1];
        assert_eq!(fnn.name, "fnn-core");
    }
}
