//! Lichexternal Daemon (`lichexternald`)
//!
//! Unattended background service bridging local/remote chess engines to the Lichess External Engine API.

use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use lichexternal_core::config::Config;
use lichexternal_daemon::ipc::run_ipc_server;
use lichexternal_daemon::supervisor::SupervisorPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Collect command line arguments
    let mut args = std::env::args().skip(1);
    let mut config_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => {
                config_path = args.next().map(PathBuf::from);
            }
            "-v" | "--version" => {
                println!("lichexternald v{}", lichexternal_core::VERSION);
                return Ok(());
            }
            "-h" | "--help" => {
                println!("Usage: lichexternald [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -c, --config <PATH>  Path to configuration file");
                println!("  -v, --version        Print version information");
                println!("  -h, --help           Print help");
                return Ok(());
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let config_file = match config_path {
        Some(path) => path,
        None => match Config::default_config_path() {
            Some(path) => path,
            None => {
                eprintln!("Error: Could not determine standard config directory for your OS.");
                std::process::exit(1);
            }
        },
    };

    if !config_file.exists() {
        eprintln!(
            "Configuration file not found at: {}\nCreate one based on config.example.toml or specify --config <path>",
            config_file.display()
        );
        std::process::exit(1);
    }

    let config = Config::load_from_path(&config_file)?;

    // Initialize logging
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    info!(
        version = %lichexternal_core::VERSION,
        config = %config_file.display(),
        engines = %config.engines.len(),
        "Starting Lichexternal Daemon"
    );

    let socket_path = config
        .socket_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(Config::default_socket_path);

    let (pool, shutdown_rx) = SupervisorPool::new(&config);

    let pool_clone = pool.clone();
    let (ipc_shutdown_tx, mut ipc_shutdown_rx) = tokio::sync::watch::channel(false);
    let ipc_shutdown_trigger = ipc_shutdown_tx.clone();
    let ipc_server_shutdown_rx = ipc_shutdown_rx.clone();

    let ipc_task = tokio::spawn(async move {
        if let Err(e) = run_ipc_server(
            socket_path,
            pool_clone,
            ipc_shutdown_trigger,
            ipc_server_shutdown_rx,
        )
        .await
        {
            error!(error = %e, "IPC server error");
        }
    });

    // Wait for Ctrl+C or IPC shutdown signal
    tokio::select! {
        res = tokio::signal::ctrl_c() => {
            match res {
                Ok(()) => info!("Ctrl+C received; shutting down daemon gracefully"),
                Err(e) => error!(error = %e, "Failed to listen for Ctrl+C"),
            }
        }
        _ = ipc_shutdown_rx.changed() => {
            info!("IPC shutdown signal received; shutting down daemon gracefully");
        }
    }

    // Signal all workers and IPC server to stop
    pool.shutdown();
    let _ = ipc_shutdown_tx.send(true);
    let _ = ipc_task.await;

    // Await all session tasks
    while !*shutdown_rx.borrow() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    info!("Lichexternal Daemon stopped cleanly");
    Ok(())
}
