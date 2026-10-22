//! Cross-platform local socket engine backend using `interprocess`.

use interprocess::local_socket::tokio::Stream;
use interprocess::local_socket::traits::tokio::Stream as _;
use interprocess::local_socket::ToFsName;
#[cfg(windows)]
use interprocess::local_socket::ToNsName;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tracing::debug;

use super::{BackendError, EngineBackend};

/// Engine backend connecting to a persistent background engine daemon via Unix Domain Socket
/// (Linux/macOS/Android) or Named Pipe (Windows).
pub struct SocketBackend {
    endpoint: String,
    write_half: WriteHalf<Stream>,
    read_half: BufReader<ReadHalf<Stream>>,
    alive: bool,
}

impl SocketBackend {
    /// Connect to a local engine socket or named pipe.
    pub async fn connect(endpoint: &str) -> Result<Self, BackendError> {
        debug!(endpoint, "Connecting to socket engine backend");

        #[cfg(windows)]
        let name = if endpoint.starts_with(r"\\.\pipe\") || endpoint.starts_with('@') {
            endpoint
                .to_ns_name::<interprocess::local_socket::GenericNamespaced>()
                .map_err(|e| BackendError::HandshakeFailed(e.to_string()))?
        } else {
            endpoint
                .to_fs_name::<interprocess::local_socket::GenericFilePath>()
                .map_err(|e| BackendError::HandshakeFailed(e.to_string()))?
        };

        #[cfg(not(windows))]
        let name = endpoint
            .to_fs_name::<interprocess::local_socket::GenericFilePath>()
            .map_err(|e| BackendError::HandshakeFailed(e.to_string()))?;

        let stream = Stream::connect(name).await.map_err(BackendError::Io)?;
        let (reader, writer) = tokio::io::split(stream);

        Ok(Self {
            endpoint: endpoint.to_string(),
            write_half: writer,
            read_half: BufReader::new(reader),
            alive: true,
        })
    }
}

impl EngineBackend for SocketBackend {
    async fn send_command(&mut self, cmd: &str) -> Result<(), BackendError> {
        let trimmed = cmd.trim();
        debug!(endpoint = %self.endpoint, command = %trimmed, "Sending command to socket engine");

        if !self.alive {
            return Err(BackendError::Disconnected);
        }

        self.write_half
            .write_all(trimmed.as_bytes())
            .await
            .map_err(|e| {
                self.alive = false;
                BackendError::Io(e)
            })?;
        self.write_half
            .write_all(b"\n")
            .await
            .map_err(|e| {
                self.alive = false;
                BackendError::Io(e)
            })?;
        self.write_half
            .flush()
            .await
            .map_err(|e| {
                self.alive = false;
                BackendError::Io(e)
            })?;

        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, BackendError> {
        if !self.alive {
            return Ok(None);
        }

        let mut line = String::new();
        match self.read_half.read_line(&mut line).await {
            Ok(0) => {
                self.alive = false;
                Ok(None)
            }
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => {
                self.alive = false;
                Err(BackendError::Io(e))
            }
        }
    }

    fn is_alive(&mut self) -> bool {
        self.alive
    }
}
