//! Lichexternal Core: Pure, portable protocol models and UCI parser.
//!
//! Designed to compile cleanly on `wasm32-unknown-unknown`, Windows, Linux, macOS, and Android.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

/// Current version of the lichexternal-core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod models;
pub use models::*;
