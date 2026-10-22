//! Cross-platform local daemon IPC control server using `interprocess`.

use interprocess::local_socket::traits::tokio::Listener as _;
use interprocess::local_socket::ListenerOptions;
use interprocess::local_socket::ToFsName;
#[cfg(windows)]
use interprocess::local_socket::ToNsName;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::supervisor::{EngineStatusSnapshot, SupervisorPool};

/// Incoming IPC commands from the `lichexternal` CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "payload", rename_all = "snake_case")]
pub enum IpcRequest {
    /// Ping daemon liveness.
    Ping,
    /// Request status snapshot of all engines.
    Status,
    /// Request graceful shutdown of daemon.
    Shutdown,
}

/// Outgoing IPC responses sent from the daemon to the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum IpcResponse {
    /// Pong response.
    Pong,
    /// Real-time engine status snapshot.
    Status {
        /// Daemon version.
        version: String,
        /// List of supervised engine states.
        engines: Vec<EngineStatusSnapshot>,
    },
    /// Generic success response.
    Ok {
        /// Informational message.
        message: String,
    },
    /// Request failed.
    Error {
        /// Error description.
        message: String,
    },
}

/// Runs the local control IPC server.
pub async fn run_ipc_server(
    socket_path: PathBuf,
    pool: Arc<SupervisorPool>,
    shutdown_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), anyhow::Error> {
    #[cfg(not(windows))]
    {
        // Ensure parent directory exists and remove stale socket file
        if let Some(parent) = socket_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::remove_file(&socket_path).await;
    }

    let socket_str = socket_path.to_string_lossy().to_string();

    #[cfg(windows)]
    let name = if socket_str.starts_with(r"\\.\pipe\") || socket_str.starts_with('@') {
        socket_str
            .clone()
            .to_ns_name::<interprocess::local_socket::GenericNamespaced>()
            .map_err(|e| anyhow::anyhow!("Invalid socket name: {e}"))?
    } else {
        socket_str
            .clone()
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .map_err(|e| anyhow::anyhow!("Invalid socket path: {e}"))?
    };

    #[cfg(not(windows))]
    let name = socket_str
        .clone()
        .to_fs_name::<interprocess::local_socket::GenericFilePath>()
        .map_err(|e| anyhow::anyhow!("Invalid socket path: {e}"))?;

    let listener = ListenerOptions::new()
        .name(name)
        .create_tokio()
        .map_err(|e| {
            anyhow::anyhow!("Failed to bind local IPC listener at '{}': {e}", socket_str)
        })?;

    while !*shutdown_rx.borrow() {
        tokio::select! {
            _ = shutdown_rx.changed() => break,
            accept_res = listener.accept() => {
                match accept_res {
                    Ok(stream) => {
                        let pool = Arc::clone(&pool);
                        let shutdown_tx = shutdown_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, pool, shutdown_tx).await {
                                debug!(error = %e, "Client IPC connection ended with error");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "Error accepting client IPC connection");
                    }
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = tokio::fs::remove_file(&socket_path).await;
    }

    info!("IPC control server stopped");
    Ok(())
}

async fn handle_client(
    stream: interprocess::local_socket::tokio::Stream,
    pool: Arc<SupervisorPool>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<(), anyhow::Error> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    while buf_reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let response = match serde_json::from_str::<IpcRequest>(trimmed) {
            Ok(IpcRequest::Ping) => IpcResponse::Pong,
            Ok(IpcRequest::Status) => IpcResponse::Status {
                version: lichexternal_core::VERSION.to_string(),
                engines: pool.get_statuses(),
            },
            Ok(IpcRequest::Shutdown) => {
                info!("IPC client requested daemon shutdown");
                let _ = shutdown_tx.send(true);
                IpcResponse::Ok {
                    message: "Daemon shutting down".to_string(),
                }
            }
            Err(e) => IpcResponse::Error {
                message: format!("Invalid IPC request: {e}"),
            },
        };

        let mut resp_json = serde_json::to_string(&response)?;
        resp_json.push('\n');
        writer.write_all(resp_json.as_bytes()).await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}
