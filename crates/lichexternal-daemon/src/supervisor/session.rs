//! Engine session lifecycle supervisor and work dispatch loop.

use std::cmp::min;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use lichexternal_core::config::{BackendType, EngineConfig};
use lichexternal_core::models::{AcquireResponse, EngineMetadata, UciVariant};
use lichexternal_core::uci::{
    parse_uci_line, GoParams, PositionInitial, UciCommand, UciOutputLine,
};

use crate::backend::{AnyBackend, EngineBackend, SocketBackend, StdioBackend};
use crate::broker::{BrokerClient, BrokerError};

/// Runtime operational status of an engine profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Initializing engine process and performing UCI handshake.
    Initializing,
    /// Engine is ready and idle, waiting for work assignments from Lichess.
    Idle,
    /// Engine is actively calculating an evaluation for a Lichess user.
    Analyzing {
        /// Active Lichess work ID.
        work_id: String,
        /// Current search depth reached.
        depth: Option<u32>,
    },
    /// Session was terminated or engine encountered a fatal error.
    Stopped,
    /// Engine encountered an error and is waiting to restart.
    Error(String),
}

/// Supervises a single engine profile, bridging it to Lichess broker work queues.
pub struct EngineSession {
    config: EngineConfig,
    broker_client: BrokerClient,
    metadata: Option<EngineMetadata>,
    state: SessionState,
    state_tx: watch::Sender<SessionState>,
}

impl EngineSession {
    /// Creates a new engine session for the given engine profile configuration.
    pub fn new(
        config: EngineConfig,
        broker_client: BrokerClient,
    ) -> (Self, watch::Receiver<SessionState>) {
        let (state_tx, state_rx) = watch::channel(SessionState::Initializing);
        let session = Self {
            config,
            broker_client,
            metadata: None,
            state: SessionState::Initializing,
            state_tx,
        };
        (session, state_rx)
    }

    /// Returns the configured engine ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.config.id
    }

    /// Returns the engine profile name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Returns detected metadata if handshake succeeded.
    #[must_use]
    pub fn metadata(&self) -> Option<&EngineMetadata> {
        self.metadata.as_ref()
    }

    fn set_state(&mut self, state: SessionState) {
        self.state = state.clone();
        let _ = self.state_tx.send(state);
    }

    /// Main supervisor loop. Automatically restarts the engine backend if it crashes.
    pub async fn run(&mut self, mut shutdown_rx: watch::Receiver<bool>) {
        info!(engine_id = %self.config.id, name = %self.config.name, "Starting engine session supervisor");

        while !*shutdown_rx.borrow() {
            self.set_state(SessionState::Initializing);

            let mut backend = match self.init_backend().await {
                Ok(b) => b,
                Err(e) => {
                    error!(engine_id = %self.config.id, error = %e, "Failed to initialize engine backend; retrying in 5s");
                    self.set_state(SessionState::Error(e.to_string()));
                    tokio::select! {
                        _ = sleep(Duration::from_secs(5)) => continue,
                        _ = shutdown_rx.changed() => break,
                    }
                }
            };

            info!(engine_id = %self.config.id, "Engine initialized successfully; entering work polling loop");
            self.set_state(SessionState::Idle);

            if let Err(e) = self.work_loop(&mut backend, &mut shutdown_rx).await {
                warn!(engine_id = %self.config.id, error = %e, "Engine session work loop ended; restarting backend");
            }
        }

        self.set_state(SessionState::Stopped);
        info!(engine_id = %self.config.id, "Engine session stopped");
    }

    async fn init_backend(&mut self) -> Result<AnyBackend, anyhow::Error> {
        let mut backend = match self.config.backend_type {
            BackendType::Stdio => {
                let cmd = self.config.command.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Missing command for stdio engine '{}'", self.config.id)
                })?;
                let stdio = StdioBackend::spawn(cmd, &self.config.args)?;
                AnyBackend::Stdio(Box::new(stdio))
            }
            BackendType::Socket => {
                let sock = self.config.socket.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Missing socket for socket engine '{}'", self.config.id)
                })?;
                let socket = SocketBackend::connect(sock).await?;
                AnyBackend::Socket(socket)
            }
        };

        // Handshake: send uci
        backend.send_command("uci").await?;

        let mut engine_name = None;
        let mut engine_author = None;
        let mut available_options = Vec::new();

        loop {
            match backend.read_line().await? {
                Some(line) => match parse_uci_line(&line) {
                    UciOutputLine::UciOk => break,
                    UciOutputLine::IdName(name) => engine_name = Some(name.to_string()),
                    UciOutputLine::IdAuthor(author) => engine_author = Some(author.to_string()),
                    UciOutputLine::Option(opt) => {
                        available_options.push(opt.name.to_string());
                    }
                    _ => {}
                },
                None => {
                    return Err(anyhow::anyhow!(
                        "Engine process disconnected during UCI handshake"
                    ))
                }
            }
        }

        self.metadata = Some(EngineMetadata {
            name: engine_name.unwrap_or_else(|| self.config.name.clone()),
            author: engine_author,
            available_options,
        });

        // Initialize static options
        for (name, value) in &self.config.uci_options {
            backend
                .send_command(
                    &UciCommand::set_option_owned(name.clone(), Some(value.clone())).to_uci_line(),
                )
                .await?;
        }

        // Synchronize with isready
        backend.send_command("isready").await?;
        loop {
            match backend.read_line().await? {
                Some(line) if parse_uci_line(&line) == UciOutputLine::ReadyOk => break,
                Some(_) => continue,
                None => {
                    return Err(anyhow::anyhow!(
                        "Engine process disconnected waiting for readyok"
                    ))
                }
            }
        }

        Ok(backend)
    }

    async fn work_loop(
        &mut self,
        backend: &mut AnyBackend,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) -> Result<(), anyhow::Error> {
        let mut backoff = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(10);

        while !*shutdown_rx.borrow() {
            if !backend.is_alive() {
                return Err(anyhow::anyhow!("Engine process died unexpectedly"));
            }

            self.set_state(SessionState::Idle);

            tokio::select! {
                _ = shutdown_rx.changed() => break,
                poll_res = self.broker_client.poll_work(&self.config.provider_secret) => {
                    match poll_res {
                        Ok(None) => {
                            // 204 No Content timeout: broker is alive, reset backoff and immediately re-poll
                            backoff = Duration::from_secs(1);
                        }
                        Ok(Some(acquire_resp)) => {
                            backoff = Duration::from_secs(1);
                            self.execute_work(backend, acquire_resp, shutdown_rx).await?;
                        }
                        Err(BrokerError::AuthFailed(msg)) => {
                            error!(engine_id = %self.config.id, error = %msg, "Permanent broker authentication failure; backing off");
                            self.set_state(SessionState::Error(format!("Broker auth failed: {msg}")));
                            sleep(MAX_BACKOFF).await;
                        }
                        Err(e) => {
                            warn!(engine_id = %self.config.id, error = %e, "Broker polling error; backing off");
                            sleep(backoff).await;
                            backoff = min(backoff * 2, MAX_BACKOFF);
                        }
                    }
                }
            }
        }

        // Clean shutdown
        let _ = backend.send_command("quit").await;
        Ok(())
    }

    async fn execute_work(
        &mut self,
        backend: &mut AnyBackend,
        acquire_resp: AcquireResponse,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) -> Result<(), anyhow::Error> {
        let work = acquire_resp.work;
        let work_id = acquire_resp.id;

        info!(engine_id = %self.config.id, work_id = %work_id, "Executing work calculation");
        self.set_state(SessionState::Analyzing {
            work_id: work_id.clone(),
            depth: None,
        });

        // Set engine options for this work assignment
        backend.send_command("ucinewgame").await?;

        let threads = min(work.threads, self.config.max_threads);
        backend
            .send_command(
                &UciCommand::set_option_owned("Threads".into(), Some(threads.to_string()))
                    .to_uci_line(),
            )
            .await?;

        let hash = min(work.hash, self.config.max_hash);
        backend
            .send_command(
                &UciCommand::set_option_owned("Hash".into(), Some(hash.to_string())).to_uci_line(),
            )
            .await?;

        backend
            .send_command(
                &UciCommand::set_option_owned("MultiPV".into(), Some(work.multi_pv.to_string()))
                    .to_uci_line(),
            )
            .await?;

        if work.variant != UciVariant::Chess {
            backend
                .send_command(
                    &UciCommand::set_option_owned(
                        "UCI_Variant".into(),
                        Some(work.variant.as_uci_str().into()),
                    )
                    .to_uci_line(),
                )
                .await?;
        }

        // Ensure engine is ready after setting options
        backend.send_command("isready").await?;
        loop {
            match backend.read_line().await? {
                Some(line) if parse_uci_line(&line) == UciOutputLine::ReadyOk => break,
                Some(_) => continue,
                None => {
                    return Err(anyhow::anyhow!(
                        "Engine disconnected waiting for readyok before position"
                    ))
                }
            }
        }

        // Set position
        let initial = match work.initial_fen {
            Some(fen) => PositionInitial::fen_owned(fen),
            None => PositionInitial::Startpos,
        };
        let moves = work
            .moves
            .into_iter()
            .map(std::borrow::Cow::Owned)
            .collect();
        let pos_cmd = UciCommand::Position { initial, moves };
        backend.send_command(&pos_cmd.to_uci_line()).await?;

        // Open streaming transmitter to broker
        let mut transmitter = self.broker_client.start_work_stream(&work_id);

        // Start search
        let go_params = GoParams {
            depth: work.search_limit.depth,
            movetime: work.search_limit.movetime,
            nodes: work.search_limit.nodes,
            ..Default::default()
        };
        let go_cmd = UciCommand::Go(go_params);
        backend.send_command(&go_cmd.to_uci_line()).await?;

        // Calculation & streaming loop
        let mut canceled = false;

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    info!(work_id = %work_id, "Shutdown signal received during calculation; stopping engine");
                    canceled = true;
                    break;
                }
                read_res = backend.read_line() => {
                    let line = match read_res? {
                        Some(l) => l,
                        None => {
                            return Err(anyhow::anyhow!("Engine disconnected during search"));
                        }
                    };

                    // Check if the broker disconnected before or during send
                    if !transmitter.is_stream_active() {
                        info!(work_id = %work_id, "Broker HTTP stream disconnected; cancelling search");
                        canceled = true;
                        break;
                    }

                    if let Err(e) = transmitter.send_line(&line).await {
                        warn!(work_id = %work_id, error = %e, "Failed to stream line to broker; cancelling search");
                        canceled = true;
                        break;
                    }

                    let parsed = parse_uci_line(&line);
                    match parsed {
                        UciOutputLine::Info(info) => {
                            if let Some(d) = info.depth {
                                self.set_state(SessionState::Analyzing {
                                    work_id: work_id.clone(),
                                    depth: Some(d),
                                });
                            }
                        }
                        UciOutputLine::BestMove { best_move, ponder } => {
                            debug!(work_id = %work_id, best_move, ?ponder, "Engine completed search with bestmove");
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if canceled {
            // Send stop immediately and drain stdout until bestmove
            debug!(work_id = %work_id, "Sending stop to engine to drain search");
            let _ = backend.stop().await;

            // Drain engine lines until bestmove so backend remains clean
            while let Ok(Some(line)) = backend.read_line().await {
                if matches!(parse_uci_line(&line), UciOutputLine::BestMove { .. }) {
                    break;
                }
            }
        }

        let _ = transmitter.finish().await;
        info!(work_id = %work_id, canceled, "Completed work item processing");

        Ok(())
    }
}
