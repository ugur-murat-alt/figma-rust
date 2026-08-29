use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    authoring::AuthoringDocument,
    command::{
        DesignTransaction, TransactionError, TransactionReceipt, apply_transaction,
        transaction_fingerprint,
    },
    validation::{
        DocumentFingerprint, DocumentSummary, ValidationReport, document_fingerprint,
        summarize_document, validate_document,
    },
};

#[derive(Debug, Clone, Default)]
pub struct DesignWorkspace {
    documents: BTreeMap<String, AuthoringDocument>,
    applied_transactions: BTreeMap<String, BTreeMap<String, AppliedTransaction>>,
}

#[derive(Debug, Clone)]
struct AppliedTransaction {
    request_fingerprint: String,
    receipt: TransactionReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentOpenReceipt {
    pub document_id: String,
    pub revision: u64,
    pub replaced: bool,
    pub fingerprint: DocumentFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTransactionReceipt {
    pub replayed: bool,
    pub receipt: TransactionReceipt,
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("document {0:?} is already open")]
    DocumentAlreadyExists(String),
    #[error("document {0:?} is not open")]
    DocumentNotFound(String),
    #[error("document failed validation")]
    ValidationFailed { report: Box<ValidationReport> },
    #[error(
        "transaction id {transaction_id:?} was already used with different request content"
    )]
    TransactionIdConflict { transaction_id: String },
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error("failed to serialize workspace data: {0}")]
    Serialization(String),
}

impl DesignWorkspace {
    pub fn open_document(
        &mut self,
        document: AuthoringDocument,
        replace: bool,
    ) -> Result<DocumentOpenReceipt, WorkspaceError> {
        let validation = validate_document(&document);
        if !validation.valid {
            return Err(WorkspaceError::ValidationFailed {
                report: Box::new(validation),
            });
        }
        let document_id = document.document_id.clone();
        let replaced = self.documents.contains_key(&document_id);
        if replaced && !replace {
            return Err(WorkspaceError::DocumentAlreadyExists(document_id));
        }
        let fingerprint = document_fingerprint(&document)
            .map_err(|error| WorkspaceError::Serialization(error.to_string()))?;
        let revision = document.revision;
        self.documents.insert(document_id.clone(), document);
        self.applied_transactions.remove(&document_id);
        Ok(DocumentOpenReceipt {
            document_id,
            revision,
            replaced,
            fingerprint,
        })
    }

    pub fn remove_document(&mut self, document_id: &str) -> Result<(), WorkspaceError> {
        self.documents
            .remove(document_id)
            .ok_or_else(|| WorkspaceError::DocumentNotFound(document_id.to_owned()))?;
        self.applied_transactions.remove(document_id);
        Ok(())
    }

    #[must_use]
    pub fn document(&self, document_id: &str) -> Option<&AuthoringDocument> {
        self.documents.get(document_id)
    }

    #[must_use]
    pub fn summaries(&self) -> Vec<DocumentSummary> {
        self.documents.values().map(summarize_document).collect()
    }

    #[must_use]
    pub fn document_ids(&self) -> Vec<String> {
        self.documents.keys().cloned().collect()
    }

    pub fn apply(
        &mut self,
        transaction: &DesignTransaction,
    ) -> Result<WorkspaceTransactionReceipt, WorkspaceError> {
        let request_fingerprint = transaction_fingerprint(transaction)
            .map_err(|error| WorkspaceError::Serialization(error.to_string()))?;
        if let Some(applied) = self
            .applied_transactions
            .get(&transaction.document_id)
            .and_then(|items| items.get(&transaction.transaction_id))
        {
            if applied.request_fingerprint != request_fingerprint {
                return Err(WorkspaceError::TransactionIdConflict {
                    transaction_id: transaction.transaction_id.clone(),
                });
            }
            return Ok(WorkspaceTransactionReceipt {
                replayed: true,
                receipt: applied.receipt.clone(),
            });
        }

        let document = self
            .documents
            .get_mut(&transaction.document_id)
            .ok_or_else(|| WorkspaceError::DocumentNotFound(transaction.document_id.clone()))?;
        let receipt = apply_transaction(document, transaction)?;
        self.applied_transactions
            .entry(transaction.document_id.clone())
            .or_default()
            .insert(
                transaction.transaction_id.clone(),
                AppliedTransaction {
                    request_fingerprint,
                    receipt: receipt.clone(),
                },
            );
        Ok(WorkspaceTransactionReceipt {
            replayed: false,
            receipt,
        })
    }
}
