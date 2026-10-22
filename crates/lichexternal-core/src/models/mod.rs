//! Lichess External Engine API and Broker models.

pub mod engine;
pub mod variant;
pub mod work;

pub use engine::{CreateEngineRequest, EngineMetadata, EngineProfile, UpdateEngineRequest};
pub use variant::UciVariant;
pub use work::{AcquireRequest, AcquireResponse, SearchLimit, WorkRequest};
