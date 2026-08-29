use gpui_design_core::{
    AUTHORING_SCHEMA_VERSION, BindingTarget, BindingTargetKind, CodeBinding, CodeOwnership,
    DESIGN_COMMAND_VERSION, DesignCommand, DesignNode, DesignTransaction, DesignWorkspace,
    RustSymbol, SyncPolicy, TransactionError, document_fingerprint, validate_document,
};

#[test]
fn transaction_is_atomic_when_a_move_creates_a_cycle() {
    let mut document = gpui_design_core::AuthoringDocument::new("orbitline/login", "Login");
    let root = DesignNode::container("login/root", "Login Root");
    let child = DesignNode::container("login/panel", "Login Panel");
    let create = DesignTransaction {
        protocol_version: DESIGN_COMMAND_VERSION,
        transaction_id: "create-login-tree".to_owned(),
        document_id: document.document_id.clone(),
        expected_revision: 0,
        commands: vec![
            DesignCommand::CreateNode {
                node: root,
                parent: None,
                index: None,
            },
            DesignCommand::CreateNode {
                node: child,
                parent: Some("login/root".to_owned()),
                index: None,
            },
        ],
    };
    gpui_design_core::apply_transaction(&mut document, &create).expect("valid tree");
    let before = document.clone();

    let cycle = DesignTransaction {
        protocol_version: DESIGN_COMMAND_VERSION,
        transaction_id: "cycle-login-tree".to_owned(),
        document_id: document.document_id.clone(),
        expected_revision: 1,
        commands: vec![DesignCommand::MoveNode {
            node_id: "login/root".to_owned(),
            parent: Some("login/panel".to_owned()),
            index: None,
        }],
    };
    let error = gpui_design_core::apply_transaction(&mut document, &cycle)
        .expect_err("cycle must be rejected");
    assert!(matches!(error, TransactionError::ValidationFailed { .. }));
    assert_eq!(
        document, before,
        "failed transaction must not mutate source"
    );
}

#[test]
fn workspace_replays_identical_transaction_idempotently() {
    let document = gpui_design_core::AuthoringDocument::new("orbitline/shell", "Main Shell");
    let mut workspace = DesignWorkspace::default();
    workspace
        .open_document(document, false)
        .expect("open document");
    let transaction = DesignTransaction {
        protocol_version: DESIGN_COMMAND_VERSION,
        transaction_id: "rename-shell".to_owned(),
        document_id: "orbitline/shell".to_owned(),
        expected_revision: 0,
        commands: vec![DesignCommand::RenameDocument {
            name: "OrbitLine Main Shell".to_owned(),
        }],
    };

    let first = workspace.apply(&transaction).expect("first application");
    let replay = workspace.apply(&transaction).expect("idempotent replay");
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(first.receipt, replay.receipt);
    assert_eq!(
        workspace
            .document("orbitline/shell")
            .expect("document remains open")
            .revision,
        1
    );
}

#[test]
fn handwritten_behavior_is_always_reference_only() {
    let mut document = gpui_design_core::AuthoringDocument::new("orbitline/order", "Order Entry");
    document.code_bindings.insert(
        "binding/order-submit".to_owned(),
        CodeBinding {
            id: "binding/order-submit".to_owned(),
            target: BindingTarget {
                kind: BindingTargetKind::Document,
                id: document.document_id.clone(),
            },
            symbol: RustSymbol {
                crate_name: "orbitline".to_owned(),
                module_path: "orders::submit".to_owned(),
                symbol_name: "submit_order".to_owned(),
                file_path: None,
            },
            ownership: CodeOwnership::HandwrittenBehavior,
            sync_policy: SyncPolicy::Bidirectional,
            source_hash: None,
            metadata: Default::default(),
        },
    );

    let report = validate_document(&document);
    assert!(!report.valid);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "GD-BINDING-003")
    );
}

#[test]
fn content_fingerprint_ignores_revision_only_changes() {
    let document = gpui_design_core::AuthoringDocument::new("orbitline/watchlist", "Watchlist");
    let mut later_revision = document.clone();
    later_revision.revision = 42;
    assert_eq!(document.schema_version, AUTHORING_SCHEMA_VERSION);
    assert_eq!(
        document_fingerprint(&document).expect("fingerprint"),
        document_fingerprint(&later_revision).expect("fingerprint")
    );
}
