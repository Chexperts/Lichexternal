//! Asynchronous child process engine backend communicating over standard I/O.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, warn};

use super::{BackendError, EngineBackend};

/// Engine backend driving an external executable process via `stdin` and `stdout`.
pub struct StdioBackend {
    command_name: String,
    child: Child,
    stdin: ChildStdin,
    stdout_reader: BufReader<ChildStdout>,
}

impl StdioBackend {
    /// Spawns an engine executable with the given arguments.
    pub fn spawn(command: &str, args: &[String]) -> Result<Self, BackendError> {
        debug!(command, ?args, "Spawning stdio engine process");

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW on Windows to prevent console popup if desired
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(BackendError::Io)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BackendError::HandshakeFailed(String::from("Failed to capture child stdin")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::HandshakeFailed(String::from("Failed to capture child stdout")))?;

        let stdout_reader = BufReader::new(stdout);

        Ok(Self {
            command_name: command.to_string(),
            child,
            stdin,
            stdout_reader,
        })
    }

    /// Kill the child process immediately.
    pub async fn kill(&mut self) -> Result<(), BackendError> {
        warn!(command = %self.command_name, "Killing engine child process");
        self.child.kill().await.map_err(BackendError::Io)
    }
}

impl EngineBackend for StdioBackend {
    async fn send_command(&mut self, cmd: &str) -> Result<(), BackendError> {
        let trimmed = cmd.trim();
        debug!(command = %trimmed, "Sending command to engine stdin");

        self.stdin
            .write_all(trimmed.as_bytes())
            .await
            .map_err(BackendError::Io)?;
        self.stdin.write_all(b"\n").await.map_err(BackendError::Io)?;
        self.stdin.flush().await.map_err(BackendError::Io)?;

        Ok(())
    }

    async fn read_line(&mut self) -> Result<Option<String>, BackendError> {
        let mut line = String::new();
        match self.stdout_reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF reached
                let status = self.child.try_wait().ok().flatten();
                if let Some(exit_status) = status {
                    Err(BackendError::ProcessExited(exit_status.code()))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => {
                // Remove trailing newline and carriage return
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(e) => Err(BackendError::Io(e)),
        }
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for StdioBackend {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
