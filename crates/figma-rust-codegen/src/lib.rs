//! Deterministic, AST-based Rust GPUI code generation.

use std::collections::BTreeMap;
use std::fmt;

use figma_rust_core::ir::{
    AssetRoute, Axis, AxisSize, AxisSizing, BoundValue, Color, ComponentResolution, DesignDocument,
    Edges, Effect, Layout, Node, Paint, Positioning, TextStyle, TokenRef,
};
use figma_rust_core::raw::{
    RawAlignment, RawAsset, RawBlendMode, RawConstraint, RawNodeKind, RawStrokeAlign,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Exact upstream GPUI revision expected by generated compile fixtures.
pub const GPUI_REVISION: &str = "5631830c564afa89b3aba679f45d9c3345f9460f";

/// Generated Rust and its stable Figma-node source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedOutput {
    pub rust: String,
    pub source_map: SourceMap,
}

/// Maps each emitted Figma node ID to its generated function and line range.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub nodes: BTreeMap<String, SourceMapEntry>,
}

/// One generated node function's location in [`GeneratedOutput::rust`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMapEntry {
    pub symbol: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Source location attached to a code generation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorSource {
    pub node_id: String,
    pub property: String,
}

impl fmt::Display for ErrorSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "node={} property={}",
            self.node_id, self.property
        )
    }
}

/// Strict failures produced before any generated file is returned.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CodegenError {
    #[error("unsupported GPUI lowering at {location}: {feature}")]
    Unsupported {
        location: ErrorSource,
        feature: String,
    },
    #[error("invalid IR value at {location}: {value}")]
    InvalidValue {
        location: ErrorSource,
        value: String,
    },
    #[error("duplicate Figma source node ID `{node_id}`")]
    DuplicateSourceId { node_id: String },
    #[error("generated Rust failed syn validation: {message}")]
    InvalidRust { message: String },
    #[error("generated symbol `{symbol}` was absent from pretty-printed Rust")]
    MissingSourceSpan { symbol: String },
}

#[derive(Clone, Copy)]
struct IndexedNode<'a> {
    ordinal: usize,
    node: &'a Node,
    emitted: bool,
    captured_by_asset: bool,
    parent_stack_axis: Option<Axis>,
}

/// Lowers a normalized design document into deterministic GPUI Rust.
///
/// The returned Rust accepts the runtime's `TokenResolver`; bound colors use
/// the token ID while retaining the normalized literal as their fallback.
///
/// # Errors
///
/// Returns [`CodegenError`] when the document contains an unsupported or invalid
/// construct, generated syntax does not parse, or source spans cannot be found.
pub fn generate(document: &DesignDocument) -> Result<GeneratedOutput, CodegenError> {
    let (indexed, ordinals) = index_document(document)?;
    for indexed_node in &indexed {
        validate_node(
            indexed_node.node,
            &document.assets,
            indexed_node.captured_by_asset,
        )?;
    }
    let emitted = indexed
        .iter()
        .copied()
        .filter(|indexed_node| indexed_node.emitted)
        .collect::<Vec<_>>();

    let uses_assets = emitted
        .iter()
        .any(|indexed_node| is_asset_fallback(indexed_node.node));
    let functions = emitted
        .iter()
        .map(|indexed_node| lower_function(*indexed_node, &ordinals, &document.assets, uses_assets))
        .collect::<Result<Vec<_>, _>>()?;
    let root_symbols = document
        .roots
        .iter()
        .filter(|node| node.visible)
        .map(|node| {
            let ordinal = ordinals[node.source_id.as_str()];
            node_symbol(ordinal)
        })
        .collect::<Vec<_>>();
    let root_elements = root_symbols
        .iter()
        .map(|symbol| {
            if uses_assets {
                quote! { #symbol(tokens, assets) }
            } else {
                quote! { #symbol(tokens) }
            }
        })
        .collect::<Vec<_>>();
    let uses_parent_element = root_symbols.len() > 1
        || emitted.iter().any(|indexed| {
            !is_asset_fallback(indexed.node)
                && (indexed.node.text.is_some()
                    || indexed.node.children.iter().any(|child| child.visible)
                    || has_visible_stroke(indexed.node))
        });
    let imports = if emitted.is_empty() {
        quote! {}
    } else if uses_parent_element {
        quote! {
            use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
        }
    } else {
        quote! {
            use gpui::{InteractiveElement as _, Styled as _};
        }
    };
    let root = match root_elements.as_slice() {
        [] => quote! { gpui::div() },
        [element] => quote! { #element },
        elements => quote! { gpui::div() #(.child(#elements))* },
    };
    let generated_view = if uses_assets {
        quote! {
            pub fn generated_view<Tokens, Assets>(
                tokens: &Tokens,
                assets: &Assets,
            ) -> impl gpui::IntoElement + use<Tokens, Assets>
            where
                Tokens: figma_gpui_runtime::TokenResolver,
                Assets: figma_gpui_runtime::AssetResolver,
            {
                let _ = tokens;
                #root
            }
        }
    } else {
        quote! {
            pub fn generated_view<Tokens>(
                tokens: &Tokens,
            ) -> impl gpui::IntoElement + use<Tokens>
            where
                Tokens: figma_gpui_runtime::TokenResolver,
            {
                let _ = tokens;
                #root
            }
        }
    };
    let tokens = quote! {
        #imports

        #generated_view

        #(#functions)*
    };
    let syntax = syn::parse2::<syn::File>(tokens).map_err(|error| CodegenError::InvalidRust {
        message: error.to_string(),
    })?;
    let rust = prettyplease::unparse(&syntax);
    let source_map = build_source_map(&rust, &emitted)?;

    Ok(GeneratedOutput { rust, source_map })
}

fn index_document(
    document: &DesignDocument,
) -> Result<(Vec<IndexedNode<'_>>, BTreeMap<&str, usize>), CodegenError> {
    fn visit<'a>(
        node: &'a Node,
        ancestors_visible: bool,
        captured_by_asset: bool,
        parent_stack_axis: Option<Axis>,
        indexed: &mut Vec<IndexedNode<'a>>,
        ordinals: &mut BTreeMap<&'a str, usize>,
    ) -> Result<(), CodegenError> {
        let ordinal = indexed.len();
        if ordinals.insert(node.source_id.as_str(), ordinal).is_some() {
            return Err(CodegenError::DuplicateSourceId {
                node_id: node.source_id.clone(),
            });
        }
        let emitted = ancestors_visible && node.visible && !captured_by_asset;
        indexed.push(IndexedNode {
            ordinal,
            node,
            emitted,
            captured_by_asset,
            parent_stack_axis,
        });
        let child_parent_axis = match &node.layout {
            Layout::Stack { axis, .. } => Some(*axis),
            Layout::Plain { .. } | Layout::Absolute { .. } | Layout::Grid { .. } => None,
        };
        let children_captured = captured_by_asset || (emitted && is_asset_fallback(node));
        for child in &node.children {
            visit(
                child,
                emitted,
                children_captured,
                child_parent_axis,
                indexed,
                ordinals,
            )?;
        }
        Ok(())
    }

    let mut indexed = Vec::new();
    let mut ordinals = BTreeMap::new();
    for root in &document.roots {
        visit(root, true, false, None, &mut indexed, &mut ordinals)?;
    }
    Ok((indexed, ordinals))
}

fn lower_function(
    indexed: IndexedNode<'_>,
    ordinals: &BTreeMap<&str, usize>,
    assets: &[RawAsset],
    uses_assets: bool,
) -> Result<TokenStream, CodegenError> {
    let symbol = node_symbol(indexed.ordinal);
    let element = lower_node(indexed, ordinals, assets, uses_assets)?;
    if uses_assets {
        Ok(quote! {
            #[allow(clippy::too_many_lines)]
            fn #symbol<Tokens, Assets>(
                tokens: &Tokens,
                assets: &Assets,
            ) -> impl gpui::IntoElement + use<Tokens, Assets>
            where
                Tokens: figma_gpui_runtime::TokenResolver,
                Assets: figma_gpui_runtime::AssetResolver,
            {
                let _ = tokens;
                let _ = assets;
                #element
            }
        })
    } else {
        Ok(quote! {
            #[allow(clippy::too_many_lines)]
            fn #symbol<Tokens>(
                tokens: &Tokens,
            ) -> impl gpui::IntoElement + use<Tokens>
            where
                Tokens: figma_gpui_runtime::TokenResolver,
            {
                let _ = tokens;
                #element
            }
        })
    }
}

fn lower_node(
    indexed: IndexedNode<'_>,
    ordinals: &BTreeMap<&str, usize>,
    assets: &[RawAsset],
    uses_assets: bool,
) -> Result<TokenStream, CodegenError> {
    let node = indexed.node;
    let ordinal = indexed.ordinal;
    if let Some(asset) = fallback_asset(node, assets)? {
        let file_name = asset.file_name().ok_or_else(|| {
            unsupported_error(
                node,
                "asset_decision.route",
                format!("unsupported fallback media type `{}`", asset.media_type),
            )
        })?;
        let preserve_authored_colors = match asset
            .export_settings
            .get("color_policy")
            .map(String::as_str)
        {
            None | Some("monochrome") => false,
            Some("authored") => true,
            Some(policy) => {
                return Err(unsupported_error(
                    node,
                    "asset_decision.route",
                    format!("unsupported SVG color policy `{policy}`"),
                ));
            }
        };
        let mut element = match node.asset_decision.route {
            AssetRoute::Svg if preserve_authored_colors => {
                quote! {
                    gpui::img(figma_gpui_runtime::AssetResolver::asset_path(assets, #file_name))
                        .debug_selector(|| figma_gpui_runtime::source_selector(#ordinal))
                }
            }
            AssetRoute::Svg => quote! {
                gpui::svg()
                    .external_path(
                        figma_gpui_runtime::AssetResolver::asset_path(assets, #file_name)
                            .to_string_lossy()
                            .into_owned()
                    )
                    .debug_selector(|| figma_gpui_runtime::source_selector(#ordinal))
            },
            AssetRoute::Raster => quote! {
                gpui::img(figma_gpui_runtime::AssetResolver::asset_path(assets, #file_name))
                    .debug_selector(|| figma_gpui_runtime::source_selector(#ordinal))
            },
            AssetRoute::Native | AssetRoute::Runtime => unreachable!("validated asset route"),
        };
        element = lower_position(element, node)?;
        element = lower_size(element, node, indexed.parent_stack_axis)?;
        return Ok(element);
    }
    let mut element = quote! {
        gpui::div().debug_selector(|| figma_gpui_runtime::source_selector(#ordinal))
    };

    element = lower_position(element, node)?;
    element = lower_layout(element, node)?;
    element = lower_size(element, node, indexed.parent_stack_axis)?;
    element = lower_fill(element, node)?;
    element = lower_radii(element, node)?;
    element = lower_opacity(element, node)?;
    element = lower_shadows(element, node)?;
    element = lower_text_style(element, node)?;

    if let Some(text) = &node.text {
        if matches!(node.size.horizontal.sizing, AxisSizing::Hug) {
            element = quote! { #element.whitespace_nowrap() };
        }
        let characters = &text.characters;
        element = quote! { #element.child(#characters) };
    }
    for child in node.children.iter().filter(|child| child.visible) {
        let child_symbol = node_symbol(ordinals[child.source_id.as_str()]);
        element = if uses_assets {
            quote! { #element.child(#child_symbol(tokens, assets)) }
        } else {
            quote! { #element.child(#child_symbol(tokens)) }
        };
    }
    if let Some(stroke) = lower_stroke_overlay(node)? {
        element = quote! { #element.child(#stroke) };
    }
    Ok(element)
}

fn is_asset_fallback(node: &Node) -> bool {
    matches!(
        node.asset_decision.route,
        AssetRoute::Svg | AssetRoute::Raster
    )
}

fn fallback_asset<'a>(
    node: &Node,
    assets: &'a [RawAsset],
) -> Result<Option<&'a RawAsset>, CodegenError> {
    let media_matches = |asset: &&RawAsset| match node.asset_decision.route {
        AssetRoute::Svg => {
            asset.media_type == "image/svg+xml"
                && asset.export_settings.get("format").map(String::as_str) == Some("SVG")
        }
        AssetRoute::Raster => {
            asset.media_type == "image/png"
                && asset.export_settings.get("format").map(String::as_str) == Some("PNG")
        }
        AssetRoute::Native | AssetRoute::Runtime => false,
    };
    if !is_asset_fallback(node) {
        return Ok(None);
    }

    let mut matches = assets
        .iter()
        .filter(|asset| asset.source_node_id == node.source_id)
        .filter(media_matches);
    let Some(asset) = matches.next() else {
        return Err(unsupported_error(
            node,
            "asset_decision.route",
            format!(
                "{:?} fallback has no matching asset",
                node.asset_decision.route
            ),
        ));
    };
    if matches.next().is_some() {
        return Err(unsupported_error(
            node,
            "asset_decision.route",
            format!(
                "{:?} fallback has multiple matching assets",
                node.asset_decision.route
            ),
        ));
    }
    if asset.payload_base64.as_deref().is_none_or(str::is_empty) {
        return Err(unsupported_error(
            node,
            "asset_decision.route",
            format!("fallback asset `{}` has no payload", asset.id),
        ));
    }
    Ok(Some(asset))
}

fn validate_node(
    node: &Node,
    assets: &[RawAsset],
    captured_by_asset: bool,
) -> Result<(), CodegenError> {
    if let Some(component) = &node.component
        && let ComponentResolution::Mapped { mapping_id } = &component.resolution
    {
        return unsupported(
            node,
            "component.resolution",
            format!("mapped component invocation `{mapping_id}`"),
        );
    }
    if !node.reactions.is_empty() {
        return unsupported(node, "reactions", "prototype reactions");
    }
    if captured_by_asset {
        return Ok(());
    }
    if fallback_asset(node, assets)?.is_some() {
        return Ok(());
    }
    match node.kind {
        RawNodeKind::Vector | RawNodeKind::Image => {
            return unsupported(node, "kind", format!("{:?} asset node", node.kind));
        }
        RawNodeKind::Frame
        | RawNodeKind::Group
        | RawNodeKind::Rectangle
        | RawNodeKind::Ellipse
        | RawNodeKind::Text
        | RawNodeKind::Component
        | RawNodeKind::Instance
        | RawNodeKind::Scroll => {}
    }

    if node.asset_decision.route != AssetRoute::Native {
        return unsupported(
            node,
            "asset_decision.route",
            format!("{:?} asset route", node.asset_decision.route),
        );
    }
    if node.style.is_mask {
        return unsupported(node, "style.is_mask", "mask rendering");
    }
    if !matches!(
        node.style.blend_mode,
        RawBlendMode::Normal | RawBlendMode::PassThrough
    ) {
        return unsupported(
            node,
            "style.blend_mode",
            format!("{:?} blend mode", node.style.blend_mode),
        );
    }
    if matches!(node.layout, Layout::Grid { .. }) {
        return unsupported(node, "layout", "grid layout");
    }
    if node.style.fills.len() > 1 {
        return unsupported(node, "style.fills", "multiple fills");
    }
    if node.style.strokes.len() > 1 {
        return unsupported(node, "style.strokes", "multiple strokes");
    }
    validate_transform(node)?;
    validate_paints(node, "style.fills", &node.style.fills)?;
    validate_paints(node, "style.strokes", &node.style.strokes)?;
    validate_effects(node)?;
    validate_text(node)?;
    Ok(())
}

fn validate_effects(node: &Node) -> Result<(), CodegenError> {
    for effect in &node.style.effects {
        match effect {
            Effect::Shadow {
                inset: false,
                color,
                offset_x,
                offset_y,
                blur,
                spread,
            } => {
                let _ = packed_rgba(node, "style.effects.color", color.fallback)?;
                checked_f32(node, "style.effects.offset_x", *offset_x, false)?;
                checked_f32(node, "style.effects.offset_y", *offset_y, false)?;
                checked_f32(node, "style.effects.blur", *blur, true)?;
                checked_f32(node, "style.effects.spread", *spread, false)?;
            }
            Effect::Shadow { inset: true, .. } => {
                return unsupported(node, "style.effects", "inner shadow");
            }
            Effect::Unsupported { source_kind } => {
                return unsupported(
                    node,
                    "style.effects",
                    format!("unsupported effect `{source_kind}`"),
                );
            }
        }
    }
    Ok(())
}

fn validate_transform(node: &Node) -> Result<(), CodegenError> {
    let transform = match node.positioning {
        Positioning::Auto { transform, .. } | Positioning::Absolute { transform, .. } => transform,
    };
    let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    if !transform
        .matrix
        .iter()
        .zip(identity)
        .all(|(actual, expected)| same_f64(*actual, expected))
    {
        return unsupported(node, "positioning.transform", "non-identity transform");
    }
    Ok(())
}

fn validate_paints(node: &Node, property: &str, paints: &[Paint]) -> Result<(), CodegenError> {
    for paint in paints {
        match paint {
            Paint::Solid { color } => {
                let _ = packed_rgba(node, property, color.fallback)?;
            }
            Paint::Image { asset_id, .. } => {
                return unsupported(node, property, format!("image asset paint `{asset_id}`"));
            }
            Paint::Gradient { .. } => {
                return unsupported(node, property, "gradient paint");
            }
            Paint::Unsupported { source_kind } => {
                return unsupported(node, property, format!("unsupported paint `{source_kind}`"));
            }
        }
    }
    Ok(())
}

fn validate_text(node: &Node) -> Result<(), CodegenError> {
    let Some(text) = &node.text else {
        return Ok(());
    };
    if node.kind != RawNodeKind::Text {
        return unsupported(node, "text", "text payload on a non-text node");
    }
    if text.runs.len() > 1 {
        return unsupported(node, "text.runs", "mixed rich-text runs");
    }
    if let Some(run) = text.runs.first() {
        let full_length = u32::try_from(text.characters.encode_utf16().count())
            .map_err(|_| invalid(node, "text.characters", "UTF-16 length exceeds u32"))?;
        if run.start_utf16 != 0 || run.end_utf16 != full_length || run.text != text.characters {
            return unsupported(node, "text.runs", "partial rich-text run");
        }
        validate_text_style(node, &run.style)?;
    }
    Ok(())
}

fn validate_text_style(node: &Node, style: &TextStyle) -> Result<(), CodegenError> {
    if let Some(font_style) = &style.font_style
        && font_style_posture(font_style, style.font_weight).is_none()
    {
        return unsupported(
            node,
            "text.runs.style.font_style",
            format!("font style `{font_style}`"),
        );
    }
    if style.letter_spacing.is_some_and(|spacing| spacing != 0.0) {
        return unsupported(node, "text.runs.style.letter_spacing", "letter spacing");
    }
    if let Some(size) = &style.font_size {
        checked_f32(node, "text.runs.style.font_size", size.fallback, true)?;
    }
    if let Some(line_height) = style.line_height {
        checked_f32(node, "text.runs.style.line_height", line_height, true)?;
    }
    if let Some(color) = &style.color {
        let _ = packed_rgba(node, "text.runs.style.color", color.fallback)?;
    }
    Ok(())
}

fn lower_position(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    match node.positioning {
        Positioning::Auto { grid: Some(_), .. } => {
            return unsupported(node, "positioning.grid", "grid placement");
        }
        Positioning::Auto { grid: None, .. } => {
            element = quote! { #element.relative() };
        }
        Positioning::Absolute {
            x,
            y,
            horizontal_constraint,
            vertical_constraint,
            ..
        } => {
            if horizontal_constraint != RawConstraint::Min
                || vertical_constraint != RawConstraint::Min
            {
                return unsupported(
                    node,
                    "positioning.constraints",
                    "absolute constraints other than MIN/MIN",
                );
            }
            let x = checked_f32(node, "positioning.x", x, false)?;
            let y = checked_f32(node, "positioning.y", y, false)?;
            element = quote! { #element.absolute().left(gpui::px(#x)).top(gpui::px(#y)) };
        }
    }
    Ok(element)
}

fn lower_layout(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let (clips_content, scroll) = match &node.layout {
        Layout::Plain {
            clips_content,
            scroll,
        }
        | Layout::Absolute {
            clips_content,
            scroll,
        } => (*clips_content, *scroll),
        Layout::Stack {
            axis,
            wrap,
            primary_alignment,
            counter_alignment,
            gap,
            padding,
            clips_content,
            scroll,
        } => {
            element = quote! { #element.flex() };
            element = match axis {
                Axis::Horizontal => quote! { #element.flex_row() },
                Axis::Vertical => quote! { #element.flex_col() },
            };
            if *wrap {
                element = quote! { #element.flex_wrap() };
            }
            element = lower_primary_alignment(&element, node, *primary_alignment)?;
            element = lower_counter_alignment(&element, node, *counter_alignment)?;
            if bound_number_is_visible(gap) {
                let gap = number_tokens(node, "layout.gap", gap)?;
                element = quote! { #element.gap(gpui::px(#gap)) };
            }
            element = lower_padding(element, node, padding)?;
            (*clips_content, *scroll)
        }
        Layout::Grid { .. } => return unsupported(node, "layout", "grid layout"),
    };

    if scroll.horizontal || scroll.vertical {
        return unsupported(node, "layout.scroll", "scrolling");
    }
    if clips_content {
        element = quote! { #element.overflow_hidden() };
    }
    Ok(element)
}

fn lower_primary_alignment(
    element: &TokenStream,
    node: &Node,
    alignment: RawAlignment,
) -> Result<TokenStream, CodegenError> {
    match alignment {
        RawAlignment::Start => Ok(quote! { #element.justify_start() }),
        RawAlignment::Center => Ok(quote! { #element.justify_center() }),
        RawAlignment::End => Ok(quote! { #element.justify_end() }),
        RawAlignment::SpaceBetween => Ok(quote! { #element.justify_between() }),
        RawAlignment::Baseline | RawAlignment::Stretch => unsupported(
            node,
            "layout.primary_alignment",
            format!("{alignment:?} primary alignment"),
        ),
    }
}

fn lower_counter_alignment(
    element: &TokenStream,
    node: &Node,
    alignment: RawAlignment,
) -> Result<TokenStream, CodegenError> {
    match alignment {
        RawAlignment::Start => Ok(quote! { #element.items_start() }),
        RawAlignment::Center => Ok(quote! { #element.items_center() }),
        RawAlignment::End => Ok(quote! { #element.items_end() }),
        RawAlignment::Baseline => Ok(quote! { #element.items_baseline() }),
        RawAlignment::Stretch => Ok(quote! { #element.items_stretch() }),
        RawAlignment::SpaceBetween => unsupported(
            node,
            "layout.counter_alignment",
            "SPACE_BETWEEN counter alignment",
        ),
    }
}

fn lower_padding(
    mut element: TokenStream,
    node: &Node,
    padding: &Edges,
) -> Result<TokenStream, CodegenError> {
    if padding.top == padding.right
        && padding.right == padding.bottom
        && padding.bottom == padding.left
    {
        if bound_number_is_visible(&padding.top) {
            let top = number_tokens(node, "layout.padding", &padding.top)?;
            element = quote! { #element.p(gpui::px(#top)) };
        }
        return Ok(element);
    }
    if bound_number_is_visible(&padding.top) {
        let top = number_tokens(node, "layout.padding.top", &padding.top)?;
        element = quote! { #element.pt(gpui::px(#top)) };
    }
    if bound_number_is_visible(&padding.right) {
        let right = number_tokens(node, "layout.padding.right", &padding.right)?;
        element = quote! { #element.pr(gpui::px(#right)) };
    }
    if bound_number_is_visible(&padding.bottom) {
        let bottom = number_tokens(node, "layout.padding.bottom", &padding.bottom)?;
        element = quote! { #element.pb(gpui::px(#bottom)) };
    }
    if bound_number_is_visible(&padding.left) {
        let left = number_tokens(node, "layout.padding.left", &padding.left)?;
        element = quote! { #element.pl(gpui::px(#left)) };
    }
    Ok(element)
}

fn lower_size(
    mut element: TokenStream,
    node: &Node,
    parent_stack_axis: Option<Axis>,
) -> Result<TokenStream, CodegenError> {
    element = lower_axis_size(element, node, &node.size.horizontal, true)?;
    element = lower_axis_size(element, node, &node.size.vertical, false)?;
    let fills_main_axis = matches!(
        (parent_stack_axis, node.size.horizontal.sizing),
        (Some(Axis::Horizontal), AxisSizing::Fill)
    ) || matches!(
        (parent_stack_axis, node.size.vertical.sizing),
        (Some(Axis::Vertical), AxisSizing::Fill)
    );
    if fills_main_axis {
        element = quote! { #element.flex_1() };
    } else {
        element = quote! { #element.flex_none() };
    }
    if let Some(ratio) = node.size.aspect_ratio {
        let ratio = checked_f32(node, "size.aspect_ratio", ratio, true)?;
        if ratio == 0.0 {
            return Err(invalid(node, "size.aspect_ratio", "zero"));
        }
        element = quote! { #element.aspect_ratio(#ratio) };
    }
    Ok(element)
}

fn lower_axis_size(
    mut element: TokenStream,
    node: &Node,
    axis: &AxisSize,
    horizontal: bool,
) -> Result<TokenStream, CodegenError> {
    match axis.sizing {
        AxisSizing::Hug => {
            if let Some(value) = axis.measured {
                let property = if horizontal {
                    "size.horizontal.measured"
                } else {
                    "size.vertical.measured"
                };
                let value = checked_f32(node, property, value, true)?;
                element = if horizontal {
                    quote! { #element.w(gpui::px(#value)) }
                } else {
                    quote! { #element.h(gpui::px(#value)) }
                };
            }
        }
        AxisSizing::Fill if horizontal => element = quote! { #element.w_full() },
        AxisSizing::Fill => element = quote! { #element.h_full() },
        AxisSizing::Fixed(value) => {
            let property = if horizontal {
                "size.horizontal"
            } else {
                "size.vertical"
            };
            let value = checked_f32(node, property, value, true)?;
            element = if horizontal {
                quote! { #element.w(gpui::px(#value)) }
            } else {
                quote! { #element.h(gpui::px(#value)) }
            };
        }
    }
    if let Some(minimum) = axis.min {
        let property = if horizontal {
            "size.horizontal.min"
        } else {
            "size.vertical.min"
        };
        let minimum = checked_f32(node, property, minimum, true)?;
        element = if horizontal {
            quote! { #element.min_w(gpui::px(#minimum)) }
        } else {
            quote! { #element.min_h(gpui::px(#minimum)) }
        };
    }
    if let Some(maximum) = axis.max {
        let property = if horizontal {
            "size.horizontal.max"
        } else {
            "size.vertical.max"
        };
        let maximum = checked_f32(node, property, maximum, true)?;
        element = if horizontal {
            quote! { #element.max_w(gpui::px(#maximum)) }
        } else {
            quote! { #element.max_h(gpui::px(#maximum)) }
        };
    }
    Ok(element)
}

fn lower_fill(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let Some(Paint::Solid { color }) = node.style.fills.first() else {
        return Ok(element);
    };
    let color = color_tokens(node, "style.fills", color)?;
    element = if node.kind == RawNodeKind::Text {
        quote! { #element.text_color(#color) }
    } else {
        quote! { #element.bg(#color) }
    };
    Ok(element)
}

fn lower_stroke(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let Some(Paint::Solid { color }) = node.style.strokes.first() else {
        return Ok(element);
    };
    if node.style.strokes.len() != 1 {
        return unsupported(node, "style.strokes", "multiple strokes");
    }
    if node.style.stroke_align != RawStrokeAlign::Inside {
        return unsupported(
            node,
            "style.stroke_align",
            format!("{:?} stroke alignment", node.style.stroke_align),
        );
    }
    let widths = &node.style.stroke_widths;
    let has_width = [&widths.top, &widths.right, &widths.bottom, &widths.left]
        .into_iter()
        .any(bound_number_is_visible);
    if has_width {
        let color = color_tokens(node, "style.strokes", color)?;
        if widths.top == widths.right
            && widths.right == widths.bottom
            && widths.bottom == widths.left
        {
            let width = number_tokens(node, "style.stroke_widths", &widths.top)?;
            element = quote! { #element.border(gpui::px(#width)).border_color(#color) };
        } else {
            if bound_number_is_visible(&widths.top) {
                let width = number_tokens(node, "style.stroke_widths.top", &widths.top)?;
                element = quote! { #element.border_t(gpui::px(#width)) };
            }
            if bound_number_is_visible(&widths.right) {
                let width = number_tokens(node, "style.stroke_widths.right", &widths.right)?;
                element = quote! { #element.border_r(gpui::px(#width)) };
            }
            if bound_number_is_visible(&widths.bottom) {
                let width = number_tokens(node, "style.stroke_widths.bottom", &widths.bottom)?;
                element = quote! { #element.border_b(gpui::px(#width)) };
            }
            if bound_number_is_visible(&widths.left) {
                let width = number_tokens(node, "style.stroke_widths.left", &widths.left)?;
                element = quote! { #element.border_l(gpui::px(#width)) };
            }
            element = quote! { #element.border_color(#color) };
        }
    }
    Ok(element)
}

fn lower_stroke_overlay(node: &Node) -> Result<Option<TokenStream>, CodegenError> {
    if !has_visible_stroke(node) {
        return Ok(None);
    }

    let overlay = quote! { gpui::div().absolute().inset_0() };
    let overlay = lower_stroke(overlay, node)?;
    let overlay = lower_radii(overlay, node)?;
    Ok(Some(overlay))
}

fn has_visible_stroke(node: &Node) -> bool {
    !node.style.strokes.is_empty()
        && [
            &node.style.stroke_widths.top,
            &node.style.stroke_widths.right,
            &node.style.stroke_widths.bottom,
            &node.style.stroke_widths.left,
        ]
        .into_iter()
        .any(bound_number_is_visible)
}

fn lower_radii(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    if node.style.radii.smoothing != 0.0 {
        return unsupported(node, "style.radii.smoothing", "corner smoothing");
    }
    let radii = &node.style.radii;
    if node.kind == RawNodeKind::Ellipse {
        element = quote! { #element.rounded(gpui::px(9999.0_f32)) };
    } else if radii.top_left == radii.top_right
        && radii.top_right == radii.bottom_right
        && radii.bottom_right == radii.bottom_left
    {
        if bound_number_is_visible(&radii.top_left) {
            let radius = number_tokens(node, "style.radii", &radii.top_left)?;
            element = quote! { #element.rounded(gpui::px(#radius)) };
        }
    } else {
        if bound_number_is_visible(&radii.top_left) {
            let radius = number_tokens(node, "style.radii.top_left", &radii.top_left)?;
            element = quote! { #element.rounded_tl(gpui::px(#radius)) };
        }
        if bound_number_is_visible(&radii.top_right) {
            let radius = number_tokens(node, "style.radii.top_right", &radii.top_right)?;
            element = quote! { #element.rounded_tr(gpui::px(#radius)) };
        }
        if bound_number_is_visible(&radii.bottom_right) {
            let radius = number_tokens(node, "style.radii.bottom_right", &radii.bottom_right)?;
            element = quote! { #element.rounded_br(gpui::px(#radius)) };
        }
        if bound_number_is_visible(&radii.bottom_left) {
            let radius = number_tokens(node, "style.radii.bottom_left", &radii.bottom_left)?;
            element = quote! { #element.rounded_bl(gpui::px(#radius)) };
        }
    }
    Ok(element)
}

fn lower_opacity(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let opacity = checked_f32(node, "opacity", node.opacity, true)?;
    if opacity > 1.0 {
        return Err(invalid(node, "opacity", node.opacity.to_string()));
    }
    if !same_f32(opacity, 1.0) {
        element = quote! { #element.opacity(#opacity) };
    }
    Ok(element)
}

fn lower_shadows(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let shadows = node
        .style
        .effects
        .iter()
        .map(|effect| match effect {
            Effect::Shadow {
                inset: false,
                color,
                offset_x,
                offset_y,
                blur,
                spread,
            } => {
                let offset_x = checked_f32(node, "style.effects.offset_x", *offset_x, false)?;
                let offset_y = checked_f32(node, "style.effects.offset_y", *offset_y, false)?;
                let blur = checked_f32(node, "style.effects.blur", *blur, true)?;
                let spread = checked_f32(node, "style.effects.spread", *spread, false)?;
                let color = color_tokens(node, "style.effects.color", color)?;
                Ok(quote! {
                    gpui::BoxShadow::new(
                        gpui::px(#offset_x),
                        gpui::px(#offset_y),
                        (#color).into(),
                    )
                    .blur_radius(gpui::px(#blur))
                    .spread_radius(gpui::px(#spread))
                })
            }
            Effect::Shadow { inset: true, .. } => {
                unsupported(node, "style.effects", "inner shadow")
            }
            Effect::Unsupported { source_kind } => unsupported(
                node,
                "style.effects",
                format!("unsupported effect `{source_kind}`"),
            ),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !shadows.is_empty() {
        element = quote! { #element.shadow(vec![#(#shadows),*]) };
    }
    Ok(element)
}

fn lower_text_style(mut element: TokenStream, node: &Node) -> Result<TokenStream, CodegenError> {
    let Some(style) = node
        .text
        .as_ref()
        .and_then(|text| text.runs.first())
        .map(|run| &run.style)
    else {
        return Ok(element);
    };
    if let Some(family) = &style.font_family {
        element = quote! { #element.font_family(#family) };
    }
    if style
        .font_style
        .as_ref()
        .is_some_and(|font_style| font_style_posture(font_style, style.font_weight) == Some(true))
    {
        element = quote! { #element.italic() };
    }
    if let Some(size) = &style.font_size {
        let size = number_tokens(node, "text.runs.style.font_size", size)?;
        element = quote! { #element.text_size(gpui::px(#size)) };
    }
    if let Some(weight) = style.font_weight {
        let weight = f32::from(weight);
        element = quote! { #element.font_weight(gpui::FontWeight(#weight)) };
    }
    if let Some(line_height) = style.line_height {
        let line_height = checked_f32(node, "text.runs.style.line_height", line_height, true)?;
        element = quote! { #element.line_height(gpui::px(#line_height)) };
    }
    if let Some(color) = &style.color {
        let color = color_tokens(node, "text.runs.style.color", color)?;
        element = quote! { #element.text_color(#color) };
    }
    Ok(element)
}

fn font_style_posture(font_style: &str, font_weight: Option<u16>) -> Option<bool> {
    let compact = font_style
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let (weight_name, italic) = compact
        .strip_suffix("italic")
        .map_or((compact.as_str(), false), |weight_name| (weight_name, true));
    let posture_only = matches!(weight_name, "" | "normal" | "regular" | "roman");
    let named_weight = matches!(
        weight_name,
        "hairline"
            | "thin"
            | "extralight"
            | "ultralight"
            | "light"
            | "book"
            | "medium"
            | "semibold"
            | "demibold"
            | "bold"
            | "extrabold"
            | "ultrabold"
            | "black"
            | "heavy"
    );
    (posture_only || (named_weight && font_weight.is_some())).then_some(italic)
}

fn color_tokens(
    node: &Node,
    property: &str,
    color: &BoundValue<Color>,
) -> Result<TokenStream, CodegenError> {
    let packed = packed_rgba(node, property, color.fallback)?;
    let literal = syn::LitInt::new(
        &format!("0x{:04x}_{:04x}", packed >> 16, packed & 0xffff),
        Span::call_site(),
    );
    let fallback = quote! { gpui::rgba(#literal) };
    Ok(if let Some(token) = &color.token {
        let context = token_context_tokens(token, &color.mode_context);
        quote! {
            figma_gpui_runtime::TokenResolver::color_with_context(
                tokens,
                #context,
                gpui::Hsla::from(#fallback),
            )
        }
    } else {
        fallback
    })
}

fn number_tokens(
    node: &Node,
    property: &str,
    number: &BoundValue<f64>,
) -> Result<TokenStream, CodegenError> {
    let fallback = checked_f32(node, property, number.fallback, true)?;
    Ok(if let Some(token) = &number.token {
        let context = token_context_tokens(token, &number.mode_context);
        quote! {
            figma_gpui_runtime::TokenResolver::number_with_context(tokens, #context, #fallback)
        }
    } else {
        quote! { #fallback }
    })
}

fn bound_number_is_visible(number: &BoundValue<f64>) -> bool {
    number.token.is_some() || number.fallback != 0.0
}

fn token_context_tokens(token: &TokenRef, mode_context: &BTreeMap<String, String>) -> TokenStream {
    let id = &token.id;
    let collection_id = token.collection_id.as_ref().map_or_else(
        || quote! { None },
        |collection_id| quote! { Some(#collection_id) },
    );
    let mode_id = token
        .mode_id
        .as_ref()
        .map_or_else(|| quote! { None }, |mode_id| quote! { Some(#mode_id) });
    let modes = mode_context.iter().map(|(collection_id, mode_id)| {
        quote! { (#collection_id, #mode_id) }
    });
    quote! {
        figma_gpui_runtime::TokenContext {
            id: #id,
            collection_id: #collection_id,
            mode_id: #mode_id,
            modes: &[#(#modes),*],
        }
    }
}

fn packed_rgba(node: &Node, property: &str, color: Color) -> Result<u32, CodegenError> {
    fn channel(node: &Node, property: &str, name: &str, value: f64) -> Result<u8, CodegenError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(invalid(
                node,
                property,
                format!("{name} channel {value} is not normalized"),
            ));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = (value * 255.0).round() as u8;
        Ok(byte)
    }

    let red = channel(node, property, "red", color.r)?;
    let green = channel(node, property, "green", color.g)?;
    let blue = channel(node, property, "blue", color.b)?;
    let alpha = channel(node, property, "alpha", color.a)?;
    Ok(u32::from_be_bytes([red, green, blue, alpha]))
}

fn checked_f32(
    node: &Node,
    property: &str,
    value: f64,
    nonnegative: bool,
) -> Result<f32, CodegenError> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) || (nonnegative && value < 0.0) {
        return Err(invalid(node, property, value.to_string()));
    }
    #[allow(clippy::cast_possible_truncation)]
    let value = value as f32;
    Ok(value)
}

fn same_f64(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
}

fn same_f32(left: f32, right: f32) -> bool {
    left.to_bits() == right.to_bits()
}

fn node_symbol(ordinal: usize) -> proc_macro2::Ident {
    format_ident!("node_{ordinal:04}")
}

fn build_source_map(rust: &str, emitted: &[IndexedNode<'_>]) -> Result<SourceMap, CodegenError> {
    let lines = rust.lines().collect::<Vec<_>>();
    let mut starts = emitted
        .iter()
        .map(|indexed| {
            let symbol = node_symbol(indexed.ordinal).to_string();
            let needle = format!("fn {symbol}<");
            let start_line = lines
                .iter()
                .position(|line| line.trim_start().starts_with(&needle))
                .map(|line| line + 1)
                .ok_or_else(|| CodegenError::MissingSourceSpan {
                    symbol: symbol.clone(),
                })?;
            Ok((indexed.node.source_id.clone(), symbol, start_line))
        })
        .collect::<Result<Vec<_>, _>>()?;
    starts.sort_by_key(|(_, _, start_line)| *start_line);

    let mut nodes = BTreeMap::new();
    for (index, (source_id, symbol, start_line)) in starts.iter().enumerate() {
        let search_end = starts
            .get(index + 1)
            .map_or(lines.len(), |(_, _, next_start)| {
                next_start.saturating_sub(1)
            });
        let end_line = lines[start_line.saturating_sub(1)..search_end]
            .iter()
            .rposition(|line| line.trim() == "}")
            .map(|offset| start_line + offset)
            .ok_or_else(|| CodegenError::MissingSourceSpan {
                symbol: symbol.clone(),
            })?;
        nodes.insert(
            source_id.clone(),
            SourceMapEntry {
                symbol: symbol.clone(),
                start_line: *start_line,
                end_line,
            },
        );
    }
    Ok(SourceMap { nodes })
}

fn unsupported<T>(
    node: &Node,
    property: &str,
    feature: impl Into<String>,
) -> Result<T, CodegenError> {
    Err(unsupported_error(node, property, feature))
}

fn unsupported_error(node: &Node, property: &str, feature: impl Into<String>) -> CodegenError {
    CodegenError::Unsupported {
        location: ErrorSource {
            node_id: node.source_id.clone(),
            property: property.to_owned(),
        },
        feature: feature.into(),
    }
}

fn invalid(node: &Node, property: &str, value: impl Into<String>) -> CodegenError {
    CodegenError::InvalidValue {
        location: ErrorSource {
            node_id: node.source_id.clone(),
            property: property.to_owned(),
        },
        value: value.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use figma_rust_core::ir::{
        AssetRoute, Axis, AxisSize, AxisSizing, BoundValue, Color, ComponentResolution,
        DesignDocument, Edges, Effect, Layout, Paint, Positioning, Radii, Scroll, Size, Style,
        Text, TextRun, TextStyle,
    };
    use figma_rust_core::raw::{
        RawAction, RawAlignment, RawAsset, RawBlendMode, RawConstraint, RawNodeKind, RawReaction,
        RawStrokeAlign, RawTrigger,
    };
    use serde::Deserialize;

    use super::{CodegenError, generate};

    fn number(value: f64) -> BoundValue<f64> {
        BoundValue {
            token: None,
            mode_context: BTreeMap::new(),
            fallback: value,
        }
    }

    #[derive(Deserialize)]
    struct GoldenEnvelope {
        document: DesignDocument,
    }

    fn basic_document() -> DesignDocument {
        let envelope = serde_json::from_str::<GoldenEnvelope>(include_str!(
            "../../figma-rust-core/tests/fixtures/basic.ir.json"
        ));
        match envelope {
            Ok(envelope) => envelope.document,
            Err(error) => panic!("core golden IR must deserialize: {error}"),
        }
    }

    fn supported_document() -> DesignDocument {
        let mut document = basic_document();
        configure_supported_root(&mut document.roots[0]);
        document.roots[0].children = vec![absolute_child(), text_child()];
        document
    }

    fn configure_supported_root(root: &mut figma_rust_core::ir::Node) {
        root.layout = Layout::Stack {
            axis: Axis::Vertical,
            wrap: true,
            primary_alignment: RawAlignment::SpaceBetween,
            counter_alignment: RawAlignment::Center,
            gap: number(12.0),
            padding: Edges {
                top: number(4.0),
                right: number(8.0),
                bottom: number(12.0),
                left: number(16.0),
            },
            clips_content: true,
            scroll: Scroll {
                horizontal: false,
                vertical: false,
            },
        };
        root.opacity = 0.75;
        root.style.strokes = vec![Paint::Solid {
            color: BoundValue {
                token: None,
                mode_context: BTreeMap::new(),
                fallback: Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.5,
                },
            },
        }];
        root.style.stroke_widths = Edges {
            top: number(1.0),
            right: number(1.0),
            bottom: number(1.0),
            left: number(1.0),
        };
        root.style.effects = vec![Effect::Shadow {
            inset: false,
            color: BoundValue {
                token: None,
                mode_context: BTreeMap::new(),
                fallback: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.25,
                },
            },
            offset_x: 0.0,
            offset_y: 4.0,
            blur: 12.0,
            spread: 1.0,
        }];
    }

    fn empty_style() -> Style {
        Style {
            fills: Vec::new(),
            strokes: Vec::new(),
            stroke_widths: Edges {
                top: number(0.0),
                right: number(0.0),
                bottom: number(0.0),
                left: number(0.0),
            },
            stroke_align: RawStrokeAlign::Inside,
            radii: Radii {
                top_left: number(0.0),
                top_right: number(0.0),
                bottom_right: number(0.0),
                bottom_left: number(0.0),
                smoothing: 0.0,
            },
            effects: Vec::new(),
            blend_mode: RawBlendMode::Normal,
            is_mask: false,
        }
    }

    fn absolute_child() -> figma_rust_core::ir::Node {
        let mut node = basic_document().roots.remove(0);
        node.source_id = "1:2".to_owned();
        node.name = "Absolute".to_owned();
        node.positioning = Positioning::Absolute {
            x: 8.0,
            y: 10.0,
            horizontal_constraint: RawConstraint::Min,
            vertical_constraint: RawConstraint::Min,
            transform: figma_rust_core::ir::Transform {
                matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            },
        };
        node.style = empty_style();
        node.children.clear();
        node
    }

    fn text_child() -> figma_rust_core::ir::Node {
        let mut node = basic_document().roots.remove(0);
        node.source_id = "1:3".to_owned();
        node.name = "Label".to_owned();
        node.kind = RawNodeKind::Text;
        node.size = Size {
            horizontal: AxisSize {
                sizing: AxisSizing::Hug,
                measured: None,
                min: None,
                max: None,
            },
            vertical: AxisSize {
                sizing: AxisSizing::Fill,
                measured: None,
                min: None,
                max: None,
            },
            aspect_ratio: None,
        };
        node.style.fills = Vec::new();
        node.text = Some(Text {
            characters: "Generated from Figma".to_owned(),
            runs: Vec::new(),
        });
        node.children.clear();
        node
    }

    #[test]
    fn emits_ast_backed_token_resolution_and_source_map() {
        let output = match generate(&basic_document()) {
            Ok(output) => output,
            Err(error) => panic!("basic document must generate: {error}"),
        };

        assert!(syn::parse_file(&output.rust).is_ok());
        assert!(output.rust.contains("fn node_0000<"));
        assert!(output.rust.contains("ParentElement as _"));
        assert!(output.rust.contains("TokenResolver::color"));
        assert!(output.rust.contains("TokenResolver::number_with_context"));
        assert!(
            output
                .rust
                .contains("modes: &[(\"collection.theme\", \"mode.light\")]")
        );
        assert!(output.rust.contains(".border_t("));
        assert!(output.rust.contains(".border_r("));
        assert!(output.rust.contains(".border_b("));
        assert!(output.rust.contains(".border_l("));
        assert!(output.rust.contains(".rounded_tl("));
        assert!(output.rust.contains(".rounded_tr("));
        assert!(output.rust.contains(".rounded_br("));
        assert!(output.rust.contains(".rounded_bl("));
        assert!(output.rust.contains("gpui::rgba(0x2040_80ff)"));
        assert!(output.rust.contains(".min_w(gpui::px(80f32))"));
        assert!(output.rust.contains(".max_w(gpui::px(240f32))"));
        let mapped = output.source_map.nodes.get("1:1");
        assert_eq!(mapped.map(|entry| entry.symbol.as_str()), Some("node_0000"));
        assert!(mapped.is_some_and(|entry| entry.start_line <= entry.end_line));
    }

    #[test]
    fn lowers_supported_stack_absolute_text_and_visual_styles_in_preorder() {
        let document = supported_document();
        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("supported slice must generate: {error}"),
        };
        for expected in [
            ".flex_col()",
            ".flex_wrap()",
            ".justify_between()",
            ".items_center()",
            ".overflow_hidden()",
            ".border(gpui::px(1f32))",
            ".opacity(0.75f32)",
            "gpui::BoxShadow::new(",
            ".absolute()",
            ".left(gpui::px(8f32))",
            ".h_full()",
            ".child(\"Generated from Figma\")",
        ] {
            assert!(output.rust.contains(expected), "missing `{expected}`");
        }
        let root_position = output.rust.find("fn node_0000<");
        let absolute_position = output.rust.find("fn node_0001<");
        let text_position = output.rust.find("fn node_0002<");
        assert!(matches!(
            (root_position, absolute_position, text_position),
            (Some(root), Some(absolute), Some(text)) if root < absolute && absolute < text
        ));
        assert_eq!(output.source_map.nodes.len(), 3);
        for (source_id, symbol) in [
            ("1:1", "node_0000"),
            ("1:2", "node_0001"),
            ("1:3", "node_0002"),
        ] {
            let entry = output.source_map.nodes.get(source_id);
            assert_eq!(entry.map(|entry| entry.symbol.as_str()), Some(symbol));
            if let Some(entry) = entry {
                let start = output.rust.lines().nth(entry.start_line - 1);
                let end = output.rust.lines().nth(entry.end_line - 1);
                assert!(start.is_some_and(|line| line.contains(&format!("fn {symbol}<"))));
                assert_eq!(end.map(str::trim), Some("}"));
            }
        }
    }

    #[test]
    fn inside_stroke_is_an_absolute_overlay_that_does_not_change_layout() {
        let output = match generate(&supported_document()) {
            Ok(output) => output,
            Err(error) => panic!("supported slice must generate: {error}"),
        };
        let compact = output.rust.split_whitespace().collect::<String>();

        assert!(compact.contains(
            ".child(gpui::div().absolute().inset_0().border(gpui::px(1f32)).border_color("
        ));
    }

    #[test]
    fn hug_width_text_does_not_wrap_when_local_font_metrics_are_wider() {
        let document = supported_document();
        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("supported text must generate: {error}"),
        };
        let Some(entry) = output.source_map.nodes.get("1:3") else {
            panic!("text child must be source mapped");
        };
        let function = output
            .rust
            .lines()
            .skip(entry.start_line - 1)
            .take(entry.end_line - entry.start_line + 1)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(function.contains(".whitespace_nowrap()"));
    }

    #[test]
    fn lowers_figma_weight_style_names_without_losing_posture() {
        let mut document = supported_document();
        document.roots[0].children[1].text = Some(Text {
            characters: "Foundations".to_owned(),
            runs: vec![TextRun {
                start_utf16: 0,
                end_utf16: 11,
                text: "Foundations".to_owned(),
                style: TextStyle {
                    font_family: Some("Inter".to_owned()),
                    font_style: Some("Semi Bold".to_owned()),
                    font_weight: Some(600),
                    ..TextStyle::default()
                },
            }],
        });

        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("Figma weight style must generate: {error}"),
        };
        assert!(
            output
                .rust
                .contains(".font_weight(gpui::FontWeight(600f32))")
        );
        assert!(!output.rust.contains(".italic()"));

        if let Some(run) = document.roots[0].children[1]
            .text
            .as_mut()
            .and_then(|text| text.runs.first_mut())
        {
            run.style.font_style = Some("Condensed".to_owned());
        }
        assert!(matches!(
            generate(&document),
            Err(CodegenError::Unsupported { location, .. })
                if location.property == "text.runs.style.font_style"
        ));
    }

    #[test]
    fn cross_axis_fill_does_not_grow_the_parent_main_axis() {
        let mut document = supported_document();
        if let Layout::Stack { axis, .. } = &mut document.roots[0].layout {
            *axis = Axis::Horizontal;
        }
        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("cross-axis fill must generate: {error}"),
        };
        let Some(entry) = output.source_map.nodes.get("1:3") else {
            panic!("text child must be source mapped");
        };
        let function = output
            .rust
            .lines()
            .skip(entry.start_line - 1)
            .take(entry.end_line - entry.start_line + 1)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(function.contains(".h_full()"));
        assert!(function.contains(".flex_none()"));
        assert!(!function.contains(".flex_1()"));
    }

    #[test]
    fn unsupported_grid_is_node_and_property_scoped() {
        let mut document = basic_document();
        document.roots[0].layout = Layout::Grid {
            columns: Vec::new(),
            rows: Vec::new(),
            column_gap: 0.0,
            row_gap: 0.0,
            padding: figma_rust_core::ir::Edges {
                top: number(0.0),
                right: number(0.0),
                bottom: number(0.0),
                left: number(0.0),
            },
            clips_content: false,
            scroll: figma_rust_core::ir::Scroll {
                horizontal: false,
                vertical: false,
            },
        };

        let error = generate(&document).err();
        assert!(matches!(
            error,
            Some(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:1" && location.property == "layout"
        ));

        document.roots[0].visible = false;
        let hidden_error = generate(&document).err();
        assert!(matches!(
            hidden_error,
            Some(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:1" && location.property == "layout"
        ));
    }

    #[test]
    fn unsupported_effect_and_mapped_component_fail_explicitly() {
        let mut effect_document = basic_document();
        effect_document.roots[0]
            .style
            .effects
            .push(Effect::Unsupported {
                source_kind: "GLASS".to_owned(),
            });
        assert!(matches!(
            generate(&effect_document),
            Err(CodegenError::Unsupported { location, .. })
                if location.property == "style.effects"
        ));
        effect_document.roots[0].visible = false;
        assert!(matches!(
            generate(&effect_document),
            Err(CodegenError::Unsupported { location, .. })
                if location.property == "style.effects"
        ));

        let mut component_document = basic_document();
        component_document.roots[0].component = Some(figma_rust_core::ir::ComponentMetadata {
            role: figma_rust_core::raw::RawComponentRole::Instance,
            component_key: "button".to_owned(),
            component_set_key: None,
            variants: std::collections::BTreeMap::new(),
            properties: std::collections::BTreeMap::new(),
            overrides: Vec::new(),
            resolution: ComponentResolution::Mapped {
                mapping_id: "ui::Button".to_owned(),
            },
        });
        assert!(matches!(
            generate(&component_document),
            Err(CodegenError::Unsupported { location, .. })
                if location.property == "component.resolution"
        ));

        let mut asset_document = basic_document();
        asset_document.roots[0].kind = RawNodeKind::Image;
        assert!(matches!(
            generate(&asset_document),
            Err(CodegenError::Unsupported { location, .. }) if location.property == "kind"
        ));
    }

    #[test]
    fn multiple_fills_fail_for_visible_and_hidden_nodes() {
        let mut document = basic_document();
        let second_fill = document.roots[0].style.fills[0].clone();
        document.roots[0].style.fills.push(second_fill);

        assert!(matches!(
            generate(&document),
            Err(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:1" && location.property == "style.fills"
        ));
        document.roots[0].visible = false;
        assert!(matches!(
            generate(&document),
            Err(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:1" && location.property == "style.fills"
        ));
    }

    #[test]
    fn svg_fallback_uses_manifest_asset_and_collapses_the_captured_subtree() {
        let mut document = supported_document();
        document.roots[0].kind = RawNodeKind::Vector;
        document.roots[0].asset_decision.route = AssetRoute::Svg;
        document.assets = vec![RawAsset {
            id: "node:1:1:svg".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::from([("format".to_owned(), "SVG".to_owned())]),
            payload_base64: Some("PHN2Zy8+".to_owned()),
        }];

        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("SVG fallback must generate: {error}"),
        };
        assert!(output.rust.contains("AssetResolver"));
        assert!(output.rust.contains("gpui::svg()"));
        assert!(output.rust.contains("AssetResolver::asset_path"));
        assert!(output.rust.contains("asset-6e6f64653a313a313a737667.svg"));
        assert_eq!(output.source_map.nodes.len(), 1);
        assert!(!output.rust.contains("fn node_0001<"));
    }

    #[test]
    fn authored_color_svg_uses_the_color_preserving_image_renderer() {
        let mut document = supported_document();
        document.roots[0].kind = RawNodeKind::Vector;
        document.roots[0].asset_decision.route = AssetRoute::Svg;
        document.assets = vec![RawAsset {
            id: "node:1:1:svg".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::from([
                ("format".to_owned(), "SVG".to_owned()),
                ("color_policy".to_owned(), "authored".to_owned()),
            ]),
            payload_base64: Some("PHN2Zy8+".to_owned()),
        }];

        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("authored-color SVG fallback must generate: {error}"),
        };

        assert!(output.rust.contains("gpui::img("));
        assert!(!output.rust.contains("gpui::svg()"));
    }

    #[test]
    fn unknown_svg_color_policy_is_node_scoped() {
        let mut document = supported_document();
        document.roots[0].kind = RawNodeKind::Vector;
        document.roots[0].asset_decision.route = AssetRoute::Svg;
        document.assets = vec![RawAsset {
            id: "node:1:1:svg".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::from([
                ("format".to_owned(), "SVG".to_owned()),
                ("color_policy".to_owned(), "unknown".to_owned()),
            ]),
            payload_base64: Some("PHN2Zy8+".to_owned()),
        }];

        let error = generate(&document).err();

        assert!(matches!(
            error,
            Some(CodegenError::Unsupported { location, feature })
                if location.node_id == "1:1"
                    && location.property == "asset_decision.route"
                    && feature.contains("unsupported SVG color policy")
        ));
    }

    #[test]
    fn asset_aware_functions_remain_warning_free_when_they_do_not_use_assets_directly() {
        let mut document = supported_document();
        document.roots[0].children[0].kind = RawNodeKind::Vector;
        document.roots[0].children[0].asset_decision.route = AssetRoute::Svg;
        document.assets = vec![RawAsset {
            id: "node:1:2:svg".to_owned(),
            source_node_id: "1:2".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::from([("format".to_owned(), "SVG".to_owned())]),
            payload_base64: Some("PHN2Zy8+".to_owned()),
        }];

        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("mixed native/asset document must generate: {error}"),
        };
        assert_eq!(
            output.rust.matches("let _ = assets;").count(),
            output.source_map.nodes.len()
        );
    }

    #[test]
    fn fallback_without_a_matching_payload_is_node_scoped() {
        let mut document = basic_document();
        document.roots[0].asset_decision.route = AssetRoute::Raster;

        assert!(matches!(
            generate(&document),
            Err(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:1" && location.property == "asset_decision.route"
        ));
    }

    #[test]
    fn captured_descendant_reactions_are_not_silently_discarded() {
        let mut document = supported_document();
        document.roots[0].kind = RawNodeKind::Vector;
        document.roots[0].asset_decision.route = AssetRoute::Svg;
        document.roots[0].children[0].reactions = vec![RawReaction {
            trigger: RawTrigger::Click,
            action: RawAction::Emit {
                name: "clicked".to_owned(),
            },
        }];
        document.assets = vec![RawAsset {
            id: "node:1:1:svg".to_owned(),
            source_node_id: "1:1".to_owned(),
            media_type: "image/svg+xml".to_owned(),
            content_hash: None,
            export_settings: BTreeMap::from([("format".to_owned(), "SVG".to_owned())]),
            payload_base64: Some("PHN2Zy8+".to_owned()),
        }];

        assert!(matches!(
            generate(&document),
            Err(CodegenError::Unsupported { location, .. })
                if location.node_id == "1:2" && location.property == "reactions"
        ));
    }

    #[test]
    fn raster_fallback_is_distinct_from_an_image_fill_asset() {
        let mut document = basic_document();
        document.roots[0].asset_decision.route = AssetRoute::Raster;
        document.assets = vec![
            RawAsset {
                id: "image-hash".to_owned(),
                source_node_id: "1:1".to_owned(),
                media_type: "image/png".to_owned(),
                content_hash: Some("image-hash".to_owned()),
                export_settings: BTreeMap::new(),
                payload_base64: Some("aW1hZ2U=".to_owned()),
            },
            RawAsset {
                id: "node:1:1:png".to_owned(),
                source_node_id: "1:1".to_owned(),
                media_type: "image/png".to_owned(),
                content_hash: None,
                export_settings: BTreeMap::from([("format".to_owned(), "PNG".to_owned())]),
                payload_base64: Some("ZmFsbGJhY2s=".to_owned()),
            },
        ];

        let output = match generate(&document) {
            Ok(output) => output,
            Err(error) => panic!("raster fallback must generate: {error}"),
        };
        assert!(output.rust.contains("gpui::img("));
        assert!(output.rust.contains("asset-6e6f64653a313a313a706e67.png"));
        assert!(!output.rust.contains("asset-696d6167652d68617368.png"));
    }
}
