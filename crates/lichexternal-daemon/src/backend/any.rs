//! Concrete enum dispatcher wrapping any supported EngineBackend.

use super::{BackendError, EngineBackend, SocketBackend, StdioBackend};

/// Unified enum over all supported engine backend implementations.
pub enum AnyBackend {
    /// Subprocess child process backend.
    Stdio(Box<StdioBackend>),
    /// Persistent socket/pipe backend.
    Socket(SocketBackend),
}

impl EngineBackend for AnyBackend {
    async fn send_command(&mut self, cmd: &str) -> Result<(), BackendError> {
        match self {
            Self::Stdio(b) => b.send_command(cmd).await,
            Self::Socket(b) => b.send_command(cmd).await,
        }
    }

    async fn read_line(&mut self) -> Result<Option<String>, BackendError> {
        match self {
            Self::Stdio(b) => b.read_line().await,
            Self::Socket(b) => b.read_line().await,
        }
    }

    async fn stop(&mut self) -> Result<(), BackendError> {
        match self {
            Self::Stdio(b) => b.stop().await,
            Self::Socket(b) => b.stop().await,
        }
    }

    fn is_alive(&mut self) -> bool {
        match self {
            Self::Stdio(b) => b.is_alive(),
            Self::Socket(b) => b.is_alive(),
        }
    }
}
