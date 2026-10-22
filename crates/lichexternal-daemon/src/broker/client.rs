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

    /// Stream I/O error writing chunked data.
    #[error("Stream I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Background task join error.
    #[error("Stream task error: {0}")]
    Task(#[from] tokio::task::JoinError),

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

    /// Open a chunked HTTP stream for transmitting evaluation lines back to the broker.
    ///
    /// Endpoint: `POST /api/external-engine/work/{work_id}`
    pub fn start_work_stream(&self, work_id: &str) -> WorkStreamTransmitter {
        let url = format!("{}/api/external-engine/work/{}", self.broker_base_url, work_id);
        let (read_half, write_half) = tokio::io::duplex(65536);
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(read_half));
        let client = self.client.clone();
        let id_str = work_id.to_string();

        debug!(work_id = %id_str, url = %url, "Opening chunked HTTP stream to broker");

        let http_handle = tokio::spawn(async move {
            let response = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "text/plain")
                .body(body)
                .send()
                .await?;

            let status = response.status();
            debug!(work_id = %id_str, status = %status, "Broker work stream request finished");
            Ok(status)
        });

        WorkStreamTransmitter {
            work_id: work_id.to_string(),
            writer: write_half,
            http_handle,
        }
    }
}

/// Chunked line transmitter for an active evaluation job.
pub struct WorkStreamTransmitter {
    work_id: String,
    writer: tokio::io::DuplexStream,
    http_handle: tokio::task::JoinHandle<Result<StatusCode, BrokerError>>,
}

impl WorkStreamTransmitter {
    /// Transmit a single UCI output line (`info ...` or `bestmove ...`) to the broker HTTP stream.
    pub async fn send_line(&mut self, line: &str) -> Result<(), BrokerError> {
        let trimmed = line.trim();
        debug!(work_id = %self.work_id, line = %trimmed, "Streaming line to broker");

        use tokio::io::AsyncWriteExt;
        self.writer.write_all(trimmed.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        Ok(())
    }

    /// Check if the underlying HTTP stream connection has ended or disconnected.
    #[must_use]
    pub fn is_stream_active(&self) -> bool {
        !self.http_handle.is_finished()
    }

    /// Complete transmission by closing the write channel (sending EOF to HTTP chunked stream).
    pub async fn finish(self) -> Result<StatusCode, BrokerError> {
        debug!(work_id = %self.work_id, "Closing stream transmitter and awaiting broker response");
        drop(self.writer);

        match self.http_handle.await {
            Ok(result) => result,
            Err(e) => Err(BrokerError::Task(e)),
        }
    }
}
