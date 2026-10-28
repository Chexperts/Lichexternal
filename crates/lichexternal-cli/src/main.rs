//! Lichexternal CLI (`lichexternal`)
//!
//! Command-line management, diagnostic, and profile registration tool.

mod credentials;
mod ipc_client;
mod oauth;
mod rest_client;

use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tabled::settings::Style;
use tabled::{Table, Tabled};

use lichexternal_core::config::Config;
use lichexternal_core::models::{CreateEngineRequest, UciVariant};
use lichexternal_core::uci::{parse_uci_line, UciOutputLine};
use lichexternal_daemon::backend::{EngineBackend, StdioBackend};
use lichexternal_daemon::ipc::{IpcRequest, IpcResponse};
use rest_client::LichessRestClient;

#[derive(Parser)]
#[command(
    name = "lichext",
    version = lichexternal_core::VERSION,
    about = "Management CLI for Lichess external engine bridges and daemon diagnostics"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with Lichess via OAuth 2.0 PKCE or manage saved credentials
    Auth {
        #[command(subcommand)]
        subcmd: AuthSubcommand,
    },
    /// Inspect real-time operational status of engines in the running daemon
    Status {
        /// Path to daemon IPC socket
        #[arg(short, long)]
        socket: Option<PathBuf>,
        /// Output status as JSON (machine-readable)
        #[arg(long)]
        json: bool,
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
    /// Manage remote external engine registrations on Lichess
    Engine {
        #[command(subcommand)]
        subcmd: EngineSubcommand,
    },
}

#[derive(Subcommand)]
enum AuthSubcommand {
    /// Log in interactively with Lichess using OAuth 2.0 PKCE
    Login {
        /// Custom OAuth client ID (defaults to 'chexperts-lichexternal')
        #[arg(long, default_value = oauth::DEFAULT_CLIENT_ID)]
        client_id: String,
        /// Base URL for Lichess
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
    /// Check current authentication status and user account
    Status {
        /// Base URL for Lichess
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
    /// Log out and delete locally saved credentials
    Logout,
}

#[derive(Subcommand)]
enum EngineSubcommand {
    /// List all external engines registered for your account
    List {
        /// Personal access token (or LICHESS_API_TOKEN env)
        #[arg(short, long)]
        token: Option<String>,
        /// Base URL for Lichess REST API
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
        /// Output results as JSON (machine-readable)
        #[arg(long)]
        json: bool,
    },
    /// Register a new external engine profile
    Register {
        /// Display name for the engine on Lichess
        #[arg(short, long)]
        name: String,
        /// Maximum calculation threads allowed
        #[arg(short = 't', long, default_value_t = 1)]
        max_threads: u32,
        /// Maximum hash table memory in MiB
        #[arg(short = 'm', long, default_value_t = 64)]
        max_hash: u32,
        /// Supported chess variants (e.g. chess, antichess, atomic)
        #[arg(short, long, value_delimiter = ',', default_value = "chess")]
        variants: Vec<String>,
        /// Personal access token (or LICHESS_API_TOKEN env)
        #[arg(short, long)]
        token: Option<String>,
        /// Base URL for Lichess REST API
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
    /// Inspect an individual engine profile
    Get {
        /// Engine profile ID
        id: String,
        /// Personal access token (or LICHESS_API_TOKEN env)
        #[arg(short, long)]
        token: Option<String>,
        /// Base URL for Lichess REST API
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
    /// Delete an engine registration from Lichess
    Delete {
        /// Engine profile ID
        id: String,
        /// Personal access token (or LICHESS_API_TOKEN env)
        #[arg(short, long)]
        token: Option<String>,
        /// Base URL for Lichess REST API
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
    /// Update an existing engine registration on Lichess
    Update {
        /// Engine profile ID
        id: String,
        /// Updated display name
        #[arg(short, long)]
        name: Option<String>,
        /// Updated max threads
        #[arg(short = 't', long)]
        max_threads: Option<u32>,
        /// Updated max hash in MiB
        #[arg(short = 'm', long)]
        max_hash: Option<u32>,
        /// Personal access token (or LICHESS_API_TOKEN env)
        #[arg(short, long)]
        token: Option<String>,
        /// Base URL for Lichess REST API
        #[arg(long, default_value = "https://lichess.org")]
        base_url: String,
    },
}

#[derive(Tabled)]
struct EngineStatusRow {
    #[tabled(rename = "#")]
    index: usize,
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

#[derive(Tabled)]
struct LichessEngineRow {
    #[tabled(rename = "#")]
    index: usize,
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Max Threads")]
    max_threads: u32,
    #[tabled(rename = "Max Hash (MiB)")]
    max_hash: u32,
    #[tabled(rename = "Variants")]
    variants: String,
    #[tabled(rename = "Secret Available")]
    has_secret: String,
}

fn resolve_token(token_arg: Option<String>) -> Result<String, anyhow::Error> {
    if let Some(t) = token_arg {
        return Ok(t);
    }

    if let Ok(t) = std::env::var("LICHESS_API_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t);
        }
    }

    if let Ok(Some(creds)) = credentials::load_credentials() {
        if !creds.access_token.trim().is_empty() {
            return Ok(creds.access_token);
        }
    }

    if let Some(cfg_path) = Config::default_config_path() {
        if cfg_path.exists() {
            if let Ok(config) = Config::load_from_path(&cfg_path) {
                if let Some(t) = config.lichess_token {
                    return Ok(t);
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Lichess authentication required.\n\
         Please run `lichexternal auth login` to authenticate interactively via OAuth,\n\
         or provide a token via --token <TOKEN> / LICHESS_API_TOKEN environment variable."
    ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth { subcmd } => match subcmd {
            AuthSubcommand::Login {
                client_id,
                base_url,
            } => {
                let (token_resp, username) = oauth::login_pkce(&base_url, &client_id).await?;
                let creds = credentials::Credentials {
                    access_token: token_resp.access_token,
                    username: username.clone(),
                    scope: token_resp.scope,
                };
                let saved_path = credentials::save_credentials(&creds)?;

                println!("\n✓ Successfully authenticated with Lichess!");
                if let Some(user) = username {
                    println!("  Logged in as: @{user}");
                }
                println!("  Credentials stored at: {}", saved_path.display());
            }
            AuthSubcommand::Status { base_url } => {
                if let Ok(Some(creds)) = credentials::load_credentials() {
                    println!("Stored Credentials Found:");
                    if let Some(user) = &creds.username {
                        println!("  User:   @{user}");
                    }
                    if let Some(scope) = &creds.scope {
                        println!("  Scopes: {scope}");
                    }
                    if let Some(path) = credentials::credentials_path() {
                        println!("  File:   {}", path.display());
                    }

                    print!("  Token verification: ");
                    match oauth::fetch_account_username(&base_url, &creds.access_token).await {
                        Ok(username) => {
                            println!("✓ Valid (active as @{username})");
                        }
                        Err(e) => {
                            println!("✗ Invalid or expired ({e})");
                        }
                    }
                } else if let Ok(t) = std::env::var("LICHESS_API_TOKEN") {
                    println!("Authentication: via LICHESS_API_TOKEN environment variable");
                    print!("  Token verification: ");
                    match oauth::fetch_account_username(&base_url, &t).await {
                        Ok(username) => {
                            println!("✓ Valid (active as @{username})");
                        }
                        Err(e) => {
                            println!("✗ Invalid or expired ({e})");
                        }
                    }
                } else {
                    println!("Not currently authenticated.");
                    println!("Run `lichexternal auth login` to sign in with your Lichess account.");
                }
            }
            AuthSubcommand::Logout => {
                if credentials::delete_credentials()? {
                    println!("✓ Successfully logged out. Credentials removed.");
                } else {
                    println!("No stored credentials found to remove.");
                }
            }
        },
        Commands::Status { socket, json } => {
            let socket_path = socket.unwrap_or_else(Config::default_socket_path);
            let resp = ipc_client::call_daemon_ipc(&socket_path, &IpcRequest::Status).await?;

            match resp {
                IpcResponse::Status { version, engines } => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&engines)?);
                    } else {
                        println!("Lichexternal Daemon v{version}");
                        if engines.is_empty() {
                            println!("No engines configured or running.");
                        } else {
                            let rows: Vec<EngineStatusRow> = engines
                                .into_iter()
                                .enumerate()
                                .map(|(idx, e)| EngineStatusRow {
                                    index: idx,
                                    id: e.id,
                                    name: e.name,
                                    backend: format!("{:?}", e.backend_type).to_lowercase(),
                                    state: e.state,
                                    active_work: e.active_work_id.unwrap_or_else(|| "-".into()),
                                    depth: e
                                        .current_depth
                                        .map(|d| d.to_string())
                                        .unwrap_or_else(|| "-".into()),
                                })
                                .collect();

                            let mut table = Table::new(rows);
                            table.with(Style::rounded());
                            println!("\n{table}");
                        }
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
                            options.push(opt.name.to_string());
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
        Commands::Engine { subcmd } => match subcmd {
            EngineSubcommand::List {
                token,
                base_url,
                json,
            } => {
                let token = resolve_token(token)?;
                let client = LichessRestClient::new(base_url, token);
                let engines = client.list_engines().await?;

                if json {
                    println!("{}", serde_json::to_string_pretty(&engines)?);
                } else if engines.is_empty() {
                    println!("No external engines registered on this account.");
                } else {
                    let rows: Vec<LichessEngineRow> = engines
                        .into_iter()
                        .enumerate()
                        .map(|(idx, e)| LichessEngineRow {
                            index: idx,
                            id: e.id,
                            name: e.name,
                            max_threads: e.max_threads,
                            max_hash: e.max_hash,
                            variants: e
                                .variants
                                .iter()
                                .map(|v| v.to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                            has_secret: if e.secret.is_some() { "yes" } else { "no" }.to_string(),
                        })
                        .collect();

                    let mut table = Table::new(rows);
                    table.with(Style::rounded());
                    println!("\n{table}");
                }
            }
            EngineSubcommand::Register {
                name,
                max_threads,
                max_hash,
                variants,
                token,
                base_url,
            } => {
                let token = resolve_token(token)?;
                let client = LichessRestClient::new(base_url, token);

                let parsed_variants: Result<Vec<UciVariant>, _> = variants
                    .iter()
                    .map(|v| serde_json::from_str(&format!("\"{v}\"")))
                    .collect();
                let parsed_variants =
                    parsed_variants.map_err(|e| anyhow::anyhow!("Invalid variant: {e}"))?;

                let req = CreateEngineRequest {
                    name,
                    max_threads,
                    max_hash,
                    default_options: BTreeMap::new(),
                    variants: parsed_variants,
                };

                let profile = client.create_engine(&req).await?;
                println!("Engine profile successfully registered on Lichess!\n");
                println!("  ID:              {}", profile.id);
                println!("  Name:            {}", profile.name);
                println!("  Max Threads:     {}", profile.max_threads);
                println!("  Max Hash:        {} MiB", profile.max_hash);
                if let Some(sec) = &profile.secret {
                    println!("  Provider Secret: {}", sec);
                    println!("\nIMPORTANT: Add this provider_secret to your config.toml under [[engines]]!");
                }
            }
            EngineSubcommand::Get {
                id,
                token,
                base_url,
            } => {
                let token = resolve_token(token)?;
                let client = LichessRestClient::new(base_url, token);
                let profile = client.get_engine(&id).await?;

                println!("Engine Profile Details:");
                println!("  ID:          {}", profile.id);
                println!("  Name:        {}", profile.name);
                println!("  Max Threads: {}", profile.max_threads);
                println!("  Max Hash:    {} MiB", profile.max_hash);
                println!("  Variants:    {:?}", profile.variants);
                if let Some(sec) = profile.secret {
                    println!("  Secret:      {}", sec);
                }
            }
            EngineSubcommand::Delete {
                id,
                token,
                base_url,
            } => {
                let token = resolve_token(token)?;
                let client = LichessRestClient::new(base_url, token);
                client.delete_engine(&id).await?;
                println!("Engine profile '{id}' successfully deleted from Lichess.");
            }
            EngineSubcommand::Update {
                id,
                name,
                max_threads,
                max_hash,
                token,
                base_url,
            } => {
                let token = resolve_token(token)?;
                let client = LichessRestClient::new(base_url, token);
                let req = lichexternal_core::models::UpdateEngineRequest {
                    name,
                    max_threads,
                    max_hash,
                    default_options: None,
                    variants: None,
                };
                let profile = client.update_engine(&id, &req).await?;
                println!("Engine profile '{id}' successfully updated.");
                println!("  Name:        {}", profile.name);
                println!("  Max Threads: {}", profile.max_threads);
                println!("  Max Hash:    {} MiB", profile.max_hash);
            }
        },
    }

    Ok(())
}
