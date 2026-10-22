//! Lichexternal Daemon (`lichexternald`)
//!
//! Unattended service connecting local/remote chess engines to the Lichess External Engine API.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("lichexternald v{}", lichexternal_core::VERSION);
    Ok(())
}
