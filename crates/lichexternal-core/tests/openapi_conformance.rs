//! Upstream OpenAPI schema conformance tests for Lichess External Engine models.

use lichexternal_core::models::{AcquireResponse, EngineProfile, UciVariant};
use std::fs;
use std::path::PathBuf;

fn get_spec_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LICHESS_OPENAPI_SPEC") {
        let p = PathBuf::from(path);
        if p.exists() {
            return p.parent().map(PathBuf::from);
        }
    }
    None
}

#[test]
fn test_official_list_external_engines_example_deserialization() {
    let Some(spec_dir) = get_spec_dir() else {
        eprintln!("Skipping OpenAPI validation test (LICHESS_OPENAPI_SPEC not set or not found)");
        return;
    };

    let example_path = spec_dir.join("examples/externalEngine-listExternalEngines.json.yaml");
    assert!(
        example_path.exists(),
        "Example file not found at: {}",
        example_path.display()
    );

    let content = fs::read_to_string(&example_path).expect("Failed to read example file");
    let yaml: serde_json::Value =
        serde_yaml::from_str(&content).expect("Failed to parse YAML example");
    let json_value = yaml.get("value").expect("Missing 'value' field in example");

    let engines: Vec<EngineProfile> = serde_json::from_value(json_value.clone())
        .expect("Failed to deserialize EngineProfile list");

    assert_eq!(engines.len(), 1);
    let engine = &engines[0];
    assert_eq!(engine.id, "eei_Bfh2VIM1m3rh");
    assert_eq!(engine.name, "Stockfish 17");
    assert_eq!(engine.user_id.as_deref(), Some("bobby"));
    assert_eq!(engine.max_threads, 8);
    assert_eq!(engine.max_hash, 2048);
    assert_eq!(engine.variants, vec![UciVariant::Chess]);
    assert_eq!(
        engine.client_secret.as_deref(),
        Some("ees_AKzFjHT0F7CBLvxt")
    );
}

#[test]
fn test_official_acquire_analysis_request_example_deserialization() {
    let Some(spec_dir) = get_spec_dir() else {
        eprintln!("Skipping OpenAPI validation test (LICHESS_OPENAPI_SPEC not set or not found)");
        return;
    };

    let example_path = spec_dir.join("examples/externalEngine-acquireAnalysisRequest.json.yaml");
    assert!(
        example_path.exists(),
        "Example file not found at: {}",
        example_path.display()
    );

    let content = fs::read_to_string(&example_path).expect("Failed to read example file");
    let yaml: serde_json::Value =
        serde_yaml::from_str(&content).expect("Failed to parse YAML example");
    let json_value = yaml.get("value").expect("Missing 'value' field in example");

    let resp: AcquireResponse =
        serde_json::from_value(json_value.clone()).expect("Failed to deserialize AcquireResponse");

    assert_eq!(resp.id, "dfWmlfdy0caStBY1");
    assert_eq!(resp.work.session_id, "1");
    assert_eq!(resp.work.threads, 1);
    assert_eq!(resp.work.hash, 2048);
    assert_eq!(resp.work.variant, UciVariant::Chess);
    assert_eq!(
        resp.work.moves,
        vec!["e2e4".to_string(), "g8f6".to_string()]
    );

    let engine = resp
        .engine
        .expect("Engine profile must be present in example");
    assert_eq!(engine.id, "eei_Bfh2VIM1m3rh");
    assert_eq!(engine.name, "Stockfish 17.1");
    assert_eq!(engine.user_id.as_deref(), Some("bobby"));
    assert_eq!(
        engine.client_secret.as_deref(),
        Some("ees_AKzFjHT0F7CBLvxt")
    );
}
