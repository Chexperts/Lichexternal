//! Concurrent multi-engine supervisor pool.

use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::info;

use lichexternal_core::config::{BackendType, Config, EngineConfig};
use crate::broker::BrokerClient;
use super::session::{EngineSession, SessionState};

/// Snapshot status of an individual engine supervised by the daemon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineStatusSnapshot {
    /// Engine profile identifier.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Transport backend type.
    pub backend_type: BackendType,
    /// Textual summary of current runtime state.
    pub state: String,
    /// Currently analyzing work ID (if active).
    pub active_work_id: Option<String>,
    /// Current search depth (if active).
    pub current_depth: Option<u32>,
}

struct EngineEntry {
    config: EngineConfig,
    state_rx: watch::Receiver<SessionState>,
    _handle: JoinHandle<()>,
}

/// Supervises a pool of concurrent engine sessions running independent broker dispatch loops.
pub struct SupervisorPool {
    broker_client: BrokerClient,
    engines: Vec<EngineEntry>,
    shutdown_tx: watch::Sender<bool>,
}

impl SupervisorPool {
    /// Spawn supervisor tasks for all engines configured in `Config`.
    pub fn new(config: &Config) -> (Arc<Self>, watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let broker_client = BrokerClient::new(config.broker_base_url.clone());

        let mut engines = Vec::new();

        for engine_cfg in &config.engines {
            info!(engine_id = %engine_cfg.id, name = %engine_cfg.name, "Spawning engine supervisor in pool");

            let (mut session, state_rx) = EngineSession::new(engine_cfg.clone(), broker_client.clone());
            let session_shutdown_rx = shutdown_rx.clone();

            let handle = tokio::spawn(async move {
                session.run(session_shutdown_rx).await;
            });

            engines.push(EngineEntry {
                config: engine_cfg.clone(),
                state_rx,
                _handle: handle,
            });
        }

        let pool = Arc::new(Self {
            broker_client,
            engines,
            shutdown_tx,
        });

        (pool, shutdown_rx)
    }

    /// Access the shared broker client.
    #[must_use]
    pub fn broker_client(&self) -> &BrokerClient {
        &self.broker_client
    }

    /// Retrieve a real-time status snapshot of all running engines in the pool.
    pub fn get_statuses(&self) -> Vec<EngineStatusSnapshot> {
        self.engines
            .iter()
            .map(|entry| {
                let state_val = entry.state_rx.borrow().clone();
                let (state_str, active_work_id, current_depth) = match state_val {
                    SessionState::Initializing => ("initializing".to_string(), None, None),
                    SessionState::Idle => ("idle".to_string(), None, None),
                    SessionState::Analyzing { work_id, depth } => ("analyzing".to_string(), Some(work_id), depth),
                    SessionState::Stopped => ("stopped".to_string(), None, None),
                    SessionState::Error(err) => (format!("error: {err}"), None, None),
                };

                EngineStatusSnapshot {
                    id: entry.config.id.clone(),
                    name: entry.config.name.clone(),
                    backend_type: entry.config.backend_type,
                    state: state_str,
                    active_work_id,
                    current_depth,
                }
            })
            .collect()
    }

    /// Trigger graceful shutdown of all supervised engines.
    pub fn shutdown(&self) {
        info!("Signaling shutdown to all engine pool supervisors");
        let _ = self.shutdown_tx.send(true);
    }
}
