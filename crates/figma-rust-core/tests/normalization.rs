use std::collections::BTreeMap;

use figma_rust_core::diagnostic::codes;
use figma_rust_core::ir::{
    AssetRoute, AxisSizing, ComponentResolution, Layout, Paint, Positioning,
};
use figma_rust_core::raw::{
    RawAsset, RawColor, RawComponent, RawConstraint, RawLiteral, RawVariable,
};
use figma_rust_core::{
    ComponentMapping, ComponentRegistry, normalize_bundle, normalize_bundle_with_registry,
    parse_and_normalize, parse_bundle,
};
use serde_json::{Value, json};

fn bundle_with_roots(roots: &Value) -> figma_rust_core::raw::ExtractionBundle {
    let value = json!({
        "schema_version": 2,
        "source": {
            "page_id": "0:1",
            "selected_node_ids": ["1:1"],
            "plugin_api_version": "1.135.0"
        },
        "roots": roots,
        "variables": [],
        "components": [],
        "assets": [],
        "future_bundle_field": { "kept": true }
    });
    parse_bundle(&value.to_string()).expect("test bundle must parse")
}

fn fixed_node(id: &str, kind: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "kind": kind,
        "size": {
            "width": 100.0,
            "height": 40.0,
            "horizontal": "FIXED",
            "vertical": "FIXED"
        }
    })
}

fn mode_context(collection_id: &str, mode_id: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(collection_id.to_owned(), mode_id.to_owned())])
}

#[test]
fn output_is_deterministic_and_unknown_fields_survive_parsing() {
    let bundle = bundle_with_roots(&json!([fixed_node("1:1", "RECTANGLE")]));
    assert_eq!(
        bundle.extensions["future_bundle_field"],
        json!({"kept": true})
    );

    let first = serde_json::to_vec_pretty(&normalize_bundle(&bundle)).expect("serialize first IR");
    let second =
        serde_json::to_vec_pretty(&normalize_bundle(&bundle)).expect("serialize second IR");
    assert_eq!(first, second);
}

#[test]
fn fill_is_parent_aware() {
    let mut root = fixed_node("1:1", "FRAME");
    root["layout"] = json!({"mode": "HORIZONTAL"});
    let mut child = fixed_node("1:2", "RECTANGLE");
    child["size"] = json!({
        "height": 40.0,
        "horizontal": "FILL",
        "vertical": "FIXED"
    });
    root["children"] = json!([child]);
    let valid = normalize_bundle(&bundle_with_roots(&json!([root])));
    assert!(
        !valid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == codes::AMBIGUOUS_FILL)
    );
    assert_eq!(
        valid.document.roots[0].children[0].size.horizontal.sizing,
        AxisSizing::Fill
    );

    let mut ambiguous = fixed_node("2:1", "RECTANGLE");
    ambiguous["size"]["horizontal"] = json!("FILL");
    let invalid = normalize_bundle(&bundle_with_roots(&json!([ambiguous])));
    let diagnostic = invalid
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == codes::AMBIGUOUS_FILL)
        .expect("root FILL must be diagnosed");
    assert_eq!(diagnostic.node_id.as_deref(), Some("2:1"));
}

#[test]
fn grid_absolute_and_min_max_contracts_are_normalized() {
    let mut grid = fixed_node("3:1", "FRAME");
    grid["layout"] = json!({
        "mode": "GRID",
        "grid": {
            "columns": [
                {"kind": "FIXED", "value": 80.0},
                {"kind": "FLEX", "value": 1.0}
            ],
            "rows": [{"kind": "HUG"}],
            "column_gap": 12.0,
            "row_gap": 4.0
        }
    });
    let mut grid_child = fixed_node("3:2", "RECTANGLE");
    grid_child["position"] = json!({
        "grid": {"row": 0, "column": 1, "row_span": 1, "column_span": 2}
    });
    grid["children"] = json!([grid_child]);

    let mut absolute = fixed_node("4:1", "FRAME");
    absolute["size"]["horizontal"] = json!("HUG");
    let mut absolute_child = fixed_node("4:2", "RECTANGLE");
    absolute_child["position"] = json!({
        "x": 12.0,
        "y": 18.0,
        "horizontal_constraint": "CENTER"
    });
    absolute["children"] = json!([absolute_child]);
    absolute["size"]["min_width"] = json!(200.0);
    absolute["size"]["max_width"] = json!(100.0);

    let mut baseline = fixed_node("5:1", "FRAME");
    baseline["layout"] = json!({
        "mode": "HORIZONTAL",
        "counter_alignment": "BASELINE"
    });
    let mut uniform_zero = fixed_node("6:1", "FRAME");
    uniform_zero["layout"] = json!({
        "mode": "GRID",
        "grid": {
            "columns": [
                {"kind": "FIXED", "value": -0.0},
                {"kind": "FIXED", "value": 0.0}
            ]
        }
    });

    let output = normalize_bundle(&bundle_with_roots(&json!([
        grid,
        absolute,
        baseline,
        uniform_zero
    ])));
    assert!(matches!(
        output.document.roots[0].layout,
        Layout::Grid { .. }
    ));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Runtime
    );
    assert!(matches!(
        output.document.roots[0].children[0].positioning,
        Positioning::Auto { grid: Some(_), .. }
    ));
    assert_eq!(
        output.document.roots[0].children[0].asset_decision.route,
        AssetRoute::Runtime
    );
    assert!(matches!(
        output.document.roots[1].layout,
        Layout::Absolute { .. }
    ));
    let Positioning::Absolute { x, y, .. } = output.document.roots[1].children[0].positioning
    else {
        panic!("child of a plain frame must use absolute positioning");
    };
    assert!((x - 12.0).abs() < f64::EPSILON);
    assert!((y - 18.0).abs() < f64::EPSILON);
    assert_eq!(
        output.document.roots[1].children[0].asset_decision.route,
        AssetRoute::Runtime
    );
    assert_eq!(
        output.document.roots[2].asset_decision.route,
        AssetRoute::Runtime
    );
    assert_eq!(
        output.document.roots[3].asset_decision.route,
        AssetRoute::Native
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::INVALID_CONSTRAINT && diagnostic.node_id.as_deref() == Some("4:1")
    }));
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::RUNTIME_FALLBACK && diagnostic.node_id.as_deref() == Some("3:1")
    }));
}

#[test]
fn hug_axes_retain_the_figma_measured_dimensions() {
    let mut node = fixed_node("4:5", "FRAME");
    node["size"] = json!({
        "width": 120.5,
        "height": 48.25,
        "horizontal": "HUG",
        "vertical": "HUG"
    });

    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    let document = serde_json::to_value(&output.document).expect("IR must serialize");

    assert_eq!(
        document["roots"][0]["size"]["horizontal"]["measured"],
        json!(120.5)
    );
    assert_eq!(
        document["roots"][0]["size"]["vertical"]["measured"],
        json!(48.25)
    );
    assert!(matches!(
        output.document.roots[0].size.horizontal.sizing,
        AxisSizing::Hug
    ));
    assert!(matches!(
        output.document.roots[0].size.vertical.sizing,
        AxisSizing::Hug
    ));
}

#[test]
fn constraints_under_fixed_parents_canonicalize_to_min() {
    let mut parent = fixed_node("4:10", "FRAME");
    parent["size"] = json!({
        "width": 28.0,
        "height": 28.0,
        "horizontal": "FIXED",
        "vertical": "FIXED"
    });
    let mut full_size_child = fixed_node("4:11", "FRAME");
    full_size_child["size"] = json!({
        "width": 28.0,
        "height": 28.0,
        "horizontal": "FIXED",
        "vertical": "FIXED"
    });
    full_size_child["position"] = json!({
        "x": 0.0,
        "y": 0.0,
        "horizontal_constraint": "SCALE",
        "vertical_constraint": "SCALE"
    });
    let mut scaled_child = fixed_node("4:12", "FRAME");
    scaled_child["position"] = json!({
        "x": 4.0,
        "y": 4.0,
        "horizontal_constraint": "SCALE",
        "vertical_constraint": "SCALE"
    });
    parent["children"] = json!([full_size_child, scaled_child]);

    let output = normalize_bundle(&bundle_with_roots(&json!([parent])));
    let Positioning::Absolute {
        horizontal_constraint,
        vertical_constraint,
        ..
    } = output.document.roots[0].children[0].positioning
    else {
        panic!("child of a plain frame must use absolute positioning");
    };
    assert_eq!(horizontal_constraint, RawConstraint::Min);
    assert_eq!(vertical_constraint, RawConstraint::Min);
    assert_eq!(
        output.document.roots[0].children[0].asset_decision.route,
        AssetRoute::Native
    );
    assert_eq!(
        output.document.roots[0].children[1].asset_decision.route,
        AssetRoute::Native
    );

    let mut hug_parent = fixed_node("4:20", "FRAME");
    hug_parent["size"]["horizontal"] = json!("HUG");
    let mut constrained_child = fixed_node("4:21", "FRAME");
    constrained_child["position"] = json!({
        "x": 4.0,
        "y": 4.0,
        "horizontal_constraint": "SCALE",
        "vertical_constraint": "MIN"
    });
    hug_parent["children"] = json!([constrained_child]);
    let hug_output = normalize_bundle(&bundle_with_roots(&json!([hug_parent])));
    assert_eq!(
        hug_output.document.roots[0].children[0]
            .asset_decision
            .route,
        AssetRoute::Runtime
    );
}

#[test]
fn unresolved_token_keeps_literal_fallback_and_token_identity() {
    let mut node = fixed_node("1:1", "RECTANGLE");
    node["style"] = json!({
        "fills": [{
            "kind": "SOLID",
            "color": {
                "literal": {"r": 0.1, "g": 0.2, "b": 0.3, "a": 1.0},
                "token_id": "missing-token"
            }
        }]
    });
    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == codes::UNRESOLVED_TOKEN)
        .expect("missing token must be diagnosed");
    assert_eq!(diagnostic.node_id.as_deref(), Some("1:1"));

    let figma_rust_core::ir::Paint::Solid { color } = &output.document.roots[0].style.fills[0]
    else {
        panic!("solid paint must stay solid");
    };
    assert_eq!(
        color.token.as_ref().map(|token| token.id.as_str()),
        Some("missing-token")
    );
    assert!((color.fallback.r - 0.1).abs() < f64::EPSILON);
}

#[test]
fn unmapped_instance_retains_structural_fallback() {
    let mut instance = fixed_node("1:1", "INSTANCE");
    instance["component"] = json!({
        "role": "INSTANCE",
        "component_key": "button-primary",
        "component_set_key": "button",
        "variants": {"Size": "Large"},
        "properties": {"Label": {"kind": "TEXT", "value": "Save"}},
        "overrides": []
    });
    instance["children"] = json!([fixed_node("1:2", "TEXT")]);
    let bundle = bundle_with_roots(&json!([instance]));
    let output = normalize_bundle(&bundle);
    let metadata = output.document.roots[0]
        .component
        .as_ref()
        .expect("component metadata");
    assert_eq!(metadata.resolution, ComponentResolution::StructuralFallback);
    assert_eq!(output.document.roots[0].children.len(), 1);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::UNMAPPED_COMPONENT && diagnostic.node_id.as_deref() == Some("1:1")
    }));

    let registry = ComponentRegistry {
        schema_version: 1,
        mappings: BTreeMap::from([(
            "button-primary".to_owned(),
            ComponentMapping {
                id: "app.button".to_owned(),
                component_key: "button-primary".to_owned(),
                component_set_key: Some("button".to_owned()),
            },
        )]),
    };
    let mapped = normalize_bundle_with_registry(&bundle, &registry);
    assert_eq!(
        mapped.document.roots[0]
            .component
            .as_ref()
            .map(|metadata| &metadata.resolution),
        Some(&ComponentResolution::Mapped {
            mapping_id: "app.button".to_owned()
        })
    );
}

#[test]
fn instance_without_metadata_is_diagnosed_and_children_survive() {
    let mut instance = fixed_node("8:1", "INSTANCE");
    instance["children"] = json!([fixed_node("8:2", "RECTANGLE")]);
    let output = normalize_bundle(&bundle_with_roots(&json!([instance])));
    assert_eq!(output.document.roots[0].children.len(), 1);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::UNMAPPED_COMPONENT && diagnostic.node_id.as_deref() == Some("8:1")
    }));
}

#[test]
fn component_role_mismatch_is_not_treated_as_a_definition() {
    let mut instance = fixed_node("8:3", "INSTANCE");
    instance["component"] = json!({
        "role": "COMPONENT",
        "component_key": "broken-instance"
    });
    let output = normalize_bundle(&bundle_with_roots(&json!([instance])));
    assert_eq!(
        output.document.roots[0]
            .component
            .as_ref()
            .map(|metadata| &metadata.resolution),
        Some(&ComponentResolution::StructuralFallback)
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::COMPONENT_METADATA_MISMATCH
            && diagnostic.node_id.as_deref() == Some("8:3")
    }));
}

#[test]
fn missing_image_asset_is_an_error_and_explicit_raster_decision() {
    let mut node = fixed_node("7:1", "IMAGE");
    node["style"] = json!({
        "fills": [{"kind": "IMAGE", "asset_id": "missing-image", "scale_mode": "FIT"}]
    });
    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Raster
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::MISSING_ASSET && diagnostic.node_id.as_deref() == Some("7:1")
    }));
}

#[test]
fn unordered_metadata_is_canonical_and_variable_values_are_validated() {
    let mut first = bundle_with_roots(&json!([fixed_node("1:1", "RECTANGLE")]));
    first.variables = vec![
        RawVariable {
            id: "z".to_owned(),
            name: "Z".to_owned(),
            collection_id: "theme".to_owned(),
            mode_id: "light".to_owned(),
            mode_context: mode_context("theme", "light"),
            source_node_id: None,
            value: RawLiteral::Number(2.0),
        },
        RawVariable {
            id: "a".to_owned(),
            name: "A".to_owned(),
            collection_id: "theme".to_owned(),
            mode_id: "light".to_owned(),
            mode_context: mode_context("theme", "light"),
            source_node_id: Some("1:1".to_owned()),
            value: RawLiteral::Color(RawColor {
                r: 2.0,
                g: 0.25,
                b: 0.5,
                a: 1.0,
            }),
        },
        RawVariable {
            id: "z".to_owned(),
            name: "Z alternate".to_owned(),
            collection_id: "theme".to_owned(),
            mode_id: "dark".to_owned(),
            mode_context: mode_context("theme", "dark"),
            source_node_id: None,
            value: RawLiteral::Number(3.0),
        },
    ];
    first.components = vec![
        RawComponent {
            key: "z".to_owned(),
            name: "Z".to_owned(),
            set_key: None,
            property_definitions: BTreeMap::new(),
        },
        RawComponent {
            key: "a".to_owned(),
            name: "A".to_owned(),
            set_key: None,
            property_definitions: BTreeMap::new(),
        },
    ];
    first.assets = vec![
        RawAsset {
            id: "z".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/png".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::new(),
            payload_base64: None,
        },
        RawAsset {
            id: "a".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::new(),
            payload_base64: None,
        },
    ];
    let mut second = first.clone();
    second.variables.reverse();
    second.components.reverse();
    second.assets.reverse();

    let first_output = normalize_bundle(&first);
    let second_output = normalize_bundle(&second);
    assert_eq!(first_output, second_output);
    let RawLiteral::Color(color) = &first_output.document.variables[0].value else {
        panic!("first canonical variable must be the color token");
    };
    assert!((color.r - 1.0).abs() < f64::EPSILON);
    assert!(first_output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::INVALID_NUMBER && diagnostic.node_id.as_deref() == Some("1:1")
    }));
    assert!(
        !first_output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == codes::DUPLICATE_METADATA_ID)
    );
    assert_eq!(
        first_output
            .document
            .variables
            .iter()
            .filter_map(|variable| variable.token.mode_id.as_deref())
            .collect::<Vec<_>>(),
        vec!["light", "light", "dark"]
    );
}

#[test]
fn multi_mode_tokens_select_the_full_consumer_context() {
    let mut node = fixed_node("10:1", "RECTANGLE");
    node["style"] = json!({
        "fills": [{
            "kind": "SOLID",
            "color": {
                "literal": {"r": 0.9, "g": 0.8, "b": 0.7, "a": 1.0},
                "token_id": "surface",
                "mode_context": {"primitive": "dark", "theme": "light"}
            }
        }]
    });
    let mut bundle = bundle_with_roots(&json!([node]));
    bundle.variables = vec![
        RawVariable {
            id: "surface".to_owned(),
            name: "Surface".to_owned(),
            collection_id: "theme".to_owned(),
            mode_id: "light".to_owned(),
            mode_context: BTreeMap::from([
                ("primitive".to_owned(), "light".to_owned()),
                ("theme".to_owned(), "light".to_owned()),
            ]),
            source_node_id: Some("10:2".to_owned()),
            value: RawLiteral::Color(RawColor {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            }),
        },
        RawVariable {
            id: "surface".to_owned(),
            name: "Surface".to_owned(),
            collection_id: "theme".to_owned(),
            mode_id: "light".to_owned(),
            mode_context: BTreeMap::from([
                ("primitive".to_owned(), "dark".to_owned()),
                ("theme".to_owned(), "light".to_owned()),
            ]),
            source_node_id: Some("10:1".to_owned()),
            value: RawLiteral::Color(RawColor {
                r: 0.9,
                g: 0.8,
                b: 0.7,
                a: 1.0,
            }),
        },
    ];

    let output = normalize_bundle(&bundle);
    let Paint::Solid { color } = &output.document.roots[0].style.fills[0] else {
        panic!("fixture must normalize to a solid fill");
    };
    assert_eq!(
        color
            .token
            .as_ref()
            .and_then(|token| token.mode_id.as_deref()),
        Some("light")
    );
    assert_eq!(
        color.mode_context.get("primitive").map(String::as_str),
        Some("dark")
    );
    assert!((color.fallback.r - 0.9).abs() < f64::EPSILON);
    assert_eq!(output.document.variables.len(), 2);
    assert!(!output.has_errors());
}

#[test]
fn modeled_numeric_bindings_preserve_tokens_and_fallbacks() {
    let output = parse_and_normalize(include_str!("fixtures/basic.raw.json"))
        .expect("numeric binding fixture must parse");
    assert!(!output.has_errors());
    let root = &output.document.roots[0];
    let Layout::Stack { gap, padding, .. } = &root.layout else {
        panic!("numeric binding fixture must normalize to a stack");
    };
    assert_eq!(
        gap.token.as_ref().map(|token| token.id.as_str()),
        Some("number.layout")
    );
    assert!((gap.fallback - 8.0).abs() < f64::EPSILON);
    assert_eq!(
        padding.top.token.as_ref().map(|token| token.id.as_str()),
        Some("number.layout")
    );
    assert!((root.style.radii.top_left.fallback - 8.0).abs() < f64::EPSILON);
    assert_eq!(
        root.style
            .radii
            .bottom_left
            .token
            .as_ref()
            .map(|token| token.id.as_str()),
        Some("radius.bottom-left")
    );
    assert!((root.style.stroke_widths.top.fallback - 1.0).abs() < f64::EPSILON);
    assert_eq!(
        root.style
            .stroke_widths
            .left
            .token
            .as_ref()
            .map(|token| token.id.as_str()),
        Some("stroke.left")
    );
}

#[test]
fn schema_one_is_rejected_after_the_explicit_v2_change() {
    let output = parse_and_normalize(include_str!("fixtures/basic.v1.raw.json"))
        .expect("the exact v1 fixture must remain readable for an explicit version diagnostic");
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == codes::SCHEMA_VERSION)
    );
}

#[test]
fn schema_two_rejects_legacy_scalar_bound_values() {
    let relabeled = include_str!("fixtures/basic.v1.raw.json")
        .replace("\"schema_version\": 1", "\"schema_version\": 2");
    assert!(parse_bundle(&relabeled).is_err());

    let mut bare_color = serde_json::from_str::<Value>(include_str!("fixtures/basic.raw.json"))
        .expect("v2 fixture must be JSON");
    bare_color["roots"][0]["style"]["fills"][0]["color"] =
        bare_color["roots"][0]["style"]["fills"][0]["color"]["literal"].take();
    assert!(parse_bundle(&bare_color.to_string()).is_err());
}

#[test]
fn schema_two_rejects_duplicate_known_fields() {
    let basic = include_str!("fixtures/basic.raw.json");
    let duplicate_schema = basic.replace(
        "\"schema_version\": 2",
        "\"schema_version\": 1, \"schema_version\": 2",
    );
    assert!(parse_bundle(&duplicate_schema).is_err());

    let duplicate_gap = basic.replacen("\"gap\": {", "\"gap\": 8.0, \"gap\": {", 1);
    assert!(parse_bundle(&duplicate_gap).is_err());

    let duplicate_literal =
        basic.replacen("\"literal\": 8.0", "\"literal\": 7.0, \"literal\": 8.0", 1);
    assert!(parse_bundle(&duplicate_literal).is_err());
}

#[test]
fn negative_zero_is_canonicalized_in_node_numbers() {
    let mut bundle = bundle_with_roots(&json!([fixed_node("6:1", "RECTANGLE")]));
    bundle.roots[0].size.width = Some(-0.0);
    bundle.roots[0].position.transform.matrix[4] = -0.0;
    let output = normalize_bundle(&bundle);
    let AxisSizing::Fixed(width) = output.document.roots[0].size.horizontal.sizing else {
        panic!("fixture width must stay fixed");
    };
    assert_eq!(width.to_bits(), 0.0_f64.to_bits());
    let Positioning::Auto { transform, .. } = output.document.roots[0].positioning else {
        panic!("root must remain auto-positioned");
    };
    assert_eq!(transform.matrix[4].to_bits(), 0.0_f64.to_bits());
}

#[test]
fn multiple_stroke_paints_choose_svg_with_a_node_diagnostic() {
    let mut node = fixed_node("6:2", "RECTANGLE");
    node["style"] = json!({
        "strokes": [
            {
                "kind": "SOLID",
                "color": {"literal": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}}
            },
            {
                "kind": "SOLID",
                "color": {"literal": {"r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0}}
            }
        ]
    });
    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Svg
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::SVG_FALLBACK && diagnostic.node_id.as_deref() == Some("6:2")
    }));
}

#[test]
fn unsupported_effect_is_node_scoped_and_forces_raster() {
    let mut node = fixed_node("9:9", "RECTANGLE");
    node["style"] = json!({"effects": [{"kind": "LAYER_BLUR", "radius": 8.0}]});
    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Raster
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::UNSUPPORTED_EFFECT && diagnostic.node_id.as_deref() == Some("9:9")
    }));
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::RASTER_FALLBACK && diagnostic.node_id.as_deref() == Some("9:9")
    }));
}

#[test]
fn text_runs_use_utf16_offsets_without_splitting_unicode() {
    let mut node = fixed_node("1:1", "TEXT");
    node["text"] = json!({
        "characters": "A😀B",
        "runs": [{"start_utf16": 1, "end_utf16": 3, "style": {}}]
    });
    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    let text = output.document.roots[0].text.as_ref().expect("text IR");
    assert_eq!(text.runs[0].text, "😀");
    assert!(!output.has_errors());
}

#[test]
fn inert_leaf_and_opaque_container_pass_through_stay_native() {
    let mut opaque = fixed_node("11:1", "GROUP");
    opaque["style"] = json!({"blend_mode": "PASS_THROUGH"});
    opaque["children"] = json!([fixed_node("11:1:1", "RECTANGLE")]);
    let mut translucent = fixed_node("11:2", "GROUP");
    translucent["opacity"] = json!(0.5);
    translucent["style"] = json!({"blend_mode": "PASS_THROUGH"});
    translucent["children"] = json!([fixed_node("11:2:1", "RECTANGLE")]);
    let mut rectangle = fixed_node("11:3", "RECTANGLE");
    rectangle["style"] = json!({"blend_mode": "PASS_THROUGH"});
    let mut frame = fixed_node("11:4", "FRAME");
    frame["style"] = json!({"blend_mode": "PASS_THROUGH"});
    frame["children"] = json!([fixed_node("11:4:1", "RECTANGLE")]);

    let output = normalize_bundle(&bundle_with_roots(&json!([
        opaque,
        translucent,
        rectangle,
        frame
    ])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Native
    );
    assert_eq!(
        output.document.roots[1].asset_decision.route,
        AssetRoute::Runtime
    );
    assert_eq!(
        output.document.roots[2].asset_decision.route,
        AssetRoute::Native
    );
    assert_eq!(
        output.document.roots[3].asset_decision.route,
        AssetRoute::Native
    );
}

#[test]
fn stroke_alignment_without_strokes_stays_native() {
    let mut no_stroke = fixed_node("11:10", "TEXT");
    no_stroke["style"] = json!({"stroke_align": "OUTSIDE", "strokes": []});
    let mut outside_stroke = fixed_node("11:11", "RECTANGLE");
    outside_stroke["style"] = json!({
        "stroke_align": "OUTSIDE",
        "strokes": [{
            "kind": "SOLID",
            "color": {"literal": {"r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0}}
        }]
    });

    let output = normalize_bundle(&bundle_with_roots(&json!([no_stroke, outside_stroke])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Native
    );
    assert_eq!(
        output.document.roots[1].asset_decision.route,
        AssetRoute::Runtime
    );
}

#[test]
fn nonzero_letter_spacing_uses_svg_fallback() {
    let mut node = fixed_node("12:1", "TEXT");
    node["text"] = json!({
        "characters": "Tracked",
        "runs": [{
            "start_utf16": 0,
            "end_utf16": 7,
            "style": {"letter_spacing": 1.5}
        }]
    });

    let output = normalize_bundle(&bundle_with_roots(&json!([node])));
    assert_eq!(
        output.document.roots[0].asset_decision.route,
        AssetRoute::Svg
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == codes::SVG_FALLBACK
            && diagnostic.node_id.as_deref() == Some("12:1")
            && diagnostic.property_path.as_deref() == Some("asset_decision")
    }));
}
