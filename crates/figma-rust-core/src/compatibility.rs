use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{normalize::EXTRACTION_SCHEMA_VERSION, raw::ExtractionBundle};

pub const EXTRACTION_FINGERPRINT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchemaCompatibility {
    Current,
    LegacyDiagnosticOnly,
    UnsupportedDiagnosticOnly,
}

impl fmt::Display for SchemaCompatibility {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Current => "CURRENT",
            Self::LegacyDiagnosticOnly => "LEGACY_DIAGNOSTIC_ONLY",
            Self::UnsupportedDiagnosticOnly => "UNSUPPORTED_DIAGNOSTIC_ONLY",
        })
    }
}

#[must_use]
pub const fn schema_compatibility(schema_version: u32) -> SchemaCompatibility {
    if schema_version == EXTRACTION_SCHEMA_VERSION {
        SchemaCompatibility::Current
    } else if schema_version == 1 {
        SchemaCompatibility::LegacyDiagnosticOnly
    } else {
        SchemaCompatibility::UnsupportedDiagnosticOnly
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionFingerprint {
    pub version: u32,
    pub algorithm: String,
    pub value: String,
}

/// Hashes the canonical typed bundle, not its input formatting or object-key order.
///
/// Capability flags are set-like: order and duplicates do not affect the result.
/// Arrays with semantic order, including roots, children, text runs, and selected
/// roots, remain order-sensitive.
///
/// # Errors
///
/// Returns an error if a programmatically constructed bundle contains a value
/// that JSON cannot serialize, such as a non-finite number.
pub fn extraction_fingerprint(
    bundle: &ExtractionBundle,
) -> Result<ExtractionFingerprint, serde_json::Error> {
    let mut canonical = bundle.clone();
    if let Some(manifest) = &mut canonical.extraction_manifest {
        manifest.capabilities.sort();
        manifest.capabilities.dedup();
    }
    let canonical_json = serde_json::to_vec(&canonical)?;
    let mut hasher = Sha256::new();
    hasher.update(b"figma-rust-extraction-fingerprint-v1\0");
    hasher.update(canonical_json);
    let digest = hasher.finalize();
    let mut value = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    Ok(ExtractionFingerprint {
        version: EXTRACTION_FINGERPRINT_VERSION,
        algorithm: "SHA-256".to_owned(),
        value,
    })
}
