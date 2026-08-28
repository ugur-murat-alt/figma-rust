//! Target-independent raw Figma model, normalized design IR, and diagnostics.

pub mod compatibility;
pub mod diagnostic;
pub mod ir;
pub mod normalize;
pub mod profile;
pub mod raw;

pub use compatibility::{
    EXTRACTION_FINGERPRINT_VERSION, ExtractionFingerprint, SchemaCompatibility,
    extraction_fingerprint, schema_compatibility,
};
pub use diagnostic::{Diagnostic, Severity};
pub use normalize::{
    ComponentMapping, ComponentRegistry, NormalizationOutput, ParseError, normalize_bundle,
    normalize_bundle_with_registry, parse_and_normalize, parse_bundle,
};
pub use profile::{
    CAPABILITY_PROFILE_REPORT_VERSION, CapabilityInventoryEntry, CapabilityPolicy,
    CapabilityProfileReport, CapabilityProfileSummary, CapabilityStatus, CapabilityUsage,
    ORBITLINE_MINIMAL_PROFILE_V1, evaluate_capability_profile,
};
