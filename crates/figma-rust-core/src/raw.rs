use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Diagnostic;

/// Versioned extraction payload shared by the plugin and future REST importer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionBundle {
    pub schema_version: u32,
    pub source: RawSource,
    #[serde(default)]
    pub roots: Vec<RawNode>,
    #[serde(default)]
    pub variables: Vec<RawVariable>,
    #[serde(default)]
    pub components: Vec<RawComponent>,
    #[serde(default)]
    pub assets: Vec<RawAsset>,
    #[serde(default)]
    pub extraction_diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_snapshot: Option<Value>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_key: Option<String>,
    pub page_id: String,
    #[serde(default)]
    pub selected_node_ids: Vec<String>,
    pub plugin_api_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawNode {
    pub id: String,
    pub name: String,
    pub kind: RawNodeKind,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub layout: RawLayout,
    #[serde(default)]
    pub size: RawSize,
    #[serde(default)]
    pub position: RawPosition,
    #[serde(default)]
    pub style: RawStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<RawText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<RawComponentMetadata>,
    #[serde(default)]
    pub reactions: Vec<RawReaction>,
    #[serde(default)]
    pub children: Vec<RawNode>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}

const fn default_true() -> bool {
    true
}

const fn default_opacity() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawNodeKind {
    Frame,
    Group,
    Rectangle,
    Ellipse,
    Text,
    Vector,
    Image,
    Component,
    Instance,
    Scroll,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawLayout {
    #[serde(default)]
    pub mode: RawLayoutMode,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub primary_alignment: RawAlignment,
    #[serde(default)]
    pub counter_alignment: RawAlignment,
    #[serde(default)]
    pub gap: RawBoundValue<f64>,
    #[serde(default)]
    pub padding: RawEdges,
    #[serde(default)]
    pub grid: RawGrid,
    #[serde(default)]
    pub clips_content: bool,
    #[serde(default)]
    pub scroll: RawScroll,
}

impl Default for RawLayout {
    fn default() -> Self {
        Self {
            mode: RawLayoutMode::None,
            wrap: false,
            primary_alignment: RawAlignment::Start,
            counter_alignment: RawAlignment::Start,
            gap: RawBoundValue::default(),
            padding: RawEdges::default(),
            grid: RawGrid::default(),
            clips_content: false,
            scroll: RawScroll::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawLayoutMode {
    #[default]
    None,
    Horizontal,
    Vertical,
    Grid,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawAlignment {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    Baseline,
    Stretch,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawEdges {
    #[serde(default)]
    pub top: RawBoundValue<f64>,
    #[serde(default)]
    pub right: RawBoundValue<f64>,
    #[serde(default)]
    pub bottom: RawBoundValue<f64>,
    #[serde(default)]
    pub left: RawBoundValue<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawSize {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<RawAxisSizing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<RawAxisSizing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawAxisSizing {
    Hug,
    Fill,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawPosition {
    #[serde(default)]
    pub positioning: RawPositioning,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub horizontal_constraint: RawConstraint,
    #[serde(default)]
    pub vertical_constraint: RawConstraint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<RawGridPlacement>,
    #[serde(default)]
    pub transform: RawTransform,
}

impl Default for RawPosition {
    fn default() -> Self {
        Self {
            positioning: RawPositioning::Auto,
            x: 0.0,
            y: 0.0,
            horizontal_constraint: RawConstraint::Min,
            vertical_constraint: RawConstraint::Min,
            grid: None,
            transform: RawTransform::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawPositioning {
    #[default]
    Auto,
    Absolute,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawConstraint {
    #[default]
    Min,
    Center,
    Max,
    Stretch,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RawTransform {
    pub matrix: [f64; 6],
}

impl Default for RawTransform {
    fn default() -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawGrid {
    #[serde(default)]
    pub columns: Vec<RawGridTrack>,
    #[serde(default)]
    pub rows: Vec<RawGridTrack>,
    #[serde(default)]
    pub column_gap: f64,
    #[serde(default)]
    pub row_gap: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawGridTrack {
    Fixed(f64),
    Flex(f64),
    Hug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawGridPlacement {
    pub row: u32,
    pub column: u32,
    #[serde(default = "one")]
    pub row_span: u32,
    #[serde(default = "one")]
    pub column_span: u32,
}

const fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawScroll {
    #[serde(default)]
    pub horizontal: bool,
    #[serde(default)]
    pub vertical: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawStyle {
    #[serde(default)]
    pub fills: Vec<RawPaint>,
    #[serde(default)]
    pub strokes: Vec<RawPaint>,
    #[serde(default)]
    pub stroke_widths: RawEdges,
    #[serde(default)]
    pub stroke_align: RawStrokeAlign,
    #[serde(default)]
    pub radii: RawRadii,
    #[serde(default)]
    pub effects: Vec<RawEffect>,
    #[serde(default)]
    pub blend_mode: RawBlendMode,
    #[serde(default)]
    pub is_mask: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawPaint {
    Solid {
        color: RawBoundValue<RawColor>,
    },
    Gradient {
        gradient_kind: RawGradientKind,
        stops: Vec<RawGradientStop>,
    },
    Image {
        asset_id: String,
        scale_mode: RawImageScaleMode,
    },
    Video,
    Pattern,
    Shader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawGradientKind {
    Linear,
    Radial,
    Angular,
    Diamond,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawGradientStop {
    pub position: f64,
    pub color: RawBoundValue<RawColor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawImageScaleMode {
    Fit,
    Fill,
    Crop,
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RawColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    #[serde(default = "default_opacity")]
    pub a: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawBoundValue<T> {
    pub literal: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mode_context: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawEffect {
    DropShadow {
        color: RawBoundValue<RawColor>,
        offset_x: f64,
        offset_y: f64,
        blur: f64,
        spread: f64,
    },
    InnerShadow {
        color: RawBoundValue<RawColor>,
        offset_x: f64,
        offset_y: f64,
        blur: f64,
        spread: f64,
    },
    LayerBlur {
        radius: f64,
    },
    BackgroundBlur {
        radius: f64,
    },
    ProgressiveBlur,
    Noise,
    Texture,
    Glass,
    Shader,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawStrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawBlendMode {
    #[default]
    Normal,
    PassThrough,
    Multiply,
    Screen,
    Overlay,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawRadii {
    #[serde(default)]
    pub top_left: RawBoundValue<f64>,
    #[serde(default)]
    pub top_right: RawBoundValue<f64>,
    #[serde(default)]
    pub bottom_right: RawBoundValue<f64>,
    #[serde(default)]
    pub bottom_left: RawBoundValue<f64>,
    #[serde(default)]
    pub smoothing: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawText {
    pub characters: String,
    #[serde(default)]
    pub runs: Vec<RawTextRun>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTextRun {
    pub start_utf16: u32,
    pub end_utf16: u32,
    #[serde(default)]
    pub style: RawTextStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawTextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<RawBoundValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<RawBoundValue<RawColor>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawComponentMetadata {
    pub role: RawComponentRole,
    pub component_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_set_key: Option<String>,
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default)]
    pub properties: BTreeMap<String, RawComponentValue>,
    #[serde(default)]
    pub overrides: Vec<RawOverride>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawComponentRole {
    Component,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawComponentValue {
    Variant(String),
    Text(String),
    Boolean(bool),
    InstanceSwap(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawOverride {
    pub node_id: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawReaction {
    pub trigger: RawTrigger,
    pub action: RawAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawTrigger {
    Click,
    Hover,
    Press,
    Key { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawAction {
    Emit { name: String },
    Navigate { destination_id: String },
    OpenOverlay { destination_id: String },
    SmartAnimate { destination_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawVariable {
    pub id: String,
    pub name: String,
    pub collection_id: String,
    pub mode_id: String,
    #[serde(default)]
    pub mode_context: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    pub value: RawLiteral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RawLiteral {
    Number(f64),
    Color(RawColor),
    String(String),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawComponent {
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_key: Option<String>,
    #[serde(default)]
    pub property_definitions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAsset {
    pub id: String,
    pub source_node_id: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub export_settings: BTreeMap<String, String>,
}
