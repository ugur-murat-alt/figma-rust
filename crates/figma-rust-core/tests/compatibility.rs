use figma_rust_core::{
    SchemaCompatibility, extraction_fingerprint, parse_and_normalize, parse_bundle,
    schema_compatibility,
};

const FIRST: &str = r#"{
  "schema_version": 2,
  "source": {
    "extractor": "figma-rust-plugin",
    "extractor_version": "0.3.0",
    "plugin_api_version": "1.135.0",
    "file_key": "private-fixture-key",
    "page_id": "0:1",
    "selected_node_ids": ["19:1"]
  },
  "roots": [{"id":"19:1","name":"Fingerprint","kind":"RECTANGLE"}],
  "variables": [],
  "components": [],
  "assets": [],
  "extraction_diagnostics": [],
  "extraction_manifest": {
    "capabilities": ["typed-text-layout", "bounded-traversal"],
    "traversal": {
      "chunk_node_limit": 2000,
      "node_count": 1,
      "complete": true,
      "chunks": [],
      "roots": [{"id":"19:1","node_count":1,"complete":true}]
    }
  }
}"#;

const REORDERED: &str = r#"{
  "assets": [],
  "components": [],
  "extraction_diagnostics": [],
  "extraction_manifest": {
    "traversal": {
      "roots": [{"complete":true,"node_count":1,"id":"19:1"}],
      "chunks": [],
      "complete": true,
      "node_count": 1,
      "chunk_node_limit": 2000
    },
    "capabilities": ["bounded-traversal", "typed-text-layout"]
  },
  "roots": [{"kind":"RECTANGLE","name":"Fingerprint","id":"19:1"}],
  "schema_version": 2,
  "source": {
    "selected_node_ids": ["19:1"],
    "page_id": "0:1",
    "file_key": "private-fixture-key",
    "plugin_api_version": "1.135.0",
    "extractor_version": "0.3.0",
    "extractor": "figma-rust-plugin"
  },
  "variables": []
}"#;

#[test]
fn fingerprint_is_canonical_and_privacy_safe() -> Result<(), Box<dyn std::error::Error>> {
    let first = extraction_fingerprint(&parse_bundle(FIRST)?)?;
    let reordered = extraction_fingerprint(&parse_bundle(REORDERED)?)?;

    assert_eq!(first, reordered);
    assert_eq!(first.version, 1);
    assert_eq!(first.algorithm, "SHA-256");
    assert_eq!(first.value.len(), 64);
    assert!(
        first
            .value
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    assert!(!first.value.contains("private-fixture-key"));
    Ok(())
}

#[test]
fn capability_and_semantic_changes_have_distinct_fingerprints()
-> Result<(), Box<dyn std::error::Error>> {
    let bundle = parse_bundle(FIRST)?;
    let baseline = extraction_fingerprint(&bundle)?;

    let mut capability_change = bundle.clone();
    capability_change
        .extraction_manifest
        .as_mut()
        .expect("fixture manifest")
        .capabilities
        .push("extended-mode-overrides".to_owned());
    assert_ne!(baseline, extraction_fingerprint(&capability_change)?);

    let mut semantic_change = bundle;
    semantic_change.roots[0].name = "Fingerprint changed".to_owned();
    assert_ne!(baseline, extraction_fingerprint(&semantic_change)?);

    let mut extractor_change = parse_bundle(FIRST)?;
    extractor_change.source.extractor_version = Some("0.3.1".to_owned());
    assert_ne!(baseline, extraction_fingerprint(&extractor_change)?);

    let mut api_change = parse_bundle(FIRST)?;
    api_change.source.plugin_api_version = "1.136.0".to_owned();
    assert_ne!(baseline, extraction_fingerprint(&api_change)?);
    Ok(())
}

#[test]
fn compatibility_policy_distinguishes_current_legacy_and_future() {
    assert_eq!(schema_compatibility(2), SchemaCompatibility::Current);
    assert_eq!(
        schema_compatibility(1),
        SchemaCompatibility::LegacyDiagnosticOnly
    );
    assert_eq!(
        schema_compatibility(3),
        SchemaCompatibility::UnsupportedDiagnosticOnly
    );

    for version in [1, 3] {
        let source = FIRST.replacen(
            "\"schema_version\": 2",
            &format!("\"schema_version\": {version}"),
            1,
        );
        let output =
            parse_and_normalize(&source).expect("diagnostic-only schema must stay parseable");
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == figma_rust_core::diagnostic::codes::SCHEMA_VERSION
        }));
    }
}
