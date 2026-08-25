use serde::{Deserialize, Serialize};

/// Stable diagnostic severity used by extraction, normalization, and consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A source-scoped compiler diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    pub(crate) fn node(
        severity: Severity,
        code: &str,
        message: impl Into<String>,
        node_id: &str,
        property_path: Option<&str>,
    ) -> Self {
        Self {
            severity,
            code: code.to_owned(),
            message: message.into(),
            node_id: Some(node_id.to_owned()),
            property_path: property_path.map(str::to_owned),
            help: None,
        }
    }

    pub(crate) fn bundle(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.to_owned(),
            message: message.into(),
            node_id: None,
            property_path: None,
            help: None,
        }
    }
}

pub mod codes {
    pub const SCHEMA_VERSION: &str = "FR-SCHEMA-001";
    pub const DUPLICATE_NODE_ID: &str = "FR-SCHEMA-002";
    pub const INVALID_NUMBER: &str = "FR-SCHEMA-003";
    pub const DUPLICATE_METADATA_ID: &str = "FR-SCHEMA-004";
    pub const INVALID_VARIABLE_MODE_CONTEXT: &str = "FR-SCHEMA-005";
    pub const INVALID_TEXT_RANGE: &str = "FR-TEXT-001";
    pub const AMBIGUOUS_FILL: &str = "FR-LAYOUT-001";
    pub const MISSING_SIZING: &str = "FR-LAYOUT-002";
    pub const INVALID_CONSTRAINT: &str = "FR-LAYOUT-003";
    pub const UNRESOLVED_TOKEN: &str = "FR-TOKEN-001";
    pub const UNMAPPED_COMPONENT: &str = "FR-COMPONENT-001";
    pub const COMPONENT_METADATA_MISMATCH: &str = "FR-COMPONENT-002";
    pub const UNSUPPORTED_EFFECT: &str = "FR-PAINT-001";
    pub const RUNTIME_FALLBACK: &str = "FR-ASSET-001";
    pub const SVG_FALLBACK: &str = "FR-ASSET-002";
    pub const RASTER_FALLBACK: &str = "FR-ASSET-003";
    pub const MISSING_ASSET: &str = "FR-ASSET-004";
}
