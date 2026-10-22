//! Generic engine communication backends and abstractions.

use std::future::Future;

pub mod stdio;
pub use stdio::StdioBackend;

/// Errors arising from engine backend communication.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// Engine child process exited unexpectedly.
    #[error("Engine process terminated unexpectedly with exit code: {0:?}")]
    ProcessExited(Option<i32>),

    /// Underlying I/O error communicating with the engine.
    #[error("Engine I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Communication channel was abruptly closed or disconnected.
    #[error("Engine communication channel disconnected")]
    Disconnected,

    /// Handshake with the engine timed out or failed.
    #[error("Engine handshake failed: {0}")]
    HandshakeFailed(String),

    /// Operation timed out.
    #[error("Engine operation timed out")]
    Timeout,
}

/// Generic interface for communicating with an engine session.
///
/// Implementations handle both child OS processes (`StdioBackend`) and persistent
/// daemon connections over local sockets / named pipes (`SocketBackend`).
pub trait EngineBackend: Send + Sync {
    /// Transmit a raw UCI command line (with trailing newline) to the engine.
    fn send_command(&mut self, cmd: &str) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Read the next stdout/socket line emitted by the engine.
    ///
    /// Returns `Ok(None)` if EOF is reached.
    fn read_line(&mut self) -> impl Future<Output = Result<Option<String>, BackendError>> + Send;

    /// Send a `stop\n` command to immediately halt the engine's current calculation.
    fn stop(&mut self) -> impl Future<Output = Result<(), BackendError>> + Send {
        async move { self.send_command("stop").await }
    }

    /// Check if the engine session connection is still alive.
    fn is_alive(&mut self) -> bool;
}
