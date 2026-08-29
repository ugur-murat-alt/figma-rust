//! Figma-independent authoring model and transactional workspace for GPUI design.
//!
//! This crate owns the editable design source of truth. It intentionally sits
//! above the existing compiler IR: authoring documents are revised through
//! explicit commands, validated atomically, and only then lowered by target
//! adapters.

pub mod authoring;
pub mod command;
pub mod validation;
pub mod workspace;

pub use authoring::{
    AUTHORING_SCHEMA_VERSION, Alignment, AuthoringDocument, BindingTarget, BindingTargetKind,
    CodeBinding, CodeOwnership, ColorValue, ComponentContract, ComponentInstance, ComponentRole,
    CrossAlignment, DesignNode, DesignTarget, DesignToken, DesignValue, Distribution, EdgeValues,
    Effect, EventContract, EventPayloadKind, Fill, FontValue, GridSpec, GridTrack, InstanceValue,
    LayoutFlow, LayoutSpec, MotionValue, NodeKind, Paint, PositionSpec, RadiusValues, RustSymbol,
    ScrollSpec, SizingRule, SlotContract, Stroke, SyncPolicy, TextContent, TokenKind, TokenScope,
    TokenValue, VariantAxis, VisualStyle,
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
    DesignWorkspace, WorkspaceError, WorkspaceTransactionReceipt,
};
