//! REST client for Lichess External Engine profile management API.

use reqwest::{Client, StatusCode};
use lichexternal_core::models::{CreateEngineRequest, EngineProfile, UpdateEngineRequest};

/// Client for interacting with `https://lichess.org/api/external-engine`.
pub struct LichessRestClient {
    client: Client,
    base_url: String,
    token: String,
}

impl LichessRestClient {
    /// Creates a new REST client authenticated with a user personal access token.
    pub fn new(base_url: String, token: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// List all external engines registered for the current user (`GET /api/external-engine`).
    pub async fn list_engines(&self) -> Result<Vec<EngineProfile>, anyhow::Error> {
        let url = format!("{}/api/external-engine", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Lichess API returned {status}: {body}"));
        }

        let engines: Vec<EngineProfile> = resp.json().await?;
        Ok(engines)
    }

    /// Register a new engine profile (`POST /api/external-engine`).
    pub async fn create_engine(&self, req: &CreateEngineRequest) -> Result<EngineProfile, anyhow::Error> {
        let url = format!("{}/api/external-engine", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Failed to register engine ({status}): {body}"));
        }

        let engine: EngineProfile = resp.json().await?;
        Ok(engine)
    }

    /// Retrieve an individual engine profile (`GET /api/external-engine/{id}`).
    pub async fn get_engine(&self, id: &str) -> Result<EngineProfile, anyhow::Error> {
        let url = format!("{}/api/external-engine/{id}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Failed to inspect engine {id} ({status}): {body}"));
        }

        let engine: EngineProfile = resp.json().await?;
        Ok(engine)
    }

    /// Update an engine profile (`PUT /api/external-engine/{id}`).
    pub async fn update_engine(&self, id: &str, req: &UpdateEngineRequest) -> Result<EngineProfile, anyhow::Error> {
        let url = format!("{}/api/external-engine/{id}", self.base_url);
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Failed to update engine {id} ({status}): {body}"));
        }

        let engine: EngineProfile = resp.json().await?;
        Ok(engine)
    }

    /// Delete an engine profile (`DELETE /api/external-engine/{id}`).
    pub async fn delete_engine(&self, id: &str) -> Result<(), anyhow::Error> {
        let url = format!("{}/api/external-engine/{id}", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        match resp.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            status => {
                let body = resp.text().await.unwrap_or_default();
                Err(anyhow::anyhow!("Failed to delete engine {id} ({status}): {body}"))
            }
        }
    }
}
