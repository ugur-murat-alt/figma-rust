use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::authoring::{
    AUTHORING_SCHEMA_VERSION, AuthoringDocument, BindingTargetKind, CodeOwnership, ComponentRole,
    DesignNode, DesignToken, DesignValue, Effect, Fill, GridTrack, InstanceValue, LayoutFlow,
    Paint, PositionSpec, SizingRule, SyncPolicy, TokenKind, TokenScope, TokenValue,
};

const LOWERING_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    pub message: String,
}

impl DesignDiagnostic {
    fn error(
        code: impl Into<String>,
        entity_id: Option<&str>,
        property: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: code.into(),
            entity_id: entity_id.map(str::to_owned),
            property: property.map(str::to_owned),
            message: message.into(),
        }
    }

    fn warning(
        code: impl Into<String>,
        entity_id: Option<&str>,
        property: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
            entity_id: entity_id.map(str::to_owned),
            property: property.map(str::to_owned),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<DesignDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentFingerprint {
    pub algorithm: String,
    pub schema_version: u32,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub document_id: String,
    pub revision: u64,
    pub roots: usize,
    pub nodes: usize,
    pub tokens: usize,
    pub components: usize,
    pub code_bindings: usize,
    pub token_scopes: BTreeMap<String, usize>,
    pub component_roles: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweringManifest {
    pub manifest_version: u32,
    pub document_id: String,
    pub revision: u64,
    pub fingerprint: DocumentFingerprint,
    pub summary: DocumentSummary,
    pub valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub bound_components: Vec<String>,
    pub unbound_components: Vec<String>,
    pub generated_presentation_bindings: Vec<String>,
    pub handwritten_presentation_bindings: Vec<String>,
    pub reference_only_behavior_bindings: Vec<String>,
}

#[must_use]
pub fn validate_document(document: &AuthoringDocument) -> ValidationReport {
    let mut diagnostics = Vec::new();

    if document.schema_version != AUTHORING_SCHEMA_VERSION {
        diagnostics.push(DesignDiagnostic::error(
            "GD-SCHEMA-001",
            Some(&document.document_id),
            Some("schema_version"),
            format!(
                "authoring schema {} is unsupported; expected {}",
                document.schema_version, AUTHORING_SCHEMA_VERSION
            ),
        ));
    }
    validate_identifier(
        &document.document_id,
        "GD-ID-001",
        "document_id",
        &document.document_id,
        &mut diagnostics,
    );
    if document.name.trim().is_empty() {
        diagnostics.push(DesignDiagnostic::error(
            "GD-DOC-001",
            Some(&document.document_id),
            Some("name"),
            "document name must not be empty",
        ));
    }

    validate_nodes(document, &mut diagnostics);
    validate_tokens(document, &mut diagnostics);
    validate_components(document, &mut diagnostics);
    validate_code_bindings(document, &mut diagnostics);

    diagnostics.sort_by(|left, right| {
        (
            left.severity,
            &left.code,
            left.entity_id.as_deref(),
            left.property.as_deref(),
            &left.message,
        )
            .cmp(&(
                right.severity,
                &right.code,
                right.entity_id.as_deref(),
                right.property.as_deref(),
                &right.message,
            ))
    });

    let error_count = diagnostics
        .iter()
        .filter(|item| item.severity == DiagnosticSeverity::Error)
        .count();
    let warning_count = diagnostics.len().saturating_sub(error_count);
    ValidationReport {
        valid: error_count == 0,
        error_count,
        warning_count,
        diagnostics,
    }
}

fn validate_nodes(document: &AuthoringDocument, diagnostics: &mut Vec<DesignDiagnostic>) {
    let mut roots = BTreeSet::new();
    for root_id in &document.roots {
        if !roots.insert(root_id) {
            diagnostics.push(DesignDiagnostic::error(
                "GD-TREE-001",
                Some(root_id),
                Some("roots"),
                "root appears more than once",
            ));
        }
        match document.nodes.get(root_id) {
            Some(root) if root.parent.is_none() => {}
            Some(_) => diagnostics.push(DesignDiagnostic::error(
                "GD-TREE-002",
                Some(root_id),
                Some("parent"),
                "a root node must not have a parent",
            )),
            None => diagnostics.push(DesignDiagnostic::error(
                "GD-TREE-003",
                Some(root_id),
                Some("roots"),
                "root references a missing node",
            )),
        }
    }

    for (key, node) in &document.nodes {
        validate_identifier(key, "GD-ID-002", "nodes key", key, diagnostics);
        validate_identifier(&node.id, "GD-ID-003", "node.id", key, diagnostics);
        if key != &node.id {
            diagnostics.push(DesignDiagnostic::error(
                "GD-ID-004",
                Some(key),
                Some("id"),
                format!(
                    "node map key {key:?} does not match embedded id {:?}",
                    node.id
                ),
            ));
        }
        if node.name.trim().is_empty() {
            diagnostics.push(DesignDiagnostic::warning(
                "GD-NODE-001",
                Some(key),
                Some("name"),
                "node name is empty; diagnostics and studio navigation will be harder to read",
            ));
        }
        if node.parent.is_none() && !roots.contains(key) {
            diagnostics.push(DesignDiagnostic::error(
                "GD-TREE-004",
                Some(key),
                Some("parent"),
                "parentless node is not listed as a document root",
            ));
        }
        if let Some(parent_id) = &node.parent {
            match document.nodes.get(parent_id) {
                Some(parent) if parent.children.iter().any(|child| child == key) => {}
                Some(_) => diagnostics.push(DesignDiagnostic::error(
                    "GD-TREE-005",
                    Some(key),
                    Some("parent"),
                    format!("parent {parent_id:?} does not list this node as a child"),
                )),
                None => diagnostics.push(DesignDiagnostic::error(
                    "GD-TREE-006",
                    Some(key),
                    Some("parent"),
                    format!("parent {parent_id:?} does not exist"),
                )),
            }
        }

        let mut children = BTreeSet::new();
        for child_id in &node.children {
            if !children.insert(child_id) {
                diagnostics.push(DesignDiagnostic::error(
                    "GD-TREE-007",
                    Some(key),
                    Some("children"),
                    format!("child {child_id:?} appears more than once"),
                ));
            }
            match document.nodes.get(child_id) {
                Some(child) if child.parent.as_deref() == Some(key.as_str()) => {}
                Some(child) => diagnostics.push(DesignDiagnostic::error(
                    "GD-TREE-008",
                    Some(child_id),
                    Some("parent"),
                    format!(
                        "child parent {:?} does not match containing node {key:?}",
                        child.parent
                    ),
                )),
                None => diagnostics.push(DesignDiagnostic::error(
                    "GD-TREE-009",
                    Some(key),
                    Some("children"),
                    format!("child {child_id:?} does not exist"),
                )),
            }
        }

        validate_node_tokens(document, node, diagnostics);
        validate_node_values(document, node, diagnostics);
        validate_visual_numbers(node, diagnostics);

        if let Some(instance) = &node.component {
            if !document.components.contains_key(&instance.component_id) {
                diagnostics.push(DesignDiagnostic::error(
                    "GD-COMPONENT-001",
                    Some(key),
                    Some("component.component_id"),
                    format!("component {:?} does not exist", instance.component_id),
                ));
            }
            for (property, value) in &instance.properties {
                if let InstanceValue::Token(token_id) = value {
                    validate_token_reference(
                        document,
                        token_id,
                        key,
                        &format!("component.properties.{property}"),
                        diagnostics,
                    );
                }
            }
            for (slot, node_ids) in &instance.slots {
                for node_id in node_ids {
                    if !document.nodes.contains_key(node_id) {
                        diagnostics.push(DesignDiagnostic::error(
                            "GD-COMPONENT-002",
                            Some(key),
                            Some(&format!("component.slots.{slot}")),
                            format!("slot references missing node {node_id:?}"),
                        ));
                    }
                }
            }
        }
    }

    detect_node_cycles(document, diagnostics);
}

fn validate_node_tokens(
    document: &AuthoringDocument,
    node: &DesignNode,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    for (property, token_id) in &node.token_bindings {
        validate_token_reference(document, token_id, &node.id, property, diagnostics);
    }
}

fn validate_node_values(
    document: &AuthoringDocument,
    node: &DesignNode,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    validate_sizing(
        document,
        &node.layout.horizontal,
        &node.id,
        "layout.horizontal",
        diagnostics,
    );
    validate_sizing(
        document,
        &node.layout.vertical,
        &node.id,
        "layout.vertical",
        diagnostics,
    );
    if let Some(gap) = &node.layout.gap {
        validate_bound_value(document, gap, &node.id, "layout.gap", diagnostics);
    }
    validate_edges(
        document,
        &node.layout.padding,
        &node.id,
        "layout.padding",
        diagnostics,
    );
    match &node.layout.flow {
        LayoutFlow::Grid { spec } => {
            for (index, track) in spec.columns.iter().enumerate() {
                if let GridTrack::Fixed(value) = track {
                    validate_bound_value(
                        document,
                        value,
                        &node.id,
                        &format!("layout.flow.columns[{index}]"),
                        diagnostics,
                    );
                }
            }
            for (index, track) in spec.rows.iter().enumerate() {
                if let GridTrack::Fixed(value) = track {
                    validate_bound_value(
                        document,
                        value,
                        &node.id,
                        &format!("layout.flow.rows[{index}]"),
                        diagnostics,
                    );
                }
            }
            if let Some(value) = &spec.column_gap {
                validate_bound_value(
                    document,
                    value,
                    &node.id,
                    "layout.flow.column_gap",
                    diagnostics,
                );
            }
            if let Some(value) = &spec.row_gap {
                validate_bound_value(
                    document,
                    value,
                    &node.id,
                    "layout.flow.row_gap",
                    diagnostics,
                );
            }
        }
        LayoutFlow::None | LayoutFlow::Horizontal { .. } | LayoutFlow::Vertical { .. } => {}
    }
    match &node.layout.position {
        PositionSpec::Absolute { x, y, .. } | PositionSpec::WindowFixed { x, y, .. } => {
            validate_bound_value(document, x, &node.id, "layout.position.x", diagnostics);
            validate_bound_value(document, y, &node.id, "layout.position.y", diagnostics);
        }
        PositionSpec::Flow => {}
    }

    for (index, fill) in node.visual.fills.iter().enumerate() {
        validate_fill(
            document,
            fill,
            &node.id,
            &format!("visual.fills[{index}]"),
            diagnostics,
        );
    }
    if let Some(stroke) = &node.visual.stroke {
        validate_paint(
            document,
            &stroke.paint,
            &node.id,
            "visual.stroke.paint",
            diagnostics,
        );
        validate_edges(
            document,
            &stroke.widths,
            &node.id,
            "visual.stroke.widths",
            diagnostics,
        );
    }
    validate_bound_value(
        document,
        &node.visual.radii.top_left,
        &node.id,
        "visual.radii.top_left",
        diagnostics,
    );
    validate_bound_value(
        document,
        &node.visual.radii.top_right,
        &node.id,
        "visual.radii.top_right",
        diagnostics,
    );
    validate_bound_value(
        document,
        &node.visual.radii.bottom_right,
        &node.id,
        "visual.radii.bottom_right",
        diagnostics,
    );
    validate_bound_value(
        document,
        &node.visual.radii.bottom_left,
        &node.id,
        "visual.radii.bottom_left",
        diagnostics,
    );
    for (index, effect) in node.visual.effects.iter().enumerate() {
        match effect {
            Effect::Shadow {
                color,
                x,
                y,
                blur,
                spread,
                ..
            } => {
                validate_bound_value(
                    document,
                    color,
                    &node.id,
                    &format!("visual.effects[{index}].color"),
                    diagnostics,
                );
                for (name, value) in [("x", x), ("y", y), ("blur", blur), ("spread", spread)] {
                    validate_bound_value(
                        document,
                        value,
                        &node.id,
                        &format!("visual.effects[{index}].{name}"),
                        diagnostics,
                    );
                }
            }
            Effect::Blur { radius } => validate_bound_value(
                document,
                radius,
                &node.id,
                &format!("visual.effects[{index}].radius"),
                diagnostics,
            ),
        }
    }

    if let Some(text) = &node.text {
        if let Some(value) = &text.font_family {
            validate_bound_value(document, value, &node.id, "text.font_family", diagnostics);
        }
        if let Some(value) = &text.font_size {
            validate_bound_value(document, value, &node.id, "text.font_size", diagnostics);
        }
        if let Some(value) = &text.font_weight {
            validate_bound_value(document, value, &node.id, "text.font_weight", diagnostics);
        }
        if let Some(value) = &text.line_height {
            validate_bound_value(document, value, &node.id, "text.line_height", diagnostics);
        }
        if let Some(value) = &text.letter_spacing {
            validate_bound_value(
                document,
                value,
                &node.id,
                "text.letter_spacing",
                diagnostics,
            );
        }
        if let Some(value) = &text.color {
            validate_bound_value(document, value, &node.id, "text.color", diagnostics);
        }
    }
}

fn validate_sizing(
    document: &AuthoringDocument,
    sizing: &SizingRule,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    match sizing {
        SizingRule::Fixed { value } => {
            validate_bound_value(document, value, entity_id, property, diagnostics);
        }
        SizingRule::Range {
            min,
            preferred,
            max,
        } => {
            for (suffix, value) in [("min", min), ("preferred", preferred), ("max", max)] {
                if let Some(value) = value {
                    validate_bound_value(
                        document,
                        value,
                        entity_id,
                        &format!("{property}.{suffix}"),
                        diagnostics,
                    );
                }
            }
        }
        SizingRule::Hug | SizingRule::Fill => {}
    }
}

fn validate_edges<T>(
    document: &AuthoringDocument,
    edges: &crate::authoring::EdgeValues<DesignValue<T>>,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    for (suffix, value) in [
        ("top", &edges.top),
        ("right", &edges.right),
        ("bottom", &edges.bottom),
        ("left", &edges.left),
    ] {
        validate_bound_value(
            document,
            value,
            entity_id,
            &format!("{property}.{suffix}"),
            diagnostics,
        );
    }
}

fn validate_fill(
    document: &AuthoringDocument,
    fill: &Fill,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    validate_unit_interval(
        fill.opacity,
        entity_id,
        &format!("{property}.opacity"),
        diagnostics,
    );
    validate_paint(document, &fill.paint, entity_id, property, diagnostics);
}

fn validate_paint(
    document: &AuthoringDocument,
    paint: &Paint,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    match paint {
        Paint::Solid { color } => validate_bound_value(
            document,
            color,
            entity_id,
            &format!("{property}.color"),
            diagnostics,
        ),
        Paint::LinearGradient { stops, .. } => {
            for (index, stop) in stops.iter().enumerate() {
                validate_bound_value(
                    document,
                    &stop.color,
                    entity_id,
                    &format!("{property}.stops[{index}].color"),
                    diagnostics,
                );
                validate_unit_interval(
                    stop.position,
                    entity_id,
                    &format!("{property}.stops[{index}].position"),
                    diagnostics,
                );
            }
        }
        Paint::Image { asset_id, .. } => {
            if asset_id.trim().is_empty() {
                diagnostics.push(DesignDiagnostic::error(
                    "GD-ASSET-001",
                    Some(entity_id),
                    Some(property),
                    "image paint asset_id must not be empty",
                ));
            }
        }
    }
}

fn validate_visual_numbers(node: &DesignNode, diagnostics: &mut Vec<DesignDiagnostic>) {
    validate_unit_interval(node.visual.opacity, &node.id, "visual.opacity", diagnostics);
}

fn validate_unit_interval(
    value: f64,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        diagnostics.push(DesignDiagnostic::error(
            "GD-VALUE-001",
            Some(entity_id),
            Some(property),
            format!("value {value} must be finite and within 0..=1"),
        ));
    }
}

fn validate_bound_value<T>(
    document: &AuthoringDocument,
    value: &DesignValue<T>,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    if let Some(token_id) = &value.token_id {
        validate_token_reference(document, token_id, entity_id, property, diagnostics);
    }
}

fn validate_token_reference(
    document: &AuthoringDocument,
    token_id: &str,
    entity_id: &str,
    property: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    if !document.tokens.contains_key(token_id) {
        diagnostics.push(DesignDiagnostic::error(
            "GD-TOKEN-001",
            Some(entity_id),
            Some(property),
            format!("token {token_id:?} does not exist"),
        ));
    }
}

fn detect_node_cycles(document: &AuthoringDocument, diagnostics: &mut Vec<DesignDiagnostic>) {
    let mut visited = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for node_id in document.nodes.keys() {
        visit_node(document, node_id, &mut visiting, &mut visited, diagnostics);
    }
}

fn visit_node(
    document: &AuthoringDocument,
    node_id: &str,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    if visited.contains(node_id) {
        return;
    }
    if !visiting.insert(node_id.to_owned()) {
        diagnostics.push(DesignDiagnostic::error(
            "GD-TREE-010",
            Some(node_id),
            Some("children"),
            "node hierarchy contains a cycle",
        ));
        return;
    }
    if let Some(node) = document.nodes.get(node_id) {
        for child_id in &node.children {
            visit_node(document, child_id, visiting, visited, diagnostics);
        }
    }
    visiting.remove(node_id);
    visited.insert(node_id.to_owned());
}

fn validate_tokens(document: &AuthoringDocument, diagnostics: &mut Vec<DesignDiagnostic>) {
    for (key, token) in &document.tokens {
        validate_identifier(key, "GD-ID-005", "tokens key", key, diagnostics);
        validate_identifier(&token.id, "GD-ID-006", "token.id", key, diagnostics);
        if key != &token.id {
            diagnostics.push(DesignDiagnostic::error(
                "GD-ID-007",
                Some(key),
                Some("id"),
                format!(
                    "token map key {key:?} does not match embedded id {:?}",
                    token.id
                ),
            ));
        }
        if !token.modes.contains_key(&token.default_mode) {
            diagnostics.push(DesignDiagnostic::error(
                "GD-TOKEN-002",
                Some(key),
                Some("default_mode"),
                format!(
                    "default mode {:?} is not present in modes",
                    token.default_mode
                ),
            ));
        }
        if token.modes.is_empty() {
            diagnostics.push(DesignDiagnostic::error(
                "GD-TOKEN-003",
                Some(key),
                Some("modes"),
                "token must define at least one mode",
            ));
        }
        for (mode, value) in &token.modes {
            if mode.trim().is_empty() {
                diagnostics.push(DesignDiagnostic::error(
                    "GD-TOKEN-004",
                    Some(key),
                    Some("modes"),
                    "token mode name must not be empty",
                ));
            }
            match value {
                TokenValue::Alias(target) => {
                    if !document.tokens.contains_key(target) {
                        diagnostics.push(DesignDiagnostic::error(
                            "GD-TOKEN-005",
                            Some(key),
                            Some(&format!("modes.{mode}")),
                            format!("alias target {target:?} does not exist"),
                        ));
                    }
                }
                concrete if concrete.kind() != Some(token.kind) => {
                    diagnostics.push(DesignDiagnostic::error(
                        "GD-TOKEN-006",
                        Some(key),
                        Some(&format!("modes.{mode}")),
                        format!(
                            "token kind {:?} does not match mode value kind {:?}",
                            token.kind,
                            concrete.kind()
                        ),
                    ));
                }
                TokenValue::Color(color) => {
                    for (channel, value) in [
                        ("r", color.r),
                        ("g", color.g),
                        ("b", color.b),
                        ("a", color.a),
                    ] {
                        validate_unit_interval(
                            value,
                            key,
                            &format!("modes.{mode}.{channel}"),
                            diagnostics,
                        );
                    }
                }
                TokenValue::Number(value) if !value.is_finite() => {
                    diagnostics.push(DesignDiagnostic::error(
                        "GD-TOKEN-007",
                        Some(key),
                        Some(&format!("modes.{mode}")),
                        "numeric token value must be finite",
                    ));
                }
                TokenValue::Motion(value) if value.easing.iter().any(|item| !item.is_finite()) => {
                    diagnostics.push(DesignDiagnostic::error(
                        "GD-TOKEN-008",
                        Some(key),
                        Some(&format!("modes.{mode}.easing")),
                        "motion easing values must be finite",
                    ));
                }
                TokenValue::Number(_)
                | TokenValue::Text(_)
                | TokenValue::Boolean(_)
                | TokenValue::Font(_)
                | TokenValue::Motion(_) => {}
            }
        }
    }
    detect_token_alias_cycles(document, diagnostics);
}

fn detect_token_alias_cycles(
    document: &AuthoringDocument,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    for token in document.tokens.values() {
        for mode in token.modes.keys() {
            let mut path = BTreeSet::new();
            visit_token_alias(document, token, mode, &mut path, diagnostics);
        }
    }
}

fn visit_token_alias(
    document: &AuthoringDocument,
    token: &DesignToken,
    mode: &str,
    path: &mut BTreeSet<String>,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    let key = format!("{}\u{0}{mode}", token.id);
    if !path.insert(key.clone()) {
        diagnostics.push(DesignDiagnostic::error(
            "GD-TOKEN-009",
            Some(&token.id),
            Some(&format!("modes.{mode}")),
            "token aliases contain a cycle",
        ));
        return;
    }
    if let Some(TokenValue::Alias(target_id)) = token.modes.get(mode)
        && let Some(target) = document.tokens.get(target_id)
    {
        let target_mode = if target.modes.contains_key(mode) {
            mode
        } else {
            &target.default_mode
        };
        visit_token_alias(document, target, target_mode, path, diagnostics);
    }
    path.remove(&key);
}

fn validate_components(document: &AuthoringDocument, diagnostics: &mut Vec<DesignDiagnostic>) {
    for (key, component) in &document.components {
        validate_identifier(key, "GD-ID-008", "components key", key, diagnostics);
        validate_identifier(&component.id, "GD-ID-009", "component.id", key, diagnostics);
        if key != &component.id {
            diagnostics.push(DesignDiagnostic::error(
                "GD-ID-010",
                Some(key),
                Some("id"),
                format!(
                    "component map key {key:?} does not match embedded id {:?}",
                    component.id
                ),
            ));
        }
        for (axis, variant) in &component.variants {
            if !variant.values.contains(&variant.default) {
                diagnostics.push(DesignDiagnostic::error(
                    "GD-COMPONENT-003",
                    Some(key),
                    Some(&format!("variants.{axis}.default")),
                    format!(
                        "default {:?} is not a declared variant value",
                        variant.default
                    ),
                ));
            }
        }
        for (property, token_id) in &component.token_bindings {
            validate_token_reference(document, token_id, key, property, diagnostics);
        }
        if let Some(binding_id) = &component.code_binding_id
            && !document.code_bindings.contains_key(binding_id)
        {
            diagnostics.push(DesignDiagnostic::error(
                "GD-COMPONENT-004",
                Some(key),
                Some("code_binding_id"),
                format!("code binding {binding_id:?} does not exist"),
            ));
        }
    }
}

fn validate_code_bindings(document: &AuthoringDocument, diagnostics: &mut Vec<DesignDiagnostic>) {
    for (key, binding) in &document.code_bindings {
        validate_identifier(key, "GD-ID-011", "code_bindings key", key, diagnostics);
        validate_identifier(
            &binding.id,
            "GD-ID-012",
            "code_binding.id",
            key,
            diagnostics,
        );
        if key != &binding.id {
            diagnostics.push(DesignDiagnostic::error(
                "GD-ID-013",
                Some(key),
                Some("id"),
                format!(
                    "code binding map key {key:?} does not match embedded id {:?}",
                    binding.id
                ),
            ));
        }
        let target_exists = match binding.target.kind {
            BindingTargetKind::Document => binding.target.id == document.document_id,
            BindingTargetKind::Node => document.nodes.contains_key(&binding.target.id),
            BindingTargetKind::Token => document.tokens.contains_key(&binding.target.id),
            BindingTargetKind::Component => document.components.contains_key(&binding.target.id),
        };
        if !target_exists {
            diagnostics.push(DesignDiagnostic::error(
                "GD-BINDING-001",
                Some(key),
                Some("target"),
                format!(
                    "binding target {:?} {:?} does not exist",
                    binding.target.kind, binding.target.id
                ),
            ));
        }
        if binding.symbol.crate_name.trim().is_empty()
            || binding.symbol.symbol_name.trim().is_empty()
        {
            diagnostics.push(DesignDiagnostic::error(
                "GD-BINDING-002",
                Some(key),
                Some("symbol"),
                "Rust binding requires non-empty crate_name and symbol_name",
            ));
        }
        if binding.ownership == CodeOwnership::HandwrittenBehavior
            && binding.sync_policy != SyncPolicy::ReferenceOnly
        {
            diagnostics.push(DesignDiagnostic::error(
                "GD-BINDING-003",
                Some(key),
                Some("sync_policy"),
                "handwritten behavior is reference-only; design transactions must never rewrite domain behavior",
            ));
        }
    }
}

fn validate_identifier(
    value: &str,
    code: &str,
    property: &str,
    entity_id: &str,
    diagnostics: &mut Vec<DesignDiagnostic>,
) {
    let valid = !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        });
    if !valid {
        diagnostics.push(DesignDiagnostic::error(
            code,
            Some(entity_id),
            Some(property),
            "identifier must be 1..=256 ASCII letters, digits, '-', '_', '.', ':', or '/'",
        ));
    }
}

pub fn document_fingerprint(
    document: &AuthoringDocument,
) -> Result<DocumentFingerprint, serde_json::Error> {
    let mut canonical = document.clone();
    canonical.revision = 0;
    let bytes = serde_json::to_vec(&canonical)?;
    let digest = Sha256::digest(bytes);
    Ok(DocumentFingerprint {
        algorithm: "sha256".to_owned(),
        schema_version: document.schema_version,
        value: hex(&digest),
    })
}

#[must_use]
pub fn summarize_document(document: &AuthoringDocument) -> DocumentSummary {
    let mut token_scopes = BTreeMap::new();
    for token in document.tokens.values() {
        *token_scopes
            .entry(token_scope_name(token.scope))
            .or_insert(0) += 1;
    }
    let mut component_roles = BTreeMap::new();
    for component in document.components.values() {
        *component_roles
            .entry(component_role_name(component.role))
            .or_insert(0) += 1;
    }
    DocumentSummary {
        document_id: document.document_id.clone(),
        revision: document.revision,
        roots: document.roots.len(),
        nodes: document.nodes.len(),
        tokens: document.tokens.len(),
        components: document.components.len(),
        code_bindings: document.code_bindings.len(),
        token_scopes,
        component_roles,
    }
}

pub fn lowering_manifest(
    document: &AuthoringDocument,
) -> Result<LoweringManifest, serde_json::Error> {
    let validation = validate_document(document);
    let mut bound_components = Vec::new();
    let mut unbound_components = Vec::new();
    for component in document.components.values() {
        if component.code_binding_id.is_some() {
            bound_components.push(component.id.clone());
        } else {
            unbound_components.push(component.id.clone());
        }
    }
    let mut generated_presentation_bindings = Vec::new();
    let mut handwritten_presentation_bindings = Vec::new();
    let mut reference_only_behavior_bindings = Vec::new();
    for binding in document.code_bindings.values() {
        match binding.ownership {
            CodeOwnership::GeneratedPresentation => {
                generated_presentation_bindings.push(binding.id.clone());
            }
            CodeOwnership::HandwrittenPresentation => {
                handwritten_presentation_bindings.push(binding.id.clone());
            }
            CodeOwnership::HandwrittenBehavior => {
                reference_only_behavior_bindings.push(binding.id.clone());
            }
        }
    }
    Ok(LoweringManifest {
        manifest_version: LOWERING_MANIFEST_VERSION,
        document_id: document.document_id.clone(),
        revision: document.revision,
        fingerprint: document_fingerprint(document)?,
        summary: summarize_document(document),
        valid: validation.valid,
        error_count: validation.error_count,
        warning_count: validation.warning_count,
        bound_components,
        unbound_components,
        generated_presentation_bindings,
        handwritten_presentation_bindings,
        reference_only_behavior_bindings,
    })
}

fn token_scope_name(scope: TokenScope) -> String {
    match scope {
        TokenScope::Primitive => "PRIMITIVE",
        TokenScope::Semantic => "SEMANTIC",
        TokenScope::Component => "COMPONENT",
        TokenScope::Module => "MODULE",
        TokenScope::Shell => "SHELL",
        TokenScope::Platform => "PLATFORM",
    }
    .to_owned()
}

fn component_role_name(role: ComponentRole) -> String {
    match role {
        ComponentRole::Primitive => "PRIMITIVE",
        ComponentRole::Control => "CONTROL",
        ComponentRole::Composite => "COMPOSITE",
        ComponentRole::Module => "MODULE",
        ComponentRole::Shell => "SHELL",
        ComponentRole::Overlay => "OVERLAY",
    }
    .to_owned()
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
