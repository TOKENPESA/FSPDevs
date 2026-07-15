use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

#[derive(Serialize)]
pub struct TicketIssuanceResponse {
    pub ticket: String,
    pub expires_in_secs: u64,
}

/// Generates an isolated, secure single-use access ticket for compliance streams.
pub async fn issue_compliance_stream_ticket_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TicketIssuanceResponse>, StatusCode> {
    let ticket_uuid = state.compliance_tickets.issue_ticket().await;

    Ok(Json(TicketIssuanceResponse {
        ticket: ticket_uuid.to_string(),
        expires_in_secs: 30,
    }))
}

/// Mounts an authorized Server-Sent Events (SSE) pipeline streaming raw transaction metrics out to connected regulators.
pub async fn establish_regulatory_surveillance_feed(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    log::info!(
        "[SSE GATEWAY] Authorized connection established from sovereign surveillance system endpoint."
    );

    let inbound_broadcast_stream = BroadcastStream::new(state.compliance_broadcast.subscribe());

    let pipeline_stream = inbound_broadcast_stream
        .map(|incoming_frame| {
            match incoming_frame {
                Ok(audit_envelope) => {
                    if let Ok(serialized_json_string) = serde_json::to_string(&audit_envelope) {
                        Event::default().data(serialized_json_string)
                    } else {
                        Event::default().data(
                            "{\"error\":\"Serialization compilation conflict inside pipeline frame\"}",
                        )
                    }
                }
                Err(_) => Event::default().data(
                    "{\"warning\":\"Telemetry frame compression delay encountered in core stream loop\"}",
                ),
            }
        })
        .map(Ok);

    Sse::new(pipeline_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("fsp_pipeline_heartbeat_pulse"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HubConfig;
    use crate::workers::background::ExpiringLockManager;
    use mesh_core::compliance::{
        CentralBankMacroTelemetry, ComplianceAuditEnvelope, ComplianceVerdict,
        RevenueAuthorityTaxTelemetry,
    };
    use mesh_core::AssetRegistryHub;
    use mesh_core::MeshPubkeyRegistry;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::atomic::AtomicU16;
    use tokio::sync::{broadcast, mpsc, RwLock};
    use uuid::Uuid;

    fn mock_state() -> Arc<AppState> {
        let (plugin_registry, module_store, plugin_hot_reloader) =
            crate::test_support::test_plugin_stack(1_000_000);
        let (tx, _) = mpsc::channel(4);
        let (ui_broadcast, _) = broadcast::channel(4);
        let (compliance_broadcast, _) = broadcast::channel(8);
        Arc::new(AppState {
            tx_queue: tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(crate::graph::CompleteMeshGraph::new())),
            ui_broadcast,
            compliance_broadcast,
            compliance_tickets: crate::auth::EphemeralTicketRegistry::new(
                crate::auth::EPHEMERAL_TICKET_TTL_SECS,
            ),
            alert_dedupe: RwLock::new(HashSet::new()),
            alert_order: RwLock::new(VecDeque::new()),
            active_funding_locks: RwLock::new(ExpiringLockManager::new(60)),
            hub_config: HubConfig {
                rpc_url: "http://127.0.0.1:8227".to_string(),
                funding_allocation_shannons: 1_000_000,
            },
            agent_ws_token: "test".to_string(),
            api_token: "test-api-token-123456".to_string(),
            agent_fnn_pubkeys: RwLock::new(HashMap::new()),
            agent_peer_addresses: RwLock::new(HashMap::new()),
            mesh_pubkey_registry: MeshPubkeyRegistry::from_map(HashMap::new()),
            payment_waiters: Arc::new(RwLock::new(HashMap::new())),
            payment_engine: crate::payment::PaymentEngineState::new(),
            simulation_edge_nodes: AtomicU16::new(16),
            ws_allowed_origins: vec![],
            agent_liquidity_snap: RwLock::new(HashMap::new()),
            liquidity_copilot: RwLock::new(crate::workers::background::LiquidityCopilot::new()),
            multi_hub_registry: RwLock::new(crate::hub::MultiHubRegistry::new()),
            asset_registry: AssetRegistryHub::new(),
            papss_gateway: None,
            enterprise_clearinghouse: crate::clearinghouse::mock_enterprise_clearinghouse(),
            regional_clearing: Arc::new(crate::clearing::RegionalClearinghouseEngine::new()),
            edge_hardware_profiles: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry,
            module_store,
            plugin_hot_reloader,
        })
    }

    #[tokio::test]
    async fn issue_compliance_stream_ticket_returns_uuid() {
        let state = mock_state();
        let response = issue_compliance_stream_ticket_handler(State(state))
            .await
            .expect("ticket issued");
        assert_eq!(response.0.expires_in_secs, 30);
        assert!(uuid::Uuid::parse_str(&response.0.ticket).is_ok());
    }

    #[tokio::test]
    async fn surveillance_feed_accepts_compliance_broadcast_events() {
        let state = mock_state();
        let mut rx = state.compliance_broadcast.subscribe();

        state
            .compliance_broadcast
            .send(ComplianceAuditEnvelope {
                audit_id: Uuid::new_v4(),
                sequence_index: 1,
                transaction_timestamp: 1_700_000_000,
                central_bank_feed: CentralBankMacroTelemetry {
                    clearing_node_id: Uuid::new_v4(),
                    source_corridor_iso: "TZS".to_string(),
                    destination_corridor_iso: "KES".to_string(),
                    volume_fiat_value: 10_000.0,
                    rolling_24h_corridor_total: 80_000_000.0,
                    macro_velocity_percent: 12.5,
                    masked_kyc_token: "FSP-KYC-TZS-ABCD".to_string(),
                },
                revenue_authority_feed: RevenueAuthorityTaxTelemetry {
                    originating_agent_id: 44,
                    transaction_type: "CASH_OUT".to_string(),
                    gross_value_fiat: 10_000.0,
                    agent_commission_earned: 50.0,
                    calculated_sovereign_tax_levy: 10.0,
                    revenue_tax_code_reference: "REV-LEVY-TZS-NPS-2026".to_string(),
                },
                final_verdict: ComplianceVerdict::ClearedClean,
                administrative_lock_signature: None,
            })
            .expect("broadcast");

        let frame = rx.recv().await.expect("audit frame");
        assert_eq!(frame.final_verdict, ComplianceVerdict::ClearedClean);
    }
}
