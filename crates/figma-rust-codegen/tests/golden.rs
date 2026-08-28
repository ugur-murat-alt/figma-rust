use figma_rust_codegen::{SourceMap, generate};
use figma_rust_core::{Diagnostic, ir::DesignDocument, parse_and_normalize};
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

fn asset_fallback_document() -> DesignDocument {
    let normalized = parse_and_normalize(include_str!(
        "../../../fixtures/asset-fallback/extraction.json"
    ));
    match normalized {
        Ok(output) if !output.has_errors() => output.document,
        Ok(output) => panic!(
            "asset fallback fixture must normalize without errors: {:?}",
            output.diagnostics
        ),
        Err(error) => panic!("asset fallback fixture must parse: {error}"),
    }
}

fn child_alignment_document() -> DesignDocument {
    let normalized = parse_and_normalize(include_str!(
        "../../../fixtures/child-alignment/extraction.json"
    ));
    match normalized {
        Ok(output) if !output.has_errors() => output.document,
        Ok(output) => panic!(
            "child alignment fixture must normalize without errors: {:?}",
            output.diagnostics
        ),
        Err(error) => panic!("child alignment fixture must parse: {error}"),
    }
}

fn image_crop_document() -> DesignDocument {
    let normalized =
        parse_and_normalize(include_str!("../../../fixtures/image-crop/extraction.json"));
    match normalized {
        Ok(output) if !output.has_errors() => output.document,
        Ok(output) => panic!(
            "image crop fixture must normalize without errors: {:?}",
            output.diagnostics
        ),
        Err(error) => panic!("image crop fixture must parse: {error}"),
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
    let document = real_group_document();
    let output = match generate(&document) {
        Ok(output) => output,
        Err(error) => panic!("real Figma fixture must generate: {error}"),
    };
    let expected_map = serde_json::from_str::<SourceMap>(include_str!(
        "../../../fixtures/real-figma/generated/source-map.json"
    ));
    let expected_ir = serde_json::from_str::<DesignDocument>(include_str!(
        "../../../fixtures/real-figma/generated/ir.json"
    ));
    let expected_diagnostics = serde_json::from_str::<Vec<Diagnostic>>(include_str!(
        "../../../fixtures/real-figma/generated/diagnostics.json"
    ));

    assert_eq!(
        output.rust,
        include_str!("../../../fixtures/real-figma/generated/generated.rs")
    );
    match expected_map {
        Ok(expected_map) => assert_eq!(output.source_map, expected_map),
        Err(error) => panic!("real Figma source map must deserialize: {error}"),
    }
    match expected_ir {
        Ok(expected_ir) => assert_eq!(document, expected_ir),
        Err(error) => panic!("real Figma IR must deserialize: {error}"),
    }
    let normalized =
        parse_and_normalize(include_str!("../../../fixtures/real-figma/extraction.json"))
            .unwrap_or_else(|error| panic!("real Figma fixture must parse: {error}"));
    match expected_diagnostics {
        Ok(expected_diagnostics) => assert_eq!(normalized.diagnostics, expected_diagnostics),
        Err(error) => panic!("real Figma diagnostics must deserialize: {error}"),
    }
}

#[test]
fn asset_fallback_rust_and_source_map_match_fresh_generation() {
    let output = match generate(&asset_fallback_document()) {
        Ok(output) => output,
        Err(error) => panic!("asset fallback fixture must generate: {error}"),
    };
    let expected_map = serde_json::from_str::<SourceMap>(include_str!(
        "../../../fixtures/asset-fallback/generated/source-map.json"
    ));

    assert_eq!(
        output.rust,
        include_str!("../../../fixtures/asset-fallback/generated/generated.rs")
    );
    match expected_map {
        Ok(expected_map) => assert_eq!(output.source_map, expected_map),
        Err(error) => panic!("asset fallback source map must deserialize: {error}"),
    }
}

#[test]
fn child_alignment_rust_and_source_map_match_fresh_generation() {
    let output = match generate(&child_alignment_document()) {
        Ok(output) => output,
        Err(error) => panic!("child alignment fixture must generate: {error}"),
    };
    let expected_map = serde_json::from_str::<SourceMap>(include_str!(
        "../../../fixtures/child-alignment/generated/source-map.json"
    ));

    assert_eq!(
        output.rust,
        include_str!("../../../fixtures/child-alignment/generated/generated.rs")
    );
    match expected_map {
        Ok(expected_map) => assert_eq!(output.source_map, expected_map),
        Err(error) => panic!("child alignment source map must deserialize: {error}"),
    }
}

#[test]
fn image_crop_rust_and_source_map_match_fresh_generation() {
    let output = generate(&image_crop_document()).expect("image crop fixture must generate");
    let expected_map = serde_json::from_str::<SourceMap>(include_str!(
        "../../../fixtures/image-crop/generated/source-map.json"
    ));

    assert_eq!(
        output.rust,
        include_str!("../../../fixtures/image-crop/generated/generated.rs")
    );
    assert!(output.rust.contains("object_fit(gpui::ObjectFit::Contain)"));
    assert!(output.rust.contains("object_fit(gpui::ObjectFit::Cover)"));
    assert!(output.rust.contains(".left(gpui::px(-50f32))"));
    assert!(output.rust.contains(".top(gpui::px(-10f32))"));
    match expected_map {
        Ok(expected_map) => assert_eq!(output.source_map, expected_map),
        Err(error) => panic!("image crop source map must deserialize: {error}"),
    }
}
