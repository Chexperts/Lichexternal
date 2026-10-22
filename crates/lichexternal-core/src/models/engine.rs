//! Lichess External Engine profile management models.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use super::variant::UciVariant;

/// External engine registration profile stored on Lichess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct EngineProfile {
    /// Unique identifier assigned by Lichess.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Maximum number of calculation threads permitted.
    pub max_threads: u32,
    /// Maximum hash table / transposition memory in MiB.
    pub max_hash: u32,
    /// Default custom UCI options.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_options: BTreeMap<String, String>,
    /// Chess variants supported by this engine.
    #[serde(default)]
    pub variants: Vec<UciVariant>,
    /// Provider secret used to authenticate work polling on `engine.lichess.ovh`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Owner user ID on Lichess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Request body for registering a new external engine profile via `POST /api/external-engine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct CreateEngineRequest {
    /// Human-readable display name.
    pub name: String,
    /// Maximum threads advertised to Lichess.
    pub max_threads: u32,
    /// Maximum hash size in MiB advertised to Lichess.
    pub max_hash: u32,
    /// Default options sent to the engine.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_options: BTreeMap<String, String>,
    /// Supported chess variants.
    pub variants: Vec<UciVariant>,
}

/// Request body for updating an existing engine profile via `PUT /api/external-engine/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct UpdateEngineRequest {
    /// Updated display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Updated max threads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_threads: Option<u32>,
    /// Updated max hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hash: Option<u32>,
    /// Updated default options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_options: Option<BTreeMap<String, String>>,
    /// Updated supported variants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<UciVariant>>,
}

/// Local engine discovery metadata probed via UCI `uci` handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct EngineMetadata {
    /// Engine name parsed from `id name <name>`.
    pub name: String,
    /// Engine author parsed from `id author <author>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Supported UCI option names detected during handshake.
    #[serde(default)]
    pub available_options: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_profile_deserialization() {
        let payload = r#"{
            "id": "eng_123",
            "name": "Stockfish 19",
            "maxThreads": 16,
            "maxHash": 4096,
            "variants": ["chess", "atomic"],
            "secret": "prov_sec_xyz"
        }"#;

        let profile: EngineProfile = serde_json::from_str(payload).unwrap();
        assert_eq!(profile.id, "eng_123");
        assert_eq!(profile.name, "Stockfish 19");
        assert_eq!(profile.max_threads, 16);
        assert_eq!(profile.max_hash, 4096);
        assert_eq!(
            profile.variants,
            vec![UciVariant::Chess, UciVariant::Atomic]
        );
        assert_eq!(profile.secret.as_deref(), Some("prov_sec_xyz"));
    }

    #[test]
    fn test_create_engine_request_serialization() {
        let req = CreateEngineRequest {
            name: "Maia 3".into(),
            max_threads: 4,
            max_hash: 1024,
            default_options: BTreeMap::new(),
            variants: vec![UciVariant::Chess],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""name":"Maia 3""#));
        assert!(json.contains(r#""maxThreads":4"#));
        assert!(json.contains(r#""variants":["chess"]"#));
    }
}
