//! Small GPUI contracts required by generated figma-rust views.
//!
//! This crate intentionally contains no controls, application state, navigation,
//! or design-system policy.

use std::path::{Path, PathBuf};

use gpui::Hsla;

/// Source variable identity and the consumer's resolved collection modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenContext<'a> {
    pub id: &'a str,
    pub collection_id: Option<&'a str>,
    pub mode_id: Option<&'a str>,
    pub modes: &'a [(&'a str, &'a str)],
}

/// Resolves a Figma variable while retaining the source's current literal value.
///
/// Applications can map token names into their own theme. The default resolver
/// makes generated views usable before an application mapping exists.
pub trait TokenResolver {
    fn color(&self, _token: &str, fallback: Hsla) -> Hsla {
        fallback
    }

    fn number(&self, _token: &str, fallback: f32) -> f32 {
        fallback
    }

    fn color_with_context(&self, context: TokenContext<'_>, fallback: Hsla) -> Hsla {
        self.color(context.id, fallback)
    }

    fn number_with_context(&self, context: TokenContext<'_>, fallback: f32) -> f32 {
        self.number(context.id, fallback)
    }

    fn string(&self, _token: &str, fallback: &str) -> String {
        fallback.to_owned()
    }

    fn boolean(&self, _token: &str, fallback: bool) -> bool {
        fallback
    }
}

/// A resolver that always uses the literal value extracted from Figma.
#[derive(Clone, Copy, Debug, Default)]
pub struct FallbackTokens;

impl TokenResolver for FallbackTokens {}

/// Resolves compiler-owned manifest file names to local asset paths.
pub trait AssetResolver {
    fn asset_path(&self, file_name: &str) -> PathBuf;
}

/// Resolves generated assets relative to one application-owned directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryAssets {
    root: PathBuf,
}

impl DirectoryAssets {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl AssetResolver for DirectoryAssets {
    fn asset_path(&self, file_name: &str) -> PathBuf {
        self.root.join(file_name)
    }
}

/// Returns the deterministic selector used by GPUI geometry tests.
#[must_use]
pub fn source_selector(index: usize) -> String {
    format!("figma-node-{index:04}")
}

/// Receives typed UI actions from generated views.
///
/// The application implements behavior; generated code only emits the contract.
pub trait ActionSink<A> {
    fn emit(&mut self, action: A);
}

#[cfg(test)]
mod tests {
    use super::{
        AssetResolver, DirectoryAssets, FallbackTokens, TokenContext, TokenResolver,
        source_selector,
    };
    use gpui::hsla;

    #[test]
    fn fallback_tokens_preserve_literals() {
        let tokens = FallbackTokens;
        let color = hsla(0.5, 0.4, 0.3, 0.2);

        assert_eq!(tokens.color("surface/input", color), color);
        assert!((tokens.number("space/2", 8.0) - 8.0).abs() < f32::EPSILON);
        assert_eq!(tokens.string("copy/label", "Label"), "Label");
        assert!(tokens.boolean("state/enabled", true));
        let context = TokenContext {
            id: "surface/input",
            collection_id: Some("theme"),
            mode_id: Some("dark"),
            modes: &[("theme", "dark")],
        };
        assert_eq!(tokens.color_with_context(context, color), color);
        assert!((tokens.number_with_context(context, 8.0) - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn selectors_are_stable_and_sortable() {
        assert_eq!(source_selector(7), "figma-node-0007");
        assert_eq!(source_selector(42), "figma-node-0042");
    }

    #[test]
    fn directory_assets_resolve_manifest_file_names() {
        let assets = DirectoryAssets::new("generated-assets");
        assert_eq!(assets.root(), std::path::Path::new("generated-assets"));
        assert_eq!(
            assets.asset_path("asset-01.svg"),
            std::path::PathBuf::from("generated-assets/asset-01.svg")
        );
    }
}
