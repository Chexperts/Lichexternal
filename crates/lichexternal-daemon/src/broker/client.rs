//! Lichess Broker client for work acquisition and streaming responses.

use std::time::Duration;
use reqwest::{Client, StatusCode};
use tracing::{debug, error, info, warn};

use lichexternal_core::models::{AcquireRequest, AcquireResponse};

/// Broker communication errors.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Authentication failure (e.g. invalid provider secret).
    #[error("Broker authentication failed: {0}")]
    AuthFailed(String),

    /// Deserialization error.
    #[error("Failed to parse broker response: {0}")]
    Decode(#[from] serde_json::Error),

    /// Server returned an unhandled error status.
    #[error("Broker returned error HTTP status: {0}")]
    HttpStatus(StatusCode),
}

/// Client for polling and streaming work to `engine.lichess.ovh`.
#[derive(Clone)]
pub struct BrokerClient {
    client: Client,
    broker_base_url: String,
}

impl BrokerClient {
    /// Creates a new broker client.
    pub fn new(broker_base_url: String) -> Self {
        let client = Client::builder()
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            broker_base_url: broker_base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Long-polls `POST /api/external-engine/work` for pending evaluation work.
    ///
    /// Returns:
    /// - `Ok(Some(AcquireResponse))` if work was assigned.
    /// - `Ok(None)` if the broker long-poll timed out with 204 No Content (ready to re-poll).
    /// - `Err(BrokerError)` on network or authentication failure.
    pub async fn poll_work(&self, provider_secret: &str) -> Result<Option<AcquireResponse>, BrokerError> {
        let url = format!("{}/api/external-engine/work", self.broker_base_url);
        let req_body = AcquireRequest::new(provider_secret.to_string());

        debug!(url = %url, "Long-polling broker for work");

        let response = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .await?;

        match response.status() {
            StatusCode::NO_CONTENT => {
                debug!("Broker poll timed out with 204 No Content");
                Ok(None)
            }
            StatusCode::OK => {
                let resp: AcquireResponse = response.json().await?;
                info!(work_id = %resp.id, variant = %resp.work.variant, "Acquired analysis work from broker");
                Ok(Some(resp))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                let body = response.text().await.unwrap_or_default();
                error!(status = %StatusCode::UNAUTHORIZED, body = %body, "Broker authentication rejected");
                Err(BrokerError::AuthFailed(body))
            }
            status => {
                warn!(status = %status, "Broker returned unexpected HTTP status");
                Err(BrokerError::HttpStatus(status))
            }
        }
    }
}
