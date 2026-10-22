//! Lichexternal CLI (`lichexternal`)
//!
//! Command-line management, diagnostic, and profile registration tool.

mod ipc_client;

use std::path::PathBuf;
use clap::{Parser, Subcommand};
use tabled::{Table, Tabled};

use lichexternal_core::config::Config;
use lichexternal_daemon::backend::{EngineBackend, StdioBackend};
use lichexternal_daemon::ipc::{IpcRequest, IpcResponse};
use lichexternal_core::uci::{parse_uci_line, UciOutputLine};

#[derive(Parser)]
#[command(
    name = "lichexternal",
    version = lichexternal_core::VERSION,
    about = "Management CLI for Lichess external engine bridges and daemon diagnostics"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect real-time operational status of engines in the running daemon
    Status {
        /// Path to daemon IPC socket
        #[arg(short, long)]
        socket: Option<PathBuf>,
    },
    /// Ping running daemon to test connectivity
    Ping {
        /// Path to daemon IPC socket
        #[arg(short, long)]
        socket: Option<PathBuf>,
    },
    /// Signal the running daemon to shut down cleanly
    Stop {
        /// Path to daemon IPC socket
        #[arg(short, long)]
        socket: Option<PathBuf>,
    },
    /// Probe a local engine binary to verify UCI compliance
    TestEngine {
        /// Executable command or path
        command: String,
        /// Arguments passed to the engine process
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[derive(Tabled)]
struct EngineStatusRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Backend")]
    backend: String,
    #[tabled(rename = "Status")]
    state: String,
    #[tabled(rename = "Active Work")]
    active_work: String,
    #[tabled(rename = "Depth")]
    depth: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status { socket } => {
            let socket_path = socket.unwrap_or_else(Config::default_socket_path);
            let resp = ipc_client::call_daemon_ipc(&socket_path, &IpcRequest::Status).await?;

            match resp {
                IpcResponse::Status { version, engines } => {
                    println!("Lichexternal Daemon v{version}");
                    if engines.is_empty() {
                        println!("No engines configured or running.");
                    } else {
                        let rows: Vec<EngineStatusRow> = engines
                            .into_iter()
                            .map(|e| EngineStatusRow {
                                id: e.id,
                                name: e.name,
                                backend: format!("{:?}", e.backend_type).to_lowercase(),
                                state: e.state,
                                active_work: e.active_work_id.unwrap_or_else(|| "-".into()),
                                depth: e.current_depth.map(|d| d.to_string()).unwrap_or_else(|| "-".into()),
                            })
                            .collect();

                        let table = Table::new(rows).to_string();
                        println!("\n{table}");
                    }
                }
                other => {
                    eprintln!("Unexpected daemon response: {other:?}");
                }
            }
        }
        Commands::Ping { socket } => {
            let socket_path = socket.unwrap_or_else(Config::default_socket_path);
            let resp = ipc_client::call_daemon_ipc(&socket_path, &IpcRequest::Ping).await?;
            match resp {
                IpcResponse::Pong => println!("Daemon is online (pong)"),
                other => eprintln!("Unexpected response: {other:?}"),
            }
        }
        Commands::Stop { socket } => {
            let socket_path = socket.unwrap_or_else(Config::default_socket_path);
            let resp = ipc_client::call_daemon_ipc(&socket_path, &IpcRequest::Shutdown).await?;
            match resp {
                IpcResponse::Ok { message } => println!("{message}"),
                other => eprintln!("Unexpected response: {other:?}"),
            }
        }
        Commands::TestEngine { command, args } => {
            println!("Testing engine UCI compliance: {} {:?}", command, args);

            let mut backend = StdioBackend::spawn(&command, &args)?;
            println!("Spawning subprocess... [OK]");

            backend.send_command("uci").await?;
            println!("Sending 'uci'... [OK]");

            let mut name = None;
            let mut author = None;
            let mut options = Vec::new();

            loop {
                match backend.read_line().await? {
                    Some(line) => match parse_uci_line(&line) {
                        UciOutputLine::UciOk => break,
                        UciOutputLine::IdName(n) => name = Some(n.to_string()),
                        UciOutputLine::IdAuthor(a) => author = Some(a.to_string()),
                        UciOutputLine::Option(opt) => {
                            if let Some(opt_name) = opt.strip_prefix("name ") {
                                options.push(opt_name.split_whitespace().next().unwrap_or_default().to_string());
                            }
                        }
                        _ => {}
                    },
                    None => {
                        eprintln!("Engine process exited before sending 'uciok'");
                        std::process::exit(1);
                    }
                }
            }

            println!("Received 'uciok'... [OK]");

            backend.send_command("isready").await?;
            loop {
                match backend.read_line().await? {
                    Some(line) if parse_uci_line(&line) == UciOutputLine::ReadyOk => break,
                    Some(_) => continue,
                    None => {
                        eprintln!("Engine process exited before sending 'readyok'");
                        std::process::exit(1);
                    }
                }
            }

            println!("Received 'readyok'... [OK]");

            println!("\nEngine Verification Succeeded:");
            println!("  Name:    {}", name.unwrap_or_else(|| "Unknown".into()));
            println!("  Author:  {}", author.unwrap_or_else(|| "Unknown".into()));
            println!("  Options: {} supported", options.len());
            for opt in &options {
                println!("    - {opt}");
            }

            let _ = backend.send_command("quit").await;
        }
    }

    Ok(())
}
