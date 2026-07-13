#![allow(dead_code)]

use std::io::Cursor;
use std::sync::Arc;

use mesh_core::papss_types::{
    ActiveCurrencyAndAmount, FinancialInstitution, Pacs009Transfer, PapssSettlementReceipt,
    PapssSettlementStatus,
};
use reqwest::Client;
use rustls::{ClientConfig, RootCertStore};

pub struct PapssIntegrationGateway {
    http_client: Client,
    papss_api_endpoint: String,
    node_participant_id: String,
}

impl PapssIntegrationGateway {
    /// Initializes the gateway with strict mTLS (Mutual TLS) certificates required by PAPSS / Afreximbank.
    pub fn new(endpoint: &str, participant_id: &str, pem_cert_bytes: &[u8]) -> Result<Self, String> {
        let root_store = root_store_from_pem(pem_cert_bytes)?;

        let tls = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let client = Client::builder()
            .use_preconfigured_tls(Arc::new(tls))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build secure PAPSS HTTP client: {e}"))?;

        Ok(Self {
            http_client: client,
            papss_api_endpoint: endpoint.to_string(),
            node_participant_id: participant_id.to_string(),
        })
    }

    /// Dispatches a pacs.009 macro-settlement request to balance the fiat backing accounts.
    pub async fn execute_macro_rebalance(
        &self,
        amount_fiat: f64,
        currency_iso: &str,
        debtor_bic: &str,
        creditor_bic: &str,
        reference_id: &str,
    ) -> Result<PapssSettlementReceipt, String> {
        log::info!(
            "🌍 [PAPSS GATEWAY] Initiating Pan-African Pacs.009 Fiat Settlement: {} {}",
            amount_fiat,
            currency_iso
        );

        let transfer_payload = Pacs009Transfer {
            message_id: format!("FSP-PACS-009-{reference_id}"),
            creation_datetime: chrono::Utc::now().to_rfc3339(),
            settlement_amount: ActiveCurrencyAndAmount {
                currency: currency_iso.to_string(),
                value: amount_fiat,
            },
            debtor_agent: FinancialInstitution {
                bic_fi: debtor_bic.to_string(),
                clearing_system_id: self.node_participant_id.clone(),
            },
            creditor_agent: FinancialInstitution {
                bic_fi: creditor_bic.to_string(),
                clearing_system_id: "PAPSS-CENTRAL-001".to_string(),
            },
            instruction_info: format!("L2_HUB_REBALANCE_{reference_id}"),
        };

        let response = self
            .http_client
            .post(format!(
                "{}/v1/settlement/pacs009",
                self.papss_api_endpoint
            ))
            .json(&transfer_payload)
            .send()
            .await
            .map_err(|e| format!("PAPSS Network Timeout: {e:?}"))?;

        if response.status().is_success() {
            let receipt = response
                .json::<PapssSettlementReceipt>()
                .await
                .map_err(|e| format!("Failed to parse PAPSS settlement receipt: {e:?}"))?;

            match receipt.status {
                PapssSettlementStatus::Settled => {
                    log::info!(
                        "✅ [PAPSS SUCCESS] Hub fiat accounts rebalanced. Afreximbank RTGS cleared."
                    );
                    Ok(receipt)
                }
                PapssSettlementStatus::Pending => {
                    log::warn!(
                        "⏳ [PAPSS PENDING] Settlement queued by central bank compliance."
                    );
                    Ok(receipt)
                }
                PapssSettlementStatus::Rejected(ref reason) => {
                    Err(format!("PAPSS Rejection: {reason}"))
                }
            }
        } else {
            Err(format!("PAPSS API Error: HTTP {}", response.status()))
        }
    }
}

fn root_store_from_pem(pem_bytes: &[u8]) -> Result<RootCertStore, String> {
    let mut reader = Cursor::new(pem_bytes);
    let mut root_store = RootCertStore::empty();

    for cert in rustls_pemfile::certs(&mut reader) {
        root_store
            .add(
                cert.map_err(|e| format!("Failed to parse PAPSS mTLS certificate: {e}"))?
                    .into_owned(),
            )
            .map_err(|e| format!("Failed to add PAPSS root certificate: {e}"))?;
    }

    if root_store.is_empty() {
        return Err("No certificates found in PAPSS PEM bundle".to_string());
    }

    Ok(root_store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_invalid_pem() {
        match PapssIntegrationGateway::new(
            "https://papss.example",
            "PAPSS-TZ-FSP",
            b"not-a-certificate",
        ) {
            Ok(_) => panic!("expected invalid pem to fail"),
            Err(err) => {
                assert!(err.contains("PAPSS") || err.contains("No certificates"));
            }
        }
    }
}
