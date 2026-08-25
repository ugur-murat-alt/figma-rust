use figma_rust_codegen::{SourceMap, generate};
use figma_rust_core::{ir::DesignDocument, parse_and_normalize};
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenEnvelope {
    document: DesignDocument,
}

fn basic_document() -> DesignDocument {
    let parsed = serde_json::from_str::<GoldenEnvelope>(include_str!(
        "../../figma-rust-core/tests/fixtures/basic.ir.json"
    ));
    match parsed {
        Ok(envelope) => envelope.document,
        Err(error) => panic!("core golden IR must deserialize: {error}"),
    }
}

fn all_hidden_document() -> DesignDocument {
    let mut document = basic_document();
    document.roots[0].visible = false;
    document
}

fn real_group_document() -> DesignDocument {
    let normalized =
        parse_and_normalize(include_str!("../../../fixtures/real-figma/extraction.json"));
    match normalized {
        Ok(output) if !output.has_errors() => output.document,
        Ok(output) => panic!(
            "real Figma fixture must normalize without errors: {:?}",
            output.diagnostics
        ),
        Err(error) => panic!("real Figma fixture must parse: {error}"),
    }
}

#[test]
fn generated_rust_matches_golden_and_is_byte_deterministic() {
    let first = match generate(&basic_document()) {
        Ok(output) => output,
        Err(error) => panic!("basic document must generate: {error}"),
    };
    let second = match generate(&basic_document()) {
        Ok(output) => output,
        Err(error) => panic!("second generation must succeed: {error}"),
    };

    assert_eq!(first, second);
    assert_eq!(first.rust, include_str!("fixtures/basic.gpui.rs"));
}

#[test]
fn all_hidden_rust_has_no_unused_trait_imports() {
    let output = match generate(&all_hidden_document()) {
        Ok(output) => output,
        Err(error) => panic!("all-hidden document must generate: {error}"),
    };

    assert_eq!(output.rust, include_str!("fixtures/all-hidden.gpui.rs"));
    assert!(output.source_map.nodes.is_empty());
}

#[test]
fn real_group_artifacts_match_fresh_generation() {
    let output = match generate(&real_group_document()) {
        Ok(output) => output,
        Err(error) => panic!("real Figma fixture must generate: {error}"),
    };
    let expected_map = serde_json::from_str::<SourceMap>(include_str!(
        "../../../fixtures/real-figma/generated/source-map.json"
    ));

    assert_eq!(
        output.rust,
        include_str!("../../../fixtures/real-figma/generated/generated.rs")
    );
    match expected_map {
        Ok(expected_map) => assert_eq!(output.source_map, expected_map),
        Err(error) => panic!("real Figma source map must deserialize: {error}"),
    }
}
