//! Lichexternal CLI (`lichexternal`)
//!
//! Management CLI for engine profiles and local daemon diagnostics.

fn main() -> anyhow::Result<()> {
    println!("lichexternal v{}", lichexternal_core::VERSION);
    Ok(())
}
