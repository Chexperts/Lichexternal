//! Lichess External Engine Broker work models.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use super::engine::EngineProfile;
use super::variant::UciVariant;

/// Search limit constraints requested by Lichess for an evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct SearchLimit {
    /// Maximum search depth in plies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Search time limit in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movetime: Option<u64>,
    /// Node count budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
}

impl SearchLimit {
    /// Formats the search limits into UCI `go` command arguments.
    #[must_use]
    pub fn to_uci_go_args(&self) -> String {
        let mut parts = Vec::new();
        if let Some(depth) = self.depth {
            parts.push(alloc::format!("depth {depth}"));
        }
        if let Some(movetime) = self.movetime {
            parts.push(alloc::format!("movetime {movetime}"));
        }
        if let Some(nodes) = self.nodes {
            parts.push(alloc::format!("nodes {nodes}"));
        }

        if parts.is_empty() {
            String::from("infinite")
        } else {
            parts.join(" ")
        }
    }
}

/// Evaluation work requested by Lichess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct WorkRequest {
    /// Unique analysis session ID on Lichess.
    #[serde(default)]
    pub session_id: String,
    /// Number of engine calculation threads requested.
    pub threads: u32,
    /// Transposition table / hash size in MiB.
    pub hash: u32,
    /// Number of principal variations to calculate concurrently.
    #[serde(default = "default_multi_pv")]
    pub multi_pv: u32,
    /// Chess variant for the analysis.
    #[serde(default = "default_variant")]
    pub variant: UciVariant,
    /// Custom initial position in FEN notation (if not standard starting position).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_fen: Option<String>,
    /// Move sequence from the initial position in UCI notation (e.g. `["e2e4", "e7e5"]`).
    #[serde(default)]
    pub moves: Vec<String>,
    /// Search limits (depth, movetime, nodes).
    #[serde(flatten)]
    pub search_limit: SearchLimit,
}

const fn default_multi_pv() -> u32 {
    1
}

const fn default_variant() -> UciVariant {
    UciVariant::Chess
}

/// Request sent to `POST /api/external-engine/work` to acquire pending jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct AcquireRequest {
    /// Secret authentication key assigned by Lichess when registering the engine.
    pub provider_secret: String,
}

impl AcquireRequest {
    /// Create a new acquire request for long-polling.
    #[must_use]
    pub const fn new(provider_secret: String) -> Self {
        Self { provider_secret }
    }
}

/// Response returned by the broker when a work item is assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct AcquireResponse {
    /// Unique job identifier (used to stream results back to `POST /api/external-engine/work/{id}`).
    pub id: String,
    /// Calculation details and position parameters.
    pub work: WorkRequest,
    /// Engine profile details known to Lichess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_request_serialization() {
        let req = AcquireRequest::new("secret_abc_123".into());
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"providerSecret":"secret_abc_123"}"#);
    }

    #[test]
    fn test_work_request_deserialization() {
        let payload = r#"{
            "sessionId": "sess_999",
            "threads": 4,
            "hash": 128,
            "multiPv": 2,
            "variant": "chess",
            "moves": ["e2e4", "c7c5"],
            "depth": 25,
            "movetime": 3000
        }"#;

        let work: WorkRequest = serde_json::from_str(payload).unwrap();
        assert_eq!(work.session_id, "sess_999");
        assert_eq!(work.threads, 4);
        assert_eq!(work.hash, 128);
        assert_eq!(work.multi_pv, 2);
        assert_eq!(work.variant, UciVariant::Chess);
        assert_eq!(work.moves, vec!["e2e4", "c7c5"]);
        assert_eq!(work.initial_fen, None);
        assert_eq!(work.search_limit.depth, Some(25));
        assert_eq!(work.search_limit.movetime, Some(3000));
        assert_eq!(work.search_limit.nodes, None);
        assert_eq!(work.search_limit.to_uci_go_args(), "depth 25 movetime 3000");
    }

    #[test]
    fn test_acquire_response_deserialization() {
        let payload = r#"{
            "id": "work-abc-xyz",
            "work": {
                "threads": 8,
                "hash": 512,
                "variant": "crazyhouse",
                "initialFen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                "moves": []
            }
        }"#;

        let resp: AcquireResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(resp.id, "work-abc-xyz");
        assert_eq!(resp.work.threads, 8);
        assert_eq!(resp.work.hash, 512);
        assert_eq!(resp.work.multi_pv, 1);
        assert_eq!(resp.work.variant, UciVariant::Crazyhouse);
        assert!(resp.work.initial_fen.is_some());
        assert_eq!(resp.work.search_limit.to_uci_go_args(), "infinite");
    }
}
