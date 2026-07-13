//! MFA telemetry POST and offline queue flush.

use std::sync::Arc;

use mesh_core::types::MeshPulsePayload;
use secp256k1::SecretKey;

use crate::clearing_client::mfa_auth_headers;
use crate::identity::attach_telemetry_signature;
use crate::{mesh_unix_timestamp_secs, AgentDb};

pub fn prepare_ordered_telemetry_flush(mut cached_logs: Vec<MeshPulsePayload>) -> Vec<MeshPulsePayload> {
    cached_logs.sort_by_key(|payload| payload.timestamp);
    cached_logs
}

pub async fn post_telemetry(
    client: &reqwest::Client,
    url: &str,
    payload: &MeshPulsePayload,
) -> bool {
    match client
        .post(url)
        .headers(mfa_auth_headers())
        .json(payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => true,
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("⚠️ [MFA] Telemetry rejected with HTTP {status} — {body}");
            false
        }
        Err(e) => {
            eprintln!("⚠️ [MFA] Telemetry POST failed: {e}");
            false
        }
    }
}

pub async fn send_or_queue_telemetry(
    client: &reqwest::Client,
    url: &str,
    db: &Option<Arc<AgentDb>>,
    payload: &MeshPulsePayload,
    event_type: &str,
) -> bool {
    if post_telemetry(client, url, payload).await {
        return true;
    }
    if let Some(db_ref) = db {
        if let Err(e) = db_ref.enqueue_telemetry(event_type, payload) {
            eprintln!("⚠️ [STORAGE] enqueue telemetry failed: {e}");
        } else {
            eprintln!("📥 [STORAGE] Queued telemetry for MFA retry ({event_type}).");
        }
    }
    false
}

pub async fn flush_queued_telemetry(
    db: &AgentDb,
    client: &reqwest::Client,
    url: &str,
    signing_key: &SecretKey,
) -> usize {
    let mut sent = 0usize;
    loop {
        let item = match db.dequeue_telemetry() {
            Ok(Some(item)) => item,
            Ok(None) => break,
            Err(e) => {
                eprintln!("⚠️ [STORAGE] dequeue telemetry failed: {e}");
                break;
            }
        };

        let payload = match serde_json::from_str::<MeshPulsePayload>(&item.payload) {
            Ok(mut p) => {
                p.timestamp = mesh_unix_timestamp_secs();
                attach_telemetry_signature(p, signing_key)
            }
            Err(e) => {
                eprintln!("⚠️ [STORAGE] corrupt queued telemetry id={}: {e}", item.id);
                continue;
            }
        };

        if post_telemetry(client, url, &payload).await {
            sent += 1;
        } else if let Err(e) = db.enqueue_telemetry_raw(&item.event_type, &item.payload) {
            eprintln!("⚠️ [STORAGE] re-queue telemetry failed: {e}");
            break;
        } else {
            break;
        }
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload(agent: u16, timestamp: u64) -> MeshPulsePayload {
        MeshPulsePayload {
            agent_id: agent,
            timestamp,
            nonce: timestamp,
            local_capacity_shannons: 0,
            public_key_hex: None,
            signature_hex: None,
            status: "MESH_HEARTBEAT".to_string(),
            active_mesh_neighbors: vec![],
            report_target: agent,
            attempt: 0,
            fnn_pubkey_hex: None,
            peer_connect_address: None,
            asset_capacities: Vec::new(),
        }
    }

    #[test]
    fn prepare_ordered_telemetry_flush_sorts_by_timestamp() {
        let batch = prepare_ordered_telemetry_flush(vec![
            sample_payload(1, 300),
            sample_payload(2, 100),
            sample_payload(3, 200),
        ]);
        assert_eq!(
            batch.iter().map(|p| p.timestamp).collect::<Vec<_>>(),
            vec![100, 200, 300]
        );
    }
}
