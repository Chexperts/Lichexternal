//! IPC client connecting to a running `lichexternald` daemon.

use std::path::Path;
use interprocess::local_socket::tokio::Stream;
use interprocess::local_socket::traits::tokio::Stream as _;
use interprocess::local_socket::ToFsName;
#[cfg(windows)]
use interprocess::local_socket::ToNsName;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use lichexternal_daemon::ipc::{IpcRequest, IpcResponse};

/// Transmit a request to the daemon IPC socket and await the response.
pub async fn call_daemon_ipc(socket_path: &Path, request: &IpcRequest) -> Result<IpcResponse, anyhow::Error> {
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

    let stream = match Stream::connect(name).await {
        Ok(s) => s,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to connect to daemon at '{}': {e}\nIs lichexternald running?",
                socket_path.display()
            ));
        }
    };

    let (reader, mut writer) = tokio::io::split(stream);
    let mut buf_reader = BufReader::new(reader);

    let mut req_str = serde_json::to_string(request)?;
    req_str.push('\n');
    writer.write_all(req_str.as_bytes()).await?;
    writer.flush().await?;

    let mut response_line = String::new();
    if buf_reader.read_line(&mut response_line).await? == 0 {
        return Err(anyhow::anyhow!("Daemon closed connection unexpectedly"));
    }

    let response: IpcResponse = serde_json::from_str(response_line.trim())?;
    Ok(response)
}
