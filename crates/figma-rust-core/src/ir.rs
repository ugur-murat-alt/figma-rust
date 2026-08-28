use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::raw::{
    RawAction, RawAlignment, RawAsset, RawBlendMode, RawChildAlignment, RawComponent,
    RawComponentRole, RawComponentValue, RawConstraint, RawGradientKind, RawImageScaleMode,
    RawLiteral, RawNodeKind, RawOverride, RawReaction, RawSource, RawStrokeAlign,
    RawTextAutoResize, RawTextHorizontalAlignment, RawTextTruncation, RawTextVerticalAlignment,
    RawTrigger,
};

pub const DESIGN_IR_VERSION: u32 = 2;

/// Stable, target-neutral normalized document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignDocument {
    pub ir_version: u32,
    pub source: RawSource,
    pub roots: Vec<Node>,
    pub variables: Vec<Variable>,
    pub components: Vec<RawComponent>,
    pub assets: Vec<RawAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub source_id: String,
    pub name: String,
    pub kind: RawNodeKind,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_binding: Option<BoundValue<bool>>,
    pub opacity: f64,
    pub size: Size,
    pub layout: Layout,
    pub positioning: Positioning,
    #[serde(default, skip_serializing_if = "RawChildAlignment::is_inherit")]
    pub child_counter_alignment: RawChildAlignment,
    pub style: Style,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentMetadata>,
    pub reactions: Vec<Reaction>,
    pub asset_decision: AssetDecision,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub horizontal: AxisSize,
    pub vertical: AxisSize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisSize {
    pub sizing: AxisSizing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<BoundValue<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<BoundValue<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<BoundValue<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AxisSizing {
    Hug,
    Fill,
    Fixed(BoundValue<f64>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Layout {
    Plain {
        clips_content: bool,
        scroll: Scroll,
    },
    Absolute {
        clips_content: bool,
        scroll: Scroll,
    },
    Stack {
        axis: Axis,
        wrap: bool,
        primary_alignment: RawAlignment,
        counter_alignment: RawAlignment,
        gap: BoundValue<f64>,
        padding: Edges,
        clips_content: bool,
        scroll: Scroll,
    },
    Grid {
        columns: Vec<GridTrack>,
        rows: Vec<GridTrack>,
        column_gap: f64,
        row_gap: f64,
        padding: Edges,
        clips_content: bool,
        scroll: Scroll,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GridTrack {
    Fixed(f64),
    Flex(f64),
    Hug,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Positioning {
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        grid: Option<GridPlacement>,
        transform: Transform,
    },
    Absolute {
        x: f64,
        y: f64,
        horizontal_constraint: RawConstraint,
        vertical_constraint: RawConstraint,
        transform: Transform,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridPlacement {
    pub row: u32,
    pub column: u32,
    pub row_span: u32,
    pub column_span: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub matrix: [f64; 6],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edges {
    pub top: BoundValue<f64>,
    pub right: BoundValue<f64>,
    pub bottom: BoundValue<f64>,
    pub left: BoundValue<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scroll {
    pub horizontal: bool,
    pub vertical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fills: Vec<Paint>,
    pub strokes: Vec<Paint>,
    pub stroke_widths: Edges,
    pub stroke_align: RawStrokeAlign,
    pub radii: Radii,
    pub effects: Vec<Effect>,
    pub blend_mode: RawBlendMode,
    pub is_mask: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Paint {
    Solid {
        color: BoundValue<Color>,
    },
    Gradient {
        gradient_kind: RawGradientKind,
        stops: Vec<GradientStop>,
    },
    Image {
        asset_id: String,
        scale_mode: RawImageScaleMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_transform: Option<Transform>,
        opacity: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
        has_filters: bool,
    },
    Unsupported {
        source_kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f64,
    pub color: BoundValue<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundValue<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mode_context: BTreeMap<String, String>,
    pub fallback: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Effect {
    Shadow {
        inset: bool,
        color: BoundValue<Color>,
        offset_x: f64,
        offset_y: f64,
        blur: f64,
        spread: f64,
    },
    Unsupported {
        source_kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Radii {
    pub top_left: BoundValue<f64>,
    pub top_right: BoundValue<f64>,
    pub bottom_right: BoundValue<f64>,
    pub bottom_left: BoundValue<f64>,
    pub smoothing: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub characters: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters_binding: Option<BoundValue<String>>,
    #[serde(default, skip_serializing_if = "crate::raw::is_default")]
    pub auto_resize: RawTextAutoResize,
    #[serde(default, skip_serializing_if = "crate::raw::is_default")]
    pub horizontal_alignment: RawTextHorizontalAlignment,
    #[serde(default, skip_serializing_if = "crate::raw::is_default")]
    pub vertical_alignment: RawTextVerticalAlignment,
    #[serde(default, skip_serializing_if = "crate::raw::is_default")]
    pub truncation: RawTextTruncation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u32>,
    pub runs: Vec<TextRun>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub start_utf16: u32,
    pub end_utf16: u32,
    pub text: String,
    pub style: TextStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<BoundValue<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<BoundValue<Color>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub role: RawComponentRole,
    pub component_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_set_key: Option<String>,
    pub variants: BTreeMap<String, String>,
    pub properties: BTreeMap<String, RawComponentValue>,
    pub overrides: Vec<RawOverride>,
    pub resolution: ComponentResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentResolution {
    Definition,
    Mapped { mapping_id: String },
    StructuralFallback,
}

pub type Reaction = RawReaction;
pub type Trigger = RawTrigger;
pub type Action = RawAction;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub token: TokenRef,
    pub mode_context: BTreeMap<String, String>,
    pub value: RawLiteral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDecision {
    pub route: AssetRoute,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetRoute {
    Native,
    Runtime,
    Svg,
    Raster,
}
