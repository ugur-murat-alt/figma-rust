use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::{
    authoring::{
        AuthoringDocument, BindingTargetKind, CodeBinding, ComponentContract, DesignNode,
        DesignToken,
    },
    validation::{DocumentFingerprint, ValidationReport, document_fingerprint, validate_document},
};

pub const DESIGN_COMMAND_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignTransaction {
    pub protocol_version: u32,
    pub transaction_id: String,
    pub document_id: String,
    pub expected_revision: u64,
    pub commands: Vec<DesignCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesignCommand {
    CreateNode {
        node: DesignNode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    ReplaceNode {
        node: DesignNode,
    },
    MoveNode {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    DeleteNode {
        node_id: String,
        #[serde(default)]
        cascade: bool,
    },
    UpsertToken {
        token: DesignToken,
    },
    DeleteToken {
        token_id: String,
    },
    UpsertComponent {
        component: ComponentContract,
    },
    DeleteComponent {
        component_id: String,
    },
    UpsertCodeBinding {
        binding: CodeBinding,
    },
    DeleteCodeBinding {
        binding_id: String,
    },
    RenameDocument {
        name: String,
    },
    SetMetadata {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_id: String,
    pub document_id: String,
    pub previous_revision: u64,
    pub revision: u64,
    pub command_count: usize,
    pub transaction_fingerprint: String,
    pub document_fingerprint: DocumentFingerprint,
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error(
        "unsupported design command protocol {actual}; expected {expected}"
    )]
    UnsupportedProtocol { actual: u32, expected: u32 },
    #[error("transaction_id must not be empty")]
    EmptyTransactionId,
    #[error("transaction must contain at least one command")]
    EmptyTransaction,
    #[error("transaction targets document {actual:?}, but loaded document is {expected:?}")]
    DocumentMismatch { expected: String, actual: String },
    #[error("revision conflict: expected {expected}, current revision is {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("command {index} failed: {message}")]
    CommandFailed { index: usize, message: String },
    #[error("transaction would leave the authoring document invalid")]
    ValidationFailed { report: Box<ValidationReport> },
    #[error("revision overflow")]
    RevisionOverflow,
    #[error("failed to serialize transaction or document: {0}")]
    Serialization(String),
}

pub fn apply_transaction(
    document: &mut AuthoringDocument,
    transaction: &DesignTransaction,
) -> Result<TransactionReceipt, TransactionError> {
    if transaction.protocol_version != DESIGN_COMMAND_VERSION {
        return Err(TransactionError::UnsupportedProtocol {
            actual: transaction.protocol_version,
            expected: DESIGN_COMMAND_VERSION,
        });
    }
    if transaction.transaction_id.trim().is_empty() {
        return Err(TransactionError::EmptyTransactionId);
    }
    if transaction.commands.is_empty() {
        return Err(TransactionError::EmptyTransaction);
    }
    if transaction.document_id != document.document_id {
        return Err(TransactionError::DocumentMismatch {
            expected: document.document_id.clone(),
            actual: transaction.document_id.clone(),
        });
    }
    if transaction.expected_revision != document.revision {
        return Err(TransactionError::RevisionConflict {
            expected: transaction.expected_revision,
            actual: document.revision,
        });
    }

    let previous_revision = document.revision;
    let mut candidate = document.clone();
    for (index, command) in transaction.commands.iter().enumerate() {
        apply_command(&mut candidate, command).map_err(|message| {
            TransactionError::CommandFailed { index, message }
        })?;
    }
    candidate.revision = candidate
        .revision
        .checked_add(1)
        .ok_or(TransactionError::RevisionOverflow)?;

    let report = validate_document(&candidate);
    if !report.valid {
        return Err(TransactionError::ValidationFailed {
            report: Box::new(report),
        });
    }

    let transaction_fingerprint = transaction_fingerprint(transaction)
        .map_err(|error| TransactionError::Serialization(error.to_string()))?;
    let new_document_fingerprint = document_fingerprint(&candidate)
        .map_err(|error| TransactionError::Serialization(error.to_string()))?;
    let receipt = TransactionReceipt {
        transaction_id: transaction.transaction_id.clone(),
        document_id: transaction.document_id.clone(),
        previous_revision,
        revision: candidate.revision,
        command_count: transaction.commands.len(),
        transaction_fingerprint,
        document_fingerprint: new_document_fingerprint,
    };
    *document = candidate;
    Ok(receipt)
}

fn apply_command(
    document: &mut AuthoringDocument,
    command: &DesignCommand,
) -> Result<(), String> {
    match command {
        DesignCommand::CreateNode {
            node,
            parent,
            index,
        } => create_node(document, node.clone(), parent.clone(), *index),
        DesignCommand::ReplaceNode { node } => replace_node(document, node.clone()),
        DesignCommand::MoveNode {
            node_id,
            parent,
            index,
        } => move_node(document, node_id, parent.clone(), *index),
        DesignCommand::DeleteNode { node_id, cascade } => {
            delete_node(document, node_id, *cascade)
        }
        DesignCommand::UpsertToken { token } => {
            document.tokens.insert(token.id.clone(), token.clone());
            Ok(())
        }
        DesignCommand::DeleteToken { token_id } => {
            document
                .tokens
                .remove(token_id)
                .ok_or_else(|| format!("token {token_id:?} does not exist"))?;
            Ok(())
        }
        DesignCommand::UpsertComponent { component } => {
            document
                .components
                .insert(component.id.clone(), component.clone());
            Ok(())
        }
        DesignCommand::DeleteComponent { component_id } => {
            document
                .components
                .remove(component_id)
                .ok_or_else(|| format!("component {component_id:?} does not exist"))?;
            Ok(())
        }
        DesignCommand::UpsertCodeBinding { binding } => {
            document
                .code_bindings
                .insert(binding.id.clone(), binding.clone());
            Ok(())
        }
        DesignCommand::DeleteCodeBinding { binding_id } => {
            document
                .code_bindings
                .remove(binding_id)
                .ok_or_else(|| format!("code binding {binding_id:?} does not exist"))?;
            for component in document.components.values_mut() {
                if component.code_binding_id.as_deref() == Some(binding_id) {
                    component.code_binding_id = None;
                }
            }
            Ok(())
        }
        DesignCommand::RenameDocument { name } => {
            if name.trim().is_empty() {
                return Err("document name must not be empty".to_owned());
            }
            document.name.clone_from(name);
            Ok(())
        }
        DesignCommand::SetMetadata { key, value } => {
            if key.trim().is_empty() {
                return Err("metadata key must not be empty".to_owned());
            }
            if let Some(value) = value {
                document.metadata.insert(key.clone(), value.clone());
            } else {
                document.metadata.remove(key);
            }
            Ok(())
        }
    }
}

fn create_node(
    document: &mut AuthoringDocument,
    mut node: DesignNode,
    parent: Option<String>,
    index: Option<usize>,
) -> Result<(), String> {
    if document.nodes.contains_key(&node.id) {
        return Err(format!("node {:?} already exists", node.id));
    }
    if !node.children.is_empty() {
        return Err("CREATE_NODE accepts one detached node; create descendants explicitly".to_owned());
    }
    if parent.as_deref() == Some(node.id.as_str()) {
        return Err("node cannot be its own parent".to_owned());
    }
    ensure_parent_exists(document, parent.as_deref())?;
    node.parent.clone_from(&parent);
    let node_id = node.id.clone();
    document.nodes.insert(node_id.clone(), node);
    insert_reference(document, parent.as_deref(), &node_id, index)
}

fn replace_node(document: &mut AuthoringDocument, node: DesignNode) -> Result<(), String> {
    let current = document
        .nodes
        .get(&node.id)
        .ok_or_else(|| format!("node {:?} does not exist", node.id))?;
    if current.parent != node.parent || current.children != node.children {
        return Err(
            "REPLACE_NODE cannot change parent or children; use MOVE_NODE and CREATE/DELETE_NODE"
                .to_owned(),
        );
    }
    document.nodes.insert(node.id.clone(), node);
    Ok(())
}

fn move_node(
    document: &mut AuthoringDocument,
    node_id: &str,
    parent: Option<String>,
    index: Option<usize>,
) -> Result<(), String> {
    if parent.as_deref() == Some(node_id) {
        return Err("node cannot be its own parent".to_owned());
    }
    ensure_parent_exists(document, parent.as_deref())?;
    let previous_parent = document
        .nodes
        .get(node_id)
        .ok_or_else(|| format!("node {node_id:?} does not exist"))?
        .parent
        .clone();
    remove_reference(document, previous_parent.as_deref(), node_id)?;
    insert_reference(document, parent.as_deref(), node_id, index)?;
    document
        .nodes
        .get_mut(node_id)
        .ok_or_else(|| format!("node {node_id:?} disappeared during move"))?
        .parent = parent;
    Ok(())
}

fn delete_node(
    document: &mut AuthoringDocument,
    node_id: &str,
    cascade: bool,
) -> Result<(), String> {
    let node = document
        .nodes
        .get(node_id)
        .ok_or_else(|| format!("node {node_id:?} does not exist"))?;
    if !cascade && !node.children.is_empty() {
        return Err("node has children; use cascade=true or delete descendants first".to_owned());
    }
    let parent = node.parent.clone();
    let removed = if cascade {
        collect_subtree(document, node_id)
    } else {
        BTreeSet::from([node_id.to_owned()])
    };
    remove_reference(document, parent.as_deref(), node_id)?;
    document.roots.retain(|root| !removed.contains(root));
    for remaining in document.nodes.values_mut() {
        remaining.children.retain(|child| !removed.contains(child));
        if let Some(instance) = &mut remaining.component {
            for nodes in instance.slots.values_mut() {
                nodes.retain(|slot_node| !removed.contains(slot_node));
            }
        }
    }
    for removed_id in &removed {
        document.nodes.remove(removed_id);
    }
    document.code_bindings.retain(|_, binding| {
        !(binding.target.kind == BindingTargetKind::Node && removed.contains(&binding.target.id))
    });
    Ok(())
}

fn collect_subtree(document: &AuthoringDocument, root: &str) -> BTreeSet<String> {
    let mut collected = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(node_id) = pending.pop() {
        if !collected.insert(node_id.clone()) {
            continue;
        }
        if let Some(node) = document.nodes.get(&node_id) {
            pending.extend(node.children.iter().cloned());
        }
    }
    collected
}

fn ensure_parent_exists(document: &AuthoringDocument, parent: Option<&str>) -> Result<(), String> {
    if let Some(parent_id) = parent
        && !document.nodes.contains_key(parent_id)
    {
        return Err(format!("parent node {parent_id:?} does not exist"));
    }
    Ok(())
}

fn insert_reference(
    document: &mut AuthoringDocument,
    parent: Option<&str>,
    node_id: &str,
    index: Option<usize>,
) -> Result<(), String> {
    let children = if let Some(parent_id) = parent {
        &mut document
            .nodes
            .get_mut(parent_id)
            .ok_or_else(|| format!("parent node {parent_id:?} does not exist"))?
            .children
    } else {
        &mut document.roots
    };
    if children.iter().any(|child| child == node_id) {
        return Err(format!("node {node_id:?} is already present in the target"));
    }
    let index = index.unwrap_or(children.len());
    if index > children.len() {
        return Err(format!(
            "insertion index {index} exceeds child count {}",
            children.len()
        ));
    }
    children.insert(index, node_id.to_owned());
    Ok(())
}

fn remove_reference(
    document: &mut AuthoringDocument,
    parent: Option<&str>,
    node_id: &str,
) -> Result<(), String> {
    let children = if let Some(parent_id) = parent {
        &mut document
            .nodes
            .get_mut(parent_id)
            .ok_or_else(|| format!("parent node {parent_id:?} does not exist"))?
            .children
    } else {
        &mut document.roots
    };
    let index = children
        .iter()
        .position(|child| child == node_id)
        .ok_or_else(|| format!("node {node_id:?} is not present in its current parent/root list"))?;
    children.remove(index);
    Ok(())
}

pub fn transaction_fingerprint(
    transaction: &DesignTransaction,
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(transaction)?;
    let digest = Sha256::digest(bytes);
    Ok(hex(&digest))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
