//! Figma-independent authoring model and transactional workspace for GPUI design.
//!
//! This crate owns the editable design source of truth. It intentionally sits
//! above the existing compiler IR: authoring documents are revised through
//! explicit commands, validated atomically, and only then lowered by target
//! adapters.

use std::cmp::Ordering;

pub mod authoring;
pub mod command;
#[allow(unused_imports)]
pub mod validation;
pub mod workspace;

pub use authoring::{
    AUTHORING_SCHEMA_VERSION, Alignment, AuthoringDocument, BindingTarget, BindingTargetKind,
    CodeBinding, CodeOwnership, ColorValue, ComponentContract, ComponentInstance, ComponentRole,
    CrossAlignment, DesignNode, DesignTarget, DesignToken, DesignValue, Distribution, EdgeValues,
    Effect, EventContract, EventPayloadKind, Fill, FontValue, GradientStop, GridSpec, GridTrack,
    ImageFit, InstanceValue, LayoutFlow, LayoutSpec, MotionValue, NodeKind, Paint, PositionSpec,
    RadiusValues, RustSymbol, ScrollSpec, SizingRule, SlotContract, Stroke, SyncPolicy,
    TextContent, TokenKind, TokenScope, TokenValue, VariantAxis, VisualStyle,
};
pub use command::{
    DESIGN_COMMAND_VERSION, DesignCommand, DesignTransaction, TransactionError,
    TransactionReceipt, apply_transaction, transaction_fingerprint,
};
pub use validation::{
    DesignDiagnostic, DiagnosticSeverity, DocumentFingerprint, DocumentSummary, LoweringManifest,
    ValidationReport, document_fingerprint, lowering_manifest, summarize_document,
    validate_document,
};
pub use workspace::{
    DesignWorkspace, DocumentOpenReceipt, WorkspaceError, WorkspaceTransactionReceipt,
};

impl PartialOrd for ComponentRole {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ComponentRole {
    fn cmp(&self, other: &Self) -> Ordering {
        component_role_rank(*self).cmp(&component_role_rank(*other))
    }
}

const fn component_role_rank(role: ComponentRole) -> u8 {
    match role {
        ComponentRole::Primitive => 0,
        ComponentRole::Control => 1,
        ComponentRole::Composite => 2,
        ComponentRole::Module => 3,
        ComponentRole::Shell => 4,
        ComponentRole::Overlay => 5,
    }
}
