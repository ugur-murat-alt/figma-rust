//! Target-independent raw Figma model, normalized design IR, and diagnostics.

pub mod diagnostic;
pub mod ir;
pub mod normalize;
pub mod raw;

pub use diagnostic::{Diagnostic, Severity};
pub use normalize::{
    ComponentMapping, ComponentRegistry, NormalizationOutput, ParseError, normalize_bundle,
    normalize_bundle_with_registry, parse_and_normalize, parse_bundle,
};
