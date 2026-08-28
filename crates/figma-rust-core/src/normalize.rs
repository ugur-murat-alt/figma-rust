use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::diagnostic::{Diagnostic, Severity, codes};
use crate::ir::{
    AssetDecision, AssetRoute, Axis, AxisSize, AxisSizing, BoundValue, Color, ComponentMetadata,
    ComponentResolution, DESIGN_IR_VERSION, DesignDocument, Edges, Effect, GradientStop,
    GridPlacement, GridTrack, Layout, Node, Paint, Positioning, Radii, Scroll, Size, Style, Text,
    TextRun, TextStyle, TokenRef, Transform, Variable,
};
use crate::raw::{
    ExtractionBundle, RawAxisSizing, RawBoundValue, RawChildAlignment, RawColor,
    RawComponentMetadata, RawComponentRole, RawEffect, RawGridTrack, RawLayoutMode, RawNode,
    RawNodeKind, RawPaint, RawPositioning, RawText, RawTextStyle, RawVariable,
};

pub const EXTRACTION_SCHEMA_VERSION: u32 = 2;

fn unbound_number(fallback: f64) -> BoundValue<f64> {
    BoundValue {
        token: None,
        mode_context: BTreeMap::new(),
        fallback,
    }
}

#[derive(Debug, Error)]
#[error("extraction bundle is not valid JSON: {0}")]
pub struct ParseError(#[from] serde_json::Error);

/// Registry entries identify semantic mappings without importing target types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentMapping {
    pub id: String,
    pub component_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_set_key: Option<String>,
}

/// Target-neutral registry index. Target-specific conversion stays in codegen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentRegistry {
    pub schema_version: u32,
    #[serde(default)]
    pub mappings: BTreeMap<String, ComponentMapping>,
}

impl ComponentRegistry {
    fn mapping_for(&self, metadata: &RawComponentMetadata) -> Option<&ComponentMapping> {
        self.mappings
            .get(&metadata.component_key)
            .filter(|mapping| {
                mapping.component_key == metadata.component_key
                    && mapping
                        .component_set_key
                        .as_ref()
                        .is_none_or(|set_key| metadata.component_set_key.as_ref() == Some(set_key))
            })
    }
}

/// IR plus ordered diagnostics. Errors do not erase the partially normalized tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizationOutput {
    pub document: DesignDocument,
    pub diagnostics: Vec<Diagnostic>,
}

impl NormalizationOutput {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

/// Parses a versioned extraction bundle.
///
/// # Errors
///
/// Returns [`ParseError`] when `input` is not valid JSON for the raw schema.
pub fn parse_bundle(input: &str) -> Result<ExtractionBundle, ParseError> {
    #[derive(Deserialize)]
    struct SchemaHeader {
        schema_version: u32,
    }

    let schema_version = serde_json::from_str::<SchemaHeader>(input)?.schema_version;
    if schema_version == 1 {
        let mut value = serde_json::from_str::<serde_json::Value>(input)?;
        adapt_v1_bound_numbers(&mut value);
        serde_json::from_value(value).map_err(ParseError::from)
    } else {
        serde_json::from_str(input).map_err(ParseError::from)
    }
}

fn adapt_v1_bound_numbers(bundle: &mut serde_json::Value) {
    let Some(roots) = bundle
        .get_mut("roots")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for root in roots {
        adapt_v1_node(root);
    }
}

fn adapt_v1_node(node: &mut serde_json::Value) {
    let Some(node) = node.as_object_mut() else {
        return;
    };
    if let Some(layout) = node
        .get_mut("layout")
        .and_then(serde_json::Value::as_object_mut)
    {
        wrap_v1_bound_number(layout.get_mut("gap"));
        wrap_v1_bound_edges(layout.get_mut("padding"));
    }
    if let Some(style) = node
        .get_mut("style")
        .and_then(serde_json::Value::as_object_mut)
    {
        wrap_v1_bound_edges(style.get_mut("stroke_widths"));
        if let Some(radii) = style
            .get_mut("radii")
            .and_then(serde_json::Value::as_object_mut)
        {
            for key in ["top_left", "top_right", "bottom_right", "bottom_left"] {
                wrap_v1_bound_number(radii.get_mut(key));
            }
        }
    }
    if let Some(children) = node
        .get_mut("children")
        .and_then(serde_json::Value::as_array_mut)
    {
        for child in children {
            adapt_v1_node(child);
        }
    }
}

fn wrap_v1_bound_edges(edges: Option<&mut serde_json::Value>) {
    let Some(edges) = edges.and_then(serde_json::Value::as_object_mut) else {
        return;
    };
    for key in ["top", "right", "bottom", "left"] {
        wrap_v1_bound_number(edges.get_mut(key));
    }
}

fn wrap_v1_bound_number(value: Option<&mut serde_json::Value>) {
    let Some(value) = value else {
        return;
    };
    let literal = value.take();
    *value = serde_json::json!({ "literal": literal });
}

/// Parses and normalizes an extraction bundle with an empty component registry.
///
/// # Errors
///
/// Returns [`ParseError`] when `input` is not valid JSON for the raw schema.
pub fn parse_and_normalize(input: &str) -> Result<NormalizationOutput, ParseError> {
    let bundle = parse_bundle(input)?;
    Ok(normalize_bundle(&bundle))
}

#[must_use]
pub fn normalize_bundle(bundle: &ExtractionBundle) -> NormalizationOutput {
    normalize_bundle_with_registry(bundle, &ComponentRegistry::default())
}

#[must_use]
pub fn normalize_bundle_with_registry(
    bundle: &ExtractionBundle,
    registry: &ComponentRegistry,
) -> NormalizationOutput {
    let mut diagnostics = bundle.extraction_diagnostics.clone();
    if bundle.schema_version != EXTRACTION_SCHEMA_VERSION {
        diagnostics.push(Diagnostic::bundle(
            Severity::Error,
            codes::SCHEMA_VERSION,
            format!(
                "unsupported extraction schema version {}; expected {}",
                bundle.schema_version, EXTRACTION_SCHEMA_VERSION
            ),
        ));
    }

    let mut node_ids = BTreeSet::new();
    for root in &bundle.roots {
        validate_node_ids(root, &mut node_ids, &mut diagnostics);
    }

    let raw_variables = sorted_variables(bundle, &mut diagnostics);
    let components = sorted_components(bundle, &mut diagnostics);
    let normalized_assets = sorted_assets(bundle, &mut diagnostics);
    let mut variables = BTreeMap::<(String, BTreeMap<String, String>), TokenRef>::new();
    for variable in &raw_variables {
        variables.insert(
            (variable.id.clone(), variable.mode_context.clone()),
            variable_token(variable),
        );
    }
    let assets = bundle
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect::<BTreeSet<_>>();

    let mut context = Context {
        variables: &variables,
        assets: &assets,
        registry,
        diagnostics,
    };
    let roots = bundle
        .roots
        .iter()
        .map(|root| context.normalize_node(root, None))
        .collect();

    let normalized_variables = raw_variables
        .iter()
        .copied()
        .map(|variable| Variable {
            token: variable_token(variable),
            mode_context: variable.mode_context.clone(),
            value: context.normalize_variable_value(variable),
        })
        .collect();
    NormalizationOutput {
        document: DesignDocument {
            ir_version: DESIGN_IR_VERSION,
            source: bundle.source.clone(),
            roots,
            variables: normalized_variables,
            components,
            assets: normalized_assets,
        },
        diagnostics: context.diagnostics,
    }
}

#[derive(Debug, Clone, Copy)]
enum ParentLayout {
    Stack,
    Grid,
    Absolute {
        fixed_width: Option<f64>,
        fixed_height: Option<f64>,
    },
}

fn canonicalize_fixed_parent_constraint(
    constraint: crate::raw::RawConstraint,
    parent_size: Option<f64>,
) -> crate::raw::RawConstraint {
    if constraint != crate::raw::RawConstraint::Min && parent_size.is_some_and(f64::is_finite) {
        crate::raw::RawConstraint::Min
    } else {
        constraint
    }
}

fn has_parent_size_aware_constraints(positioning: &Positioning) -> bool {
    matches!(
        positioning,
        Positioning::Absolute {
            horizontal_constraint,
            vertical_constraint,
            ..
        } if *horizontal_constraint != crate::raw::RawConstraint::Min
            || *vertical_constraint != crate::raw::RawConstraint::Min
    )
}

fn pass_through_requires_runtime(raw: &RawNode) -> bool {
    let is_container = matches!(
        raw.kind,
        RawNodeKind::Frame
            | RawNodeKind::Group
            | RawNodeKind::Component
            | RawNodeKind::Instance
            | RawNodeKind::Scroll
    );
    !raw.children.is_empty() && (!is_container || raw.opacity.to_bits() != 1.0_f64.to_bits())
}

struct Context<'a> {
    variables: &'a BTreeMap<(String, BTreeMap<String, String>), TokenRef>,
    assets: &'a BTreeSet<&'a str>,
    registry: &'a ComponentRegistry,
    diagnostics: Vec<Diagnostic>,
}

impl Context<'_> {
    fn normalize_node(&mut self, raw: &RawNode, parent: Option<ParentLayout>) -> Node {
        let positioning = self.normalize_positioning(raw, parent);
        let is_absolute = matches!(positioning, Positioning::Absolute { .. });
        let child_counter_alignment = raw.layout.child_counter_alignment;
        let valid_child_alignment = match (child_counter_alignment, parent) {
            (RawChildAlignment::Inherit, _) => true,
            (_, Some(ParentLayout::Stack)) if !is_absolute => true,
            _ => false,
        };
        if !valid_child_alignment {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_CHILD_ALIGNMENT,
                "child counter-axis alignment requires an auto-positioned stack child",
                &raw.id,
                Some("layout.child_counter_alignment"),
            ));
        }
        let size = Size {
            horizontal: self.normalize_axis_size(raw, true, parent, is_absolute),
            vertical: self.normalize_axis_size(raw, false, parent, is_absolute),
            aspect_ratio: raw
                .size
                .aspect_ratio
                .map(|value| self.finite_non_negative(raw, value, "size.aspect_ratio")),
        };
        let layout = self.normalize_layout(raw);
        let child_parent = match raw.layout.mode {
            RawLayoutMode::Horizontal | RawLayoutMode::Vertical => ParentLayout::Stack,
            RawLayoutMode::Grid => ParentLayout::Grid,
            RawLayoutMode::None => ParentLayout::Absolute {
                fixed_width: (raw.size.horizontal == Some(RawAxisSizing::Fixed))
                    .then(|| raw.size.width.as_ref().map(|value| value.literal))
                    .flatten(),
                fixed_height: (raw.size.vertical == Some(RawAxisSizing::Fixed))
                    .then(|| raw.size.height.as_ref().map(|value| value.literal))
                    .flatten(),
            },
        };
        let style = self.normalize_style(raw);
        let text = raw.text.as_ref().map(|text| self.normalize_text(raw, text));
        let component = if let Some(component) = raw.component.as_ref() {
            Some(self.normalize_component(raw, component))
        } else {
            if raw.kind == RawNodeKind::Instance {
                self.diagnostics.push(Diagnostic::node(
                    Severity::Warning,
                    codes::UNMAPPED_COMPONENT,
                    "instance has no component metadata; structural children are retained",
                    &raw.id,
                    Some("component"),
                ));
            }
            None
        };
        let asset_decision = self.decide_asset(raw, &positioning);
        self.emit_asset_diagnostic(raw, &asset_decision);
        let children = raw
            .children
            .iter()
            .map(|child| self.normalize_node(child, Some(child_parent)))
            .collect();

        Node {
            source_id: raw.id.clone(),
            name: raw.name.clone(),
            kind: raw.kind,
            visible: raw.visible,
            opacity: self.finite_in_range(raw, raw.opacity, 0.0, 1.0, "opacity"),
            size,
            layout,
            positioning,
            child_counter_alignment,
            style,
            text,
            component,
            reactions: raw.reactions.clone(),
            asset_decision,
            children,
        }
    }

    fn normalize_axis_size(
        &mut self,
        raw: &RawNode,
        horizontal: bool,
        parent: Option<ParentLayout>,
        is_absolute: bool,
    ) -> AxisSize {
        let (source_sizing, fixed, min, max, path, value_path, min_path, max_path) = if horizontal {
            (
                raw.size.horizontal,
                raw.size.width.as_ref(),
                raw.size.min_width.as_ref(),
                raw.size.max_width.as_ref(),
                "size.horizontal",
                "size.width",
                "size.min_width",
                "size.max_width",
            )
        } else {
            (
                raw.size.vertical,
                raw.size.height.as_ref(),
                raw.size.min_height.as_ref(),
                raw.size.max_height.as_ref(),
                "size.vertical",
                "size.height",
                "size.min_height",
                "size.max_height",
            )
        };

        let sizing = match source_sizing {
            Some(RawAxisSizing::Hug) => AxisSizing::Hug,
            Some(RawAxisSizing::Fill) => {
                let compatible_parent =
                    matches!(parent, Some(ParentLayout::Stack | ParentLayout::Grid));
                if !compatible_parent || is_absolute {
                    self.diagnostics.push(Diagnostic::node(
                        Severity::Error,
                        codes::AMBIGUOUS_FILL,
                        "FILL requires an auto-positioned child of a stack or grid parent",
                        &raw.id,
                        Some(path),
                    ));
                }
                AxisSizing::Fill
            }
            Some(RawAxisSizing::Fixed) => AxisSizing::Fixed(if let Some(value) = fixed {
                self.normalize_non_negative_number_bound(raw, value, value_path)
            } else {
                self.diagnostics.push(Diagnostic::node(
                    Severity::Error,
                    codes::MISSING_SIZING,
                    "FIXED sizing requires an explicit finite dimension",
                    &raw.id,
                    Some(path),
                ));
                unbound_number(0.0)
            }),
            None => {
                self.diagnostics.push(Diagnostic::node(
                    Severity::Error,
                    codes::MISSING_SIZING,
                    "axis sizing is missing; preserving an explicit dimension as FIXED",
                    &raw.id,
                    Some(path),
                ));
                AxisSizing::Fixed(fixed.map_or_else(
                    || unbound_number(0.0),
                    |value| self.normalize_non_negative_number_bound(raw, value, value_path),
                ))
            }
        };

        let normalized_min =
            min.map(|value| self.normalize_non_negative_number_bound(raw, value, min_path));
        let normalized_max =
            max.map(|value| self.normalize_non_negative_number_bound(raw, value, max_path));
        if normalized_min
            .as_ref()
            .zip(normalized_max.as_ref())
            .is_some_and(|(minimum, maximum)| minimum.fallback > maximum.fallback)
        {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_CONSTRAINT,
                "minimum axis size exceeds maximum axis size",
                &raw.id,
                Some(path),
            ));
        }

        AxisSize {
            sizing,
            measured: (source_sizing == Some(RawAxisSizing::Hug))
                .then(|| {
                    fixed.map(|value| {
                        self.normalize_non_negative_number_bound(raw, value, value_path)
                    })
                })
                .flatten(),
            min: normalized_min,
            max: normalized_max,
        }
    }

    fn normalize_positioning(
        &mut self,
        raw: &RawNode,
        parent: Option<ParentLayout>,
    ) -> Positioning {
        let transform = Transform {
            matrix: raw
                .position
                .transform
                .matrix
                .map(|value| self.finite_or_zero(raw, value, "position.transform.matrix")),
        };
        let absolute = raw.position.positioning == RawPositioning::Absolute
            || matches!(parent, Some(ParentLayout::Absolute { .. }));
        if absolute {
            let (mut horizontal_constraint, mut vertical_constraint) = (
                raw.position.horizontal_constraint,
                raw.position.vertical_constraint,
            );
            if let Some(ParentLayout::Absolute {
                fixed_width,
                fixed_height,
            }) = parent
            {
                horizontal_constraint =
                    canonicalize_fixed_parent_constraint(horizontal_constraint, fixed_width);
                vertical_constraint =
                    canonicalize_fixed_parent_constraint(vertical_constraint, fixed_height);
            }
            Positioning::Absolute {
                x: self.finite_or_zero(raw, raw.position.x, "position.x"),
                y: self.finite_or_zero(raw, raw.position.y, "position.y"),
                horizontal_constraint,
                vertical_constraint,
                transform,
            }
        } else {
            let grid = raw.position.grid.map(|placement| {
                let row_span = self.nonzero_span(raw, placement.row_span, "position.grid.row_span");
                let column_span =
                    self.nonzero_span(raw, placement.column_span, "position.grid.column_span");
                GridPlacement {
                    row: placement.row,
                    column: placement.column,
                    row_span,
                    column_span,
                }
            });
            Positioning::Auto { grid, transform }
        }
    }

    fn nonzero_span(&mut self, raw: &RawNode, value: u32, path: &str) -> u32 {
        if value == 0 {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_CONSTRAINT,
                "grid span must be at least one",
                &raw.id,
                Some(path),
            ));
            1
        } else {
            value
        }
    }

    fn normalize_layout(&mut self, raw: &RawNode) -> Layout {
        let scroll = Scroll {
            horizontal: raw.layout.scroll.horizontal,
            vertical: raw.layout.scroll.vertical,
        };
        let padding = self.normalize_edges(raw, &raw.layout.padding, "layout.padding");
        match raw.layout.mode {
            RawLayoutMode::None if raw.children.is_empty() => Layout::Plain {
                clips_content: raw.layout.clips_content,
                scroll,
            },
            RawLayoutMode::None => Layout::Absolute {
                clips_content: raw.layout.clips_content,
                scroll,
            },
            RawLayoutMode::Horizontal | RawLayoutMode::Vertical => Layout::Stack {
                axis: if raw.layout.mode == RawLayoutMode::Horizontal {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                },
                wrap: raw.layout.wrap,
                primary_alignment: raw.layout.primary_alignment,
                counter_alignment: raw.layout.counter_alignment,
                gap: self.normalize_non_negative_number_bound(raw, &raw.layout.gap, "layout.gap"),
                padding,
                clips_content: raw.layout.clips_content,
                scroll,
            },
            RawLayoutMode::Grid => Layout::Grid {
                columns: raw
                    .layout
                    .grid
                    .columns
                    .iter()
                    .map(|track| self.normalize_grid_track(raw, *track, "layout.grid.columns"))
                    .collect(),
                rows: raw
                    .layout
                    .grid
                    .rows
                    .iter()
                    .map(|track| self.normalize_grid_track(raw, *track, "layout.grid.rows"))
                    .collect(),
                column_gap: self.finite_non_negative(
                    raw,
                    raw.layout.grid.column_gap,
                    "layout.grid.column_gap",
                ),
                row_gap: self.finite_non_negative(
                    raw,
                    raw.layout.grid.row_gap,
                    "layout.grid.row_gap",
                ),
                padding,
                clips_content: raw.layout.clips_content,
                scroll,
            },
        }
    }

    fn normalize_grid_track(
        &mut self,
        raw: &RawNode,
        track: RawGridTrack,
        path: &str,
    ) -> GridTrack {
        match track {
            RawGridTrack::Fixed(value) => {
                GridTrack::Fixed(self.finite_non_negative(raw, value, path))
            }
            RawGridTrack::Flex(value) => {
                GridTrack::Flex(self.finite_non_negative(raw, value, path))
            }
            RawGridTrack::Hug => GridTrack::Hug,
        }
    }

    fn normalize_style(&mut self, raw: &RawNode) -> Style {
        Style {
            fills: raw
                .style
                .fills
                .iter()
                .enumerate()
                .map(|(index, paint)| {
                    self.normalize_paint(raw, paint, &format!("style.fills[{index}]"))
                })
                .collect(),
            strokes: raw
                .style
                .strokes
                .iter()
                .enumerate()
                .map(|(index, paint)| {
                    self.normalize_paint(raw, paint, &format!("style.strokes[{index}]"))
                })
                .collect(),
            stroke_widths: self.normalize_edges(
                raw,
                &raw.style.stroke_widths,
                "style.stroke_widths",
            ),
            stroke_align: raw.style.stroke_align,
            radii: Radii {
                top_left: self.normalize_non_negative_number_bound(
                    raw,
                    &raw.style.radii.top_left,
                    "style.radii.top_left",
                ),
                top_right: self.normalize_non_negative_number_bound(
                    raw,
                    &raw.style.radii.top_right,
                    "style.radii.top_right",
                ),
                bottom_right: self.normalize_non_negative_number_bound(
                    raw,
                    &raw.style.radii.bottom_right,
                    "style.radii.bottom_right",
                ),
                bottom_left: self.normalize_non_negative_number_bound(
                    raw,
                    &raw.style.radii.bottom_left,
                    "style.radii.bottom_left",
                ),
                smoothing: self.finite_in_range(
                    raw,
                    raw.style.radii.smoothing,
                    0.0,
                    1.0,
                    "style.radii.smoothing",
                ),
            },
            effects: raw
                .style
                .effects
                .iter()
                .enumerate()
                .map(|(index, effect)| {
                    self.normalize_effect(raw, effect, &format!("style.effects[{index}]"))
                })
                .collect(),
            blend_mode: raw.style.blend_mode,
            is_mask: raw.style.is_mask,
        }
    }

    fn normalize_paint(&mut self, raw: &RawNode, paint: &RawPaint, path: &str) -> Paint {
        match paint {
            RawPaint::Solid { color } => Paint::Solid {
                color: self.normalize_color_bound(raw, color, path),
            },
            RawPaint::Gradient {
                gradient_kind,
                stops,
            } => Paint::Gradient {
                gradient_kind: *gradient_kind,
                stops: stops
                    .iter()
                    .map(|stop| GradientStop {
                        position: self.finite_in_range(raw, stop.position, 0.0, 1.0, path),
                        color: self.normalize_color_bound(raw, &stop.color, path),
                    })
                    .collect(),
            },
            RawPaint::Image {
                asset_id,
                scale_mode,
                image_transform,
                opacity,
                rotation,
                has_filters,
            } => {
                if !self.assets.contains(asset_id.as_str()) {
                    self.diagnostics.push(Diagnostic::node(
                        Severity::Error,
                        codes::MISSING_ASSET,
                        format!("image asset {asset_id} is not present in the extraction bundle"),
                        &raw.id,
                        Some(path),
                    ));
                }
                Paint::Image {
                    asset_id: asset_id.clone(),
                    scale_mode: *scale_mode,
                    image_transform: image_transform.map(|transform| Transform {
                        matrix: transform.matrix.map(|value| {
                            self.finite_or_zero(raw, value, &format!("{path}.image_transform"))
                        }),
                    }),
                    opacity: self.finite_in_range(raw, *opacity, 0.0, 1.0, path),
                    rotation: rotation
                        .map(|value| self.finite_or_zero(raw, value, &format!("{path}.rotation"))),
                    has_filters: *has_filters,
                }
            }
            RawPaint::Video => Paint::Unsupported {
                source_kind: "VIDEO".to_owned(),
            },
            RawPaint::Pattern => Paint::Unsupported {
                source_kind: "PATTERN".to_owned(),
            },
            RawPaint::Shader => Paint::Unsupported {
                source_kind: "SHADER".to_owned(),
            },
        }
    }

    fn normalize_effect(&mut self, raw: &RawNode, effect: &RawEffect, path: &str) -> Effect {
        match effect {
            RawEffect::DropShadow {
                color,
                offset_x,
                offset_y,
                blur,
                spread,
            }
            | RawEffect::InnerShadow {
                color,
                offset_x,
                offset_y,
                blur,
                spread,
            } => Effect::Shadow {
                inset: matches!(effect, RawEffect::InnerShadow { .. }),
                color: self.normalize_color_bound(raw, color, path),
                offset_x: self.finite_or_zero(raw, *offset_x, path),
                offset_y: self.finite_or_zero(raw, *offset_y, path),
                blur: self.finite_non_negative(raw, *blur, path),
                spread: self.finite_or_zero(raw, *spread, path),
            },
            unsupported => {
                let kind = effect_kind(unsupported);
                self.diagnostics.push(Diagnostic::node(
                    Severity::Warning,
                    codes::UNSUPPORTED_EFFECT,
                    format!("effect {kind} requires an explicit raster fallback"),
                    &raw.id,
                    Some(path),
                ));
                Effect::Unsupported {
                    source_kind: kind.to_owned(),
                }
            }
        }
    }

    fn normalize_color_bound(
        &mut self,
        raw: &RawNode,
        value: &RawBoundValue<RawColor>,
        path: &str,
    ) -> BoundValue<Color> {
        BoundValue {
            token: self.resolve_token(raw, value.token_id.as_deref(), &value.mode_context, path),
            mode_context: value.mode_context.clone(),
            fallback: Color {
                r: self.finite_in_range(raw, value.literal.r, 0.0, 1.0, path),
                g: self.finite_in_range(raw, value.literal.g, 0.0, 1.0, path),
                b: self.finite_in_range(raw, value.literal.b, 0.0, 1.0, path),
                a: self.finite_in_range(raw, value.literal.a, 0.0, 1.0, path),
            },
        }
    }

    fn normalize_number_bound(
        &mut self,
        raw: &RawNode,
        value: &RawBoundValue<f64>,
        path: &str,
    ) -> BoundValue<f64> {
        BoundValue {
            token: self.resolve_token(raw, value.token_id.as_deref(), &value.mode_context, path),
            mode_context: value.mode_context.clone(),
            fallback: self.finite_or_zero(raw, value.literal, path),
        }
    }

    fn normalize_non_negative_number_bound(
        &mut self,
        raw: &RawNode,
        value: &RawBoundValue<f64>,
        path: &str,
    ) -> BoundValue<f64> {
        BoundValue {
            token: self.resolve_token(raw, value.token_id.as_deref(), &value.mode_context, path),
            mode_context: value.mode_context.clone(),
            fallback: self.finite_non_negative(raw, value.literal, path),
        }
    }

    fn resolve_token(
        &mut self,
        raw: &RawNode,
        token_id: Option<&str>,
        mode_context: &BTreeMap<String, String>,
        path: &str,
    ) -> Option<TokenRef> {
        token_id.map(|id| {
            let key = (id.to_owned(), mode_context.clone());
            let Some(token) = self.variables.get(&key) else {
                self.diagnostics.push(Diagnostic::node(
                    Severity::Error,
                    codes::UNRESOLVED_TOKEN,
                    format!(
                        "token {id} with mode context {mode_context:?} is not present in the extraction bundle"
                    ),
                    &raw.id,
                    Some(path),
                ));
                return TokenRef {
                    id: id.to_owned(),
                    name: None,
                    collection_id: None,
                    mode_id: None,
                };
            };
            token.clone()
        })
    }

    fn normalize_text(&mut self, raw: &RawNode, text: &RawText) -> Text {
        if text.max_lines == Some(0) {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_TEXT_LAYOUT,
                "Text max lines must be a positive integer when present",
                &raw.id,
                Some("text.max_lines"),
            ));
        }
        let runs = text
            .runs
            .iter()
            .filter_map(|run| {
                let range =
                    utf16_range_to_byte_range(&text.characters, run.start_utf16, run.end_utf16);
                if let Some(range) = range {
                    Some(TextRun {
                        start_utf16: run.start_utf16,
                        end_utf16: run.end_utf16,
                        text: text.characters[range].to_owned(),
                        style: self.normalize_text_style(raw, &run.style),
                    })
                } else {
                    self.diagnostics.push(Diagnostic::node(
                        Severity::Error,
                        codes::INVALID_TEXT_RANGE,
                        format!(
                            "UTF-16 text range {}..{} is outside the text or splits a scalar value",
                            run.start_utf16, run.end_utf16
                        ),
                        &raw.id,
                        Some("text.runs"),
                    ));
                    None
                }
            })
            .collect();
        Text {
            characters: text.characters.clone(),
            auto_resize: text.auto_resize,
            horizontal_alignment: text.horizontal_alignment,
            vertical_alignment: text.vertical_alignment,
            truncation: text.truncation,
            max_lines: text.max_lines,
            runs,
        }
    }

    fn normalize_text_style(&mut self, raw: &RawNode, style: &RawTextStyle) -> TextStyle {
        TextStyle {
            font_family: style.font_family.clone(),
            font_style: style.font_style.clone(),
            font_size: style
                .font_size
                .as_ref()
                .map(|value| self.normalize_number_bound(raw, value, "text.runs.style.font_size")),
            font_weight: style.font_weight,
            line_height: style
                .line_height
                .map(|value| self.finite_non_negative(raw, value, "text.runs.style.line_height")),
            letter_spacing: style
                .letter_spacing
                .map(|value| self.finite_or_zero(raw, value, "text.runs.style.letter_spacing")),
            color: style
                .color
                .as_ref()
                .map(|value| self.normalize_color_bound(raw, value, "text.runs.style.color")),
        }
    }

    fn normalize_component(
        &mut self,
        raw: &RawNode,
        component: &RawComponentMetadata,
    ) -> ComponentMetadata {
        let role_matches = matches!(
            (raw.kind, component.role),
            (RawNodeKind::Component, RawComponentRole::Component)
                | (RawNodeKind::Instance, RawComponentRole::Instance)
        );
        if !role_matches {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::COMPONENT_METADATA_MISMATCH,
                "node kind and component metadata role do not match",
                &raw.id,
                Some("component.role"),
            ));
        }

        let resolution = match (raw.kind, component.role) {
            (RawNodeKind::Component, RawComponentRole::Component) => {
                ComponentResolution::Definition
            }
            (RawNodeKind::Instance, RawComponentRole::Instance) => {
                if let Some(mapping) = self.registry.mapping_for(component) {
                    ComponentResolution::Mapped {
                        mapping_id: mapping.id.clone(),
                    }
                } else {
                    self.diagnostics.push(Diagnostic::node(
                        Severity::Warning,
                        codes::UNMAPPED_COMPONENT,
                        format!(
                            "component {} has no semantic mapping; structural children are retained",
                            component.component_key
                        ),
                        &raw.id,
                        Some("component.component_key"),
                    ));
                    ComponentResolution::StructuralFallback
                }
            }
            (RawNodeKind::Instance, RawComponentRole::Component) => {
                ComponentResolution::StructuralFallback
            }
            (_, RawComponentRole::Component) => ComponentResolution::Definition,
            (_, RawComponentRole::Instance) => ComponentResolution::StructuralFallback,
        };
        ComponentMetadata {
            role: component.role,
            component_key: component.component_key.clone(),
            component_set_key: component.component_set_key.clone(),
            variants: component.variants.clone(),
            properties: component.properties.clone(),
            overrides: component.overrides.clone(),
            resolution,
        }
    }

    fn decide_asset(&self, raw: &RawNode, positioning: &Positioning) -> AssetDecision {
        let mut route = AssetRoute::Native;
        let mut reasons = Vec::new();
        let mut require = |required: AssetRoute, reason: &str| {
            route = route.max(required);
            reasons.push(reason.to_owned());
        };

        if raw.kind == RawNodeKind::Vector || raw.style.is_mask {
            require(AssetRoute::Svg, "vector or mask semantics");
        }
        if raw.layout.mode == RawLayoutMode::Grid
            && (!tracks_are_uniform(&raw.layout.grid.columns)
                || !tracks_are_uniform(&raw.layout.grid.rows))
        {
            require(AssetRoute::Runtime, "mixed grid tracks");
        }
        if raw.position.grid.is_some() {
            require(AssetRoute::Runtime, "manual grid placement");
        }
        if matches!(
            raw.layout.mode,
            RawLayoutMode::Horizontal | RawLayoutMode::Vertical
        ) && raw.layout.counter_alignment == crate::raw::RawAlignment::Baseline
        {
            require(AssetRoute::Runtime, "stack baseline alignment");
        }
        if has_parent_size_aware_constraints(positioning) {
            require(
                AssetRoute::Runtime,
                "parent-size-aware absolute constraints",
            );
        }
        if !raw.style.strokes.is_empty()
            && raw.style.stroke_align != crate::raw::RawStrokeAlign::Inside
        {
            require(AssetRoute::Runtime, "center/outside stroke alignment");
        }
        if raw.style.strokes.len() > 1 {
            require(AssetRoute::Svg, "multiple stroke paints");
        }
        if raw.style.radii.smoothing > 0.0 {
            require(AssetRoute::Runtime, "corner smoothing");
        }
        match raw.style.blend_mode {
            crate::raw::RawBlendMode::Normal => {}
            crate::raw::RawBlendMode::PassThrough => {
                if pass_through_requires_runtime(raw) {
                    require(AssetRoute::Runtime, "pass-through compositing");
                }
            }
            _ => require(AssetRoute::Raster, "non-normal blend mode"),
        }
        for paint in raw.style.fills.iter().chain(&raw.style.strokes) {
            if let RawPaint::Image { asset_id, .. } = paint
                && !self.assets.contains(asset_id.as_str())
            {
                require(AssetRoute::Raster, "missing extracted image asset");
                continue;
            }
            match paint {
                RawPaint::Solid { .. } => {}
                RawPaint::Image { .. } if image_paint_is_native(paint) => {}
                RawPaint::Gradient {
                    gradient_kind: crate::raw::RawGradientKind::Linear,
                    stops,
                } if stops.len() == 2 => {}
                RawPaint::Gradient { .. } => {
                    require(AssetRoute::Runtime, "rich gradient paint");
                }
                RawPaint::Image { .. } => {
                    require(AssetRoute::Runtime, "image crop/tile semantics");
                }
                RawPaint::Pattern => require(AssetRoute::Svg, "pattern paint"),
                RawPaint::Video => require(AssetRoute::Raster, "video paint placeholder"),
                RawPaint::Shader => require(AssetRoute::Raster, "shader paint"),
            }
        }
        for effect in &raw.style.effects {
            if !matches!(
                effect,
                RawEffect::DropShadow { .. } | RawEffect::InnerShadow { .. }
            ) {
                require(AssetRoute::Raster, effect_kind(effect));
            }
        }
        if raw.text.as_ref().is_some_and(|text| {
            text.runs.iter().any(|run| {
                run.style
                    .letter_spacing
                    .is_some_and(|spacing| spacing != 0.0)
            })
        }) {
            require(AssetRoute::Svg, "letter spacing");
        }

        AssetDecision { route, reasons }
    }

    fn emit_asset_diagnostic(&mut self, raw: &RawNode, decision: &AssetDecision) {
        let (severity, code, route_name) = match decision.route {
            AssetRoute::Native => return,
            AssetRoute::Runtime => (Severity::Error, codes::RUNTIME_FALLBACK, "runtime"),
            AssetRoute::Svg => (Severity::Warning, codes::SVG_FALLBACK, "SVG"),
            AssetRoute::Raster => (Severity::Warning, codes::RASTER_FALLBACK, "raster"),
        };
        let property_path = if decision.route == AssetRoute::Runtime {
            "asset_decision.route"
        } else {
            "asset_decision"
        };
        let mut diagnostic = Diagnostic::node(
            severity,
            code,
            format!(
                "node requires {route_name} lowering: {}",
                decision.reasons.join(", ")
            ),
            &raw.id,
            Some(property_path),
        );
        if decision.route == AssetRoute::Runtime {
            diagnostic.help = Some(
                "Use a source SVG/raster fallback when semantically valid, or add a verified runtime lowering before compilation."
                    .to_owned(),
            );
        }
        self.diagnostics.push(diagnostic);
    }

    fn normalize_edges(
        &mut self,
        raw: &RawNode,
        edges: &crate::raw::RawEdges,
        path: &str,
    ) -> Edges {
        Edges {
            top: self.normalize_non_negative_number_bound(raw, &edges.top, &format!("{path}.top")),
            right: self.normalize_non_negative_number_bound(
                raw,
                &edges.right,
                &format!("{path}.right"),
            ),
            bottom: self.normalize_non_negative_number_bound(
                raw,
                &edges.bottom,
                &format!("{path}.bottom"),
            ),
            left: self.normalize_non_negative_number_bound(
                raw,
                &edges.left,
                &format!("{path}.left"),
            ),
        }
    }

    fn normalize_variable_value(&mut self, variable: &RawVariable) -> crate::raw::RawLiteral {
        use crate::raw::RawLiteral;

        match &variable.value {
            RawLiteral::Number(value) => {
                RawLiteral::Number(self.variable_number(variable, *value, None, "value"))
            }
            RawLiteral::Color(color) => RawLiteral::Color(RawColor {
                r: self.variable_number(variable, color.r, Some((0.0, 1.0)), "value.r"),
                g: self.variable_number(variable, color.g, Some((0.0, 1.0)), "value.g"),
                b: self.variable_number(variable, color.b, Some((0.0, 1.0)), "value.b"),
                a: self.variable_number(variable, color.a, Some((0.0, 1.0)), "value.a"),
            }),
            RawLiteral::String(value) => RawLiteral::String(value.clone()),
            RawLiteral::Boolean(value) => RawLiteral::Boolean(*value),
        }
    }

    fn variable_number(
        &mut self,
        variable: &RawVariable,
        value: f64,
        range: Option<(f64, f64)>,
        path: &str,
    ) -> f64 {
        let valid = value.is_finite()
            && range.is_none_or(|(minimum, maximum)| (minimum..=maximum).contains(&value));
        if valid {
            return if value.to_bits() == (-0.0_f64).to_bits() {
                0.0
            } else {
                value
            };
        }

        let message = range.map_or_else(
            || "variable number must be finite".to_owned(),
            |(minimum, maximum)| {
                format!("variable color channel must be between {minimum} and {maximum}")
            },
        );
        let property_path = format!("variables.{}.{}", variable.id, path);
        let diagnostic = if let Some(node_id) = variable.source_node_id.as_deref() {
            Diagnostic::node(
                Severity::Error,
                codes::INVALID_NUMBER,
                message,
                node_id,
                Some(&property_path),
            )
        } else {
            Diagnostic::bundle(Severity::Error, codes::INVALID_NUMBER, message)
        };
        self.diagnostics.push(diagnostic);

        if value.is_finite() {
            range.map_or(value, |(minimum, maximum)| value.clamp(minimum, maximum))
        } else {
            0.0
        }
    }

    fn finite_or_zero(&mut self, raw: &RawNode, value: f64, path: &str) -> f64 {
        if value.is_finite() {
            if value.to_bits() == (-0.0_f64).to_bits() {
                0.0
            } else {
                value
            }
        } else {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_NUMBER,
                "value must be finite",
                &raw.id,
                Some(path),
            ));
            0.0
        }
    }

    fn finite_non_negative(&mut self, raw: &RawNode, value: f64, path: &str) -> f64 {
        let finite = self.finite_or_zero(raw, value, path);
        if finite >= 0.0 {
            finite
        } else {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_NUMBER,
                "value must be non-negative",
                &raw.id,
                Some(path),
            ));
            0.0
        }
    }

    fn finite_in_range(
        &mut self,
        raw: &RawNode,
        value: f64,
        minimum: f64,
        maximum: f64,
        path: &str,
    ) -> f64 {
        let finite = self.finite_or_zero(raw, value, path);
        if (minimum..=maximum).contains(&finite) {
            finite
        } else {
            self.diagnostics.push(Diagnostic::node(
                Severity::Error,
                codes::INVALID_NUMBER,
                format!("value must be between {minimum} and {maximum}"),
                &raw.id,
                Some(path),
            ));
            finite.clamp(minimum, maximum)
        }
    }
}

fn sorted_variables<'a>(
    bundle: &'a ExtractionBundle,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<&'a RawVariable> {
    let mut variables = bundle.variables.iter().collect::<Vec<_>>();
    variables.sort_by(|left, right| compare_variables(left, right));
    for variable in &variables {
        if bundle.schema_version == EXTRACTION_SCHEMA_VERSION
            && variable.mode_context.get(&variable.collection_id) != Some(&variable.mode_id)
        {
            diagnostics.push(Diagnostic::bundle(
                Severity::Error,
                codes::INVALID_VARIABLE_MODE_CONTEXT,
                format!(
                    "variable {} mode {} does not match its mode context",
                    variable.id, variable.mode_id
                ),
            ));
        }
    }
    for duplicate in variables.windows(2).filter(|pair| {
        pair[0].id == pair[1].id
            && (bundle.schema_version != EXTRACTION_SCHEMA_VERSION
                || pair[0].mode_context == pair[1].mode_context)
    }) {
        diagnostics.push(Diagnostic::bundle(
            Severity::Error,
            codes::DUPLICATE_METADATA_ID,
            format!(
                "duplicate variable ID {} for mode context {:?}",
                duplicate[0].id, duplicate[0].mode_context
            ),
        ));
    }
    variables
}

fn sorted_components(
    bundle: &ExtractionBundle,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<crate::raw::RawComponent> {
    let mut components = bundle.components.clone();
    components.sort_by(compare_components);
    for duplicate in components
        .windows(2)
        .filter(|pair| pair[0].key == pair[1].key)
    {
        diagnostics.push(Diagnostic::bundle(
            Severity::Error,
            codes::DUPLICATE_METADATA_ID,
            format!("duplicate component key {}", duplicate[0].key),
        ));
    }
    components
}

fn sorted_assets(
    bundle: &ExtractionBundle,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<crate::raw::RawAsset> {
    let mut assets = bundle.assets.clone();
    assets.sort_by(compare_assets);
    for duplicate in assets.windows(2).filter(|pair| pair[0].id == pair[1].id) {
        diagnostics.push(Diagnostic::bundle(
            Severity::Error,
            codes::DUPLICATE_METADATA_ID,
            format!("duplicate asset ID {}", duplicate[0].id),
        ));
    }
    assets
}

fn compare_variables(left: &RawVariable, right: &RawVariable) -> Ordering {
    left.id
        .cmp(&right.id)
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.collection_id.cmp(&right.collection_id))
        .then_with(|| left.mode_id.cmp(&right.mode_id))
        .then_with(|| left.mode_context.cmp(&right.mode_context))
        .then_with(|| left.source_node_id.cmp(&right.source_node_id))
        .then_with(|| compare_literals(&left.value, &right.value))
}

fn variable_token(variable: &RawVariable) -> TokenRef {
    TokenRef {
        id: variable.id.clone(),
        name: Some(variable.name.clone()),
        collection_id: Some(variable.collection_id.clone()),
        mode_id: Some(variable.mode_id.clone()),
    }
}

fn compare_literals(left: &crate::raw::RawLiteral, right: &crate::raw::RawLiteral) -> Ordering {
    use crate::raw::RawLiteral;

    let rank = |literal: &RawLiteral| match literal {
        RawLiteral::Number(_) => 0_u8,
        RawLiteral::Color(_) => 1,
        RawLiteral::String(_) => 2,
        RawLiteral::Boolean(_) => 3,
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| match (left, right) {
            (RawLiteral::Number(left), RawLiteral::Number(right)) => left.total_cmp(right),
            (RawLiteral::Color(left), RawLiteral::Color(right)) => left
                .r
                .total_cmp(&right.r)
                .then_with(|| left.g.total_cmp(&right.g))
                .then_with(|| left.b.total_cmp(&right.b))
                .then_with(|| left.a.total_cmp(&right.a)),
            (RawLiteral::String(left), RawLiteral::String(right)) => left.cmp(right),
            (RawLiteral::Boolean(left), RawLiteral::Boolean(right)) => left.cmp(right),
            _ => Ordering::Equal,
        })
}

fn compare_components(
    left: &crate::raw::RawComponent,
    right: &crate::raw::RawComponent,
) -> Ordering {
    left.key
        .cmp(&right.key)
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.set_key.cmp(&right.set_key))
        .then_with(|| left.property_definitions.cmp(&right.property_definitions))
}

fn compare_assets(left: &crate::raw::RawAsset, right: &crate::raw::RawAsset) -> Ordering {
    left.id
        .cmp(&right.id)
        .then_with(|| left.source_node_id.cmp(&right.source_node_id))
        .then_with(|| left.media_type.cmp(&right.media_type))
        .then_with(|| left.content_hash.cmp(&right.content_hash))
        .then_with(|| left.export_settings.cmp(&right.export_settings))
        .then_with(|| left.payload_base64.cmp(&right.payload_base64))
}

fn validate_node_ids(
    node: &RawNode,
    node_ids: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !node_ids.insert(node.id.clone()) {
        diagnostics.push(Diagnostic::node(
            Severity::Error,
            codes::DUPLICATE_NODE_ID,
            format!("duplicate source node ID {}", node.id),
            &node.id,
            Some("id"),
        ));
    }
    for child in &node.children {
        validate_node_ids(child, node_ids, diagnostics);
    }
}

fn effect_kind(effect: &RawEffect) -> &'static str {
    match effect {
        RawEffect::DropShadow { .. } => "DROP_SHADOW",
        RawEffect::InnerShadow { .. } => "INNER_SHADOW",
        RawEffect::LayerBlur { .. } => "LAYER_BLUR",
        RawEffect::BackgroundBlur { .. } => "BACKGROUND_BLUR",
        RawEffect::ProgressiveBlur => "PROGRESSIVE_BLUR",
        RawEffect::Noise => "NOISE",
        RawEffect::Texture => "TEXTURE",
        RawEffect::Glass => "GLASS",
        RawEffect::Shader => "SHADER",
    }
}

fn tracks_are_uniform(tracks: &[RawGridTrack]) -> bool {
    let Some(first) = tracks.first() else {
        return true;
    };
    tracks.iter().skip(1).all(|track| match (first, track) {
        (RawGridTrack::Fixed(left), RawGridTrack::Fixed(right))
        | (RawGridTrack::Flex(left), RawGridTrack::Flex(right)) => {
            canonical_number_bits(*left) == canonical_number_bits(*right)
        }
        (RawGridTrack::Hug, RawGridTrack::Hug) => true,
        _ => false,
    })
}

fn axis_aligned_crop(matrix: [f64; 6]) -> bool {
    let [scale_x, skew_y, skew_x, scale_y, translate_x, translate_y] = matrix;
    matrix.iter().all(|value| value.is_finite())
        && scale_x > 0.0
        && scale_y > 0.0
        && skew_x.abs() <= f64::EPSILON
        && skew_y.abs() <= f64::EPSILON
        && translate_x >= 0.0
        && translate_y >= 0.0
}

fn image_paint_is_native(paint: &RawPaint) -> bool {
    match paint {
        RawPaint::Image {
            scale_mode: crate::raw::RawImageScaleMode::Fit | crate::raw::RawImageScaleMode::Fill,
            rotation,
            has_filters: false,
            ..
        } => rotation.is_none_or(|rotation| rotation.abs() <= f64::EPSILON),
        RawPaint::Image {
            scale_mode: crate::raw::RawImageScaleMode::Crop,
            image_transform: Some(transform),
            rotation,
            has_filters: false,
            ..
        } => {
            rotation.is_none_or(|rotation| rotation.abs() <= f64::EPSILON)
                && axis_aligned_crop(transform.matrix)
        }
        _ => false,
    }
}

fn canonical_number_bits(value: f64) -> u64 {
    let bits = value.to_bits();
    if bits.trailing_zeros() >= 63 { 0 } else { bits }
}

fn utf16_range_to_byte_range(
    text: &str,
    start_utf16: u32,
    end_utf16: u32,
) -> Option<std::ops::Range<usize>> {
    if start_utf16 > end_utf16 {
        return None;
    }
    let mut utf16_offset = 0_u32;
    let mut start_byte = (start_utf16 == 0).then_some(0);
    let mut end_byte = (end_utf16 == 0).then_some(0);

    for (byte_index, character) in text.char_indices() {
        if utf16_offset == start_utf16 {
            start_byte = Some(byte_index);
        }
        if utf16_offset == end_utf16 {
            end_byte = Some(byte_index);
        }
        utf16_offset = utf16_offset.checked_add(u32::try_from(character.len_utf16()).ok()?)?;
    }
    if utf16_offset == start_utf16 {
        start_byte = Some(text.len());
    }
    if utf16_offset == end_utf16 {
        end_byte = Some(text.len());
    }
    start_byte.zip(end_byte).map(|(start, end)| start..end)
}

#[cfg(test)]
mod tests {
    use super::utf16_range_to_byte_range;

    #[test]
    fn utf16_ranges_do_not_split_surrogate_pairs() {
        let text = "A😀B";
        assert_eq!(utf16_range_to_byte_range(text, 1, 3), Some(1..5));
        assert_eq!(utf16_range_to_byte_range(text, 1, 2), None);
    }
}
