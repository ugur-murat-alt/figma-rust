use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const AUTHORING_SCHEMA_VERSION: u32 = 1;

/// Editable, Figma-independent design document.
///
/// Child order is stored explicitly while maps use sorted keys, so canonical
/// serialization remains deterministic without relying on hash-map iteration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthoringDocument {
    pub schema_version: u32,
    pub document_id: String,
    pub name: String,
    pub revision: u64,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub nodes: BTreeMap<String, DesignNode>,
    #[serde(default)]
    pub tokens: BTreeMap<String, DesignToken>,
    #[serde(default)]
    pub components: BTreeMap<String, ComponentContract>,
    #[serde(default)]
    pub code_bindings: BTreeMap<String, CodeBinding>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl AuthoringDocument {
    #[must_use]
    pub fn new(document_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: AUTHORING_SCHEMA_VERSION,
            document_id: document_id.into(),
            name: name.into(),
            revision: 0,
            roots: Vec::new(),
            nodes: BTreeMap::new(),
            tokens: BTreeMap::new(),
            components: BTreeMap::new(),
            code_bindings: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub layout: LayoutSpec,
    #[serde(default)]
    pub visual: VisualStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentInstance>,
    #[serde(default)]
    pub token_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl DesignNode {
    #[must_use]
    pub fn container(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: NodeKind::Container,
            parent: None,
            children: Vec::new(),
            visible: true,
            locked: false,
            layout: LayoutSpec::default(),
            visual: VisualStyle::default(),
            text: None,
            component: None,
            token_bindings: BTreeMap::new(),
            tags: BTreeSet::new(),
            metadata: BTreeMap::new(),
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeKind {
    Canvas,
    Container,
    Text,
    Shape,
    Image,
    ComponentInstance,
    Slot,
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSpec {
    pub flow: LayoutFlow,
    pub horizontal: SizingRule,
    pub vertical: SizingRule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<DesignValue<f64>>,
    pub padding: EdgeValues<DesignValue<f64>>,
    pub alignment: Alignment,
    pub cross_alignment: CrossAlignment,
    pub distribution: Distribution,
    pub position: PositionSpec,
    pub scroll: ScrollSpec,
    #[serde(default)]
    pub clip: bool,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            flow: LayoutFlow::None,
            horizontal: SizingRule::Hug,
            vertical: SizingRule::Hug,
            gap: None,
            padding: EdgeValues::uniform(DesignValue::literal(0.0)),
            alignment: Alignment::Start,
            cross_alignment: CrossAlignment::Start,
            distribution: Distribution::Start,
            position: PositionSpec::Flow,
            scroll: ScrollSpec::default(),
            clip: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LayoutFlow {
    None,
    Horizontal { wrap: bool },
    Vertical { wrap: bool },
    Grid { spec: GridSpec },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    #[serde(default)]
    pub columns: Vec<GridTrack>,
    #[serde(default)]
    pub rows: Vec<GridTrack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_gap: Option<DesignValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_gap: Option<DesignValue<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GridTrack {
    Fixed(DesignValue<f64>),
    Flex(f64),
    Hug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SizingRule {
    Hug,
    Fill,
    Fixed {
        value: DesignValue<f64>,
    },
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<DesignValue<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preferred: Option<DesignValue<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<DesignValue<f64>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionSpec {
    Flow,
    Absolute {
        x: DesignValue<f64>,
        y: DesignValue<f64>,
        #[serde(default)]
        z_index: i32,
    },
    WindowFixed {
        x: DesignValue<f64>,
        y: DesignValue<f64>,
        #[serde(default)]
        z_index: i32,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrollSpec {
    #[serde(default)]
    pub horizontal: bool,
    #[serde(default)]
    pub vertical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Alignment {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAlignment {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Distribution {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignValue<T> {
    pub literal: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
}

impl<T> DesignValue<T> {
    #[must_use]
    pub const fn literal(literal: T) -> Self {
        Self {
            literal,
            token_id: None,
        }
    }

    #[must_use]
    pub fn bound(literal: T, token_id: impl Into<String>) -> Self {
        Self {
            literal,
            token_id: Some(token_id.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeValues<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Clone> EdgeValues<T> {
    #[must_use]
    pub fn uniform(value: T) -> Self {
        Self {
            top: value.clone(),
            right: value.clone(),
            bottom: value.clone(),
            left: value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadiusValues<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

impl<T: Clone> RadiusValues<T> {
    #[must_use]
    pub fn uniform(value: T) -> Self {
        Self {
            top_left: value.clone(),
            top_right: value.clone(),
            bottom_right: value.clone(),
            bottom_left: value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualStyle {
    #[serde(default)]
    pub fills: Vec<Fill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    pub radii: RadiusValues<DesignValue<f64>>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            fills: Vec::new(),
            stroke: None,
            radii: RadiusValues::uniform(DesignValue::literal(0.0)),
            effects: Vec::new(),
            opacity: 1.0,
        }
    }
}

const fn default_opacity() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub paint: Paint,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Paint {
    Solid {
        color: DesignValue<ColorValue>,
    },
    LinearGradient {
        angle_degrees: f64,
        stops: Vec<GradientStop>,
    },
    Image {
        asset_id: String,
        fit: ImageFit,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f64,
    pub color: DesignValue<ColorValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImageFit {
    Fill,
    Fit,
    Crop,
    Tile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub widths: EdgeValues<DesignValue<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Effect {
    Shadow {
        color: DesignValue<ColorValue>,
        x: DesignValue<f64>,
        y: DesignValue<f64>,
        blur: DesignValue<f64>,
        spread: DesignValue<f64>,
        inset: bool,
    },
    Blur {
        radius: DesignValue<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorValue {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<DesignValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<DesignValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<DesignValue<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<DesignValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<DesignValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<DesignValue<ColorValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub component_id: String,
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default)]
    pub properties: BTreeMap<String, InstanceValue>,
    #[serde(default)]
    pub slots: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstanceValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Token(String),
    Asset(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignToken {
    pub id: String,
    pub name: String,
    pub scope: TokenScope,
    pub kind: TokenKind,
    pub default_mode: String,
    #[serde(default)]
    pub modes: BTreeMap<String, TokenValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenScope {
    Primitive,
    Semantic,
    Component,
    Module,
    Shell,
    Platform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenKind {
    Color,
    Number,
    Text,
    Boolean,
    Font,
    Motion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenValue {
    Color(ColorValue),
    Number(f64),
    Text(String),
    Boolean(bool),
    Font(FontValue),
    Motion(MotionValue),
    Alias(String),
}

impl TokenValue {
    #[must_use]
    pub const fn kind(&self) -> Option<TokenKind> {
        match self {
            Self::Color(_) => Some(TokenKind::Color),
            Self::Number(_) => Some(TokenKind::Number),
            Self::Text(_) => Some(TokenKind::Text),
            Self::Boolean(_) => Some(TokenKind::Boolean),
            Self::Font(_) => Some(TokenKind::Font),
            Self::Motion(_) => Some(TokenKind::Motion),
            Self::Alias(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontValue {
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    pub weight: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionValue {
    pub duration_ms: u32,
    pub easing: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentContract {
    pub id: String,
    pub name: String,
    pub role: ComponentRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub variants: BTreeMap<String, VariantAxis>,
    #[serde(default)]
    pub slots: BTreeMap<String, SlotContract>,
    #[serde(default)]
    pub events: BTreeMap<String, EventContract>,
    #[serde(default)]
    pub states: BTreeSet<String>,
    #[serde(default)]
    pub token_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_binding_id: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentRole {
    Primitive,
    Control,
    Composite,
    Module,
    Shell,
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantAxis {
    pub values: BTreeSet<String>,
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotContract {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub accepted_component_roles: BTreeSet<ComponentRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventContract {
    pub payload: EventPayloadKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventPayloadKind {
    None,
    Boolean,
    Number,
    Text,
    Selection,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBinding {
    pub id: String,
    pub target: BindingTarget,
    pub symbol: RustSymbol,
    pub ownership: CodeOwnership,
    pub sync_policy: SyncPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingTarget {
    pub kind: BindingTargetKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingTargetKind {
    Document,
    Node,
    Token,
    Component,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustSymbol {
    pub crate_name: String,
    pub module_path: String,
    pub symbol_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodeOwnership {
    GeneratedPresentation,
    HandwrittenPresentation,
    HandwrittenBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncPolicy {
    DesignToCode,
    CodeToDesign,
    Bidirectional,
    ReferenceOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesignTarget {
    Document,
    Node { node_id: String },
    Token { token_id: String },
    Component { component_id: String },
}
