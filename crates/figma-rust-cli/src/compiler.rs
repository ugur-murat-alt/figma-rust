use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use figma_rust_codegen::{CodegenError, GeneratedOutput};
use figma_rust_core::{
    Diagnostic, ExtractionFingerprint, NormalizationOutput, SchemaCompatibility, Severity,
    extraction_fingerprint, normalize_bundle, parse_bundle, schema_compatibility,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

const GENERATED_RUST: &str = "generated.rs";
const SOURCE_MAP_JSON: &str = "source-map.json";
const IR_JSON: &str = "ir.json";
const DIAGNOSTICS_JSON: &str = "diagnostics.json";
const ASSET_MANIFEST_JSON: &str = "asset-manifest.json";
const ARTIFACT_NAMES: [&str; 5] = [
    GENERATED_RUST,
    SOURCE_MAP_JSON,
    IR_JSON,
    DIAGNOSTICS_JSON,
    ASSET_MANIFEST_JSON,
];
const ARTIFACT_LOCK: &str = ".figma-rust.lock";
const DIAGNOSTIC_SAMPLE_LIMIT: usize = 3;
const ROOT_STATUS_JSON: &str = "root-status.json";
const ROOT_OUTPUTS_DIRECTORY: &str = "roots";
const MAX_ROOT_SCOPED_ROOTS: usize = 10_000;
const MAX_ROOT_DIRECTORY_ENTRIES: usize = MAX_ROOT_SCOPED_ROOTS * 2 + 1;
const MAX_ROOT_OUTPUT_ENTRIES: usize = 50_000;
static ROOT_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum CliError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse extraction bundle from {path}: {message}")]
    Parse { path: String, message: String },
    #[error("failed to serialize {label}: {source}")]
    Serialize {
        label: &'static str,
        source: serde_json::Error,
    },
    #[error("failed to decode asset {id}: {source}")]
    DecodeAsset {
        id: String,
        source: base64::DecodeError,
    },
    #[error("invalid asset manifest {path}: {message}")]
    AssetManifest { path: String, message: String },
    #[error("unsafe output path {path}: {reason}")]
    UnsafeOutput { path: String, reason: String },
    #[error("failed to create output directory {path}: {source}")]
    CreateOutput {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("artifact transaction failed during {operation}{rollback}")]
    ArtifactTransaction { operation: String, rollback: String },
    #[error("compiler server failed: {0}")]
    Server(String),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct DiagnosticSummary {
    pub info: usize,
    pub warnings: usize,
    pub errors: usize,
}

impl DiagnosticSummary {
    fn from_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        diagnostics
            .iter()
            .fold(Self::default(), |mut summary, item| {
                match item.severity {
                    Severity::Info => summary.info += 1,
                    Severity::Warning => summary.warnings += 1,
                    Severity::Error => summary.errors += 1,
                }
                summary
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DiagnosticSample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticGroup {
    pub severity: Severity,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_root_id: Option<String>,
    pub count: usize,
    pub samples: Vec<DiagnosticSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DiagnosticGroupKey {
    severity_rank: u8,
    code: String,
    property_path: Option<String>,
    selected_root_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InspectNode {
    pub id: String,
    pub name: String,
    pub kind: figma_rust_core::raw::RawNodeKind,
    pub visible: bool,
    pub children: Vec<Self>,
}

impl InspectNode {
    fn from_raw(node: &figma_rust_core::raw::RawNode) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            kind: node.kind,
            visible: node.visible,
            children: node.children.iter().map(Self::from_raw).collect(),
        }
    }

    fn node_count(&self) -> usize {
        1 + self.children.iter().map(Self::node_count).sum::<usize>()
    }

    fn append_text(&self, depth: usize, output: &mut String) {
        output.push_str(&"  ".repeat(depth));
        output.push_str("- [");
        output.push_str(&format!("{:?}", self.kind).to_uppercase());
        output.push_str("] ");
        let quoted_name = format!("{:?}", self.name);
        output.push_str(&quoted_name);
        output.push_str(" (");
        output.push_str(&self.id);
        output.push(')');
        if !self.visible {
            output.push_str(" hidden");
        }
        output.push('\n');
        for child in &self.children {
            child.append_text(depth + 1, output);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InspectReport {
    pub schema_version: u32,
    pub schema_compatibility: SchemaCompatibility,
    pub extraction_fingerprint: ExtractionFingerprint,
    pub root_count: usize,
    pub node_count: usize,
    pub diagnostic_summary: DiagnosticSummary,
    pub tree: Vec<InspectNode>,
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    pub diagnostics: Vec<Diagnostic>,
}

impl InspectReport {
    #[must_use]
    pub fn text(&self, grouped: bool) -> String {
        let mut output = format!(
            "schema {} ({}): {} root(s), {} node(s)\nfingerprint v{} {}: {}\ndiagnostics: {} error(s), {} warning(s), {} info\n",
            self.schema_version,
            self.schema_compatibility,
            self.root_count,
            self.node_count,
            self.extraction_fingerprint.version,
            self.extraction_fingerprint.algorithm,
            self.extraction_fingerprint.value,
            self.diagnostic_summary.errors,
            self.diagnostic_summary.warnings,
            self.diagnostic_summary.info
        );
        if grouped {
            append_diagnostic_groups(&self.diagnostic_groups, &mut output);
            for root in &self.tree {
                root.append_text(0, &mut output);
            }
        } else {
            for root in &self.tree {
                root.append_text(0, &mut output);
            }
            append_diagnostics(&self.diagnostics, &mut output);
        }
        output
    }
}

#[derive(Debug, Serialize)]
pub struct LintReport {
    pub summary: DiagnosticSummary,
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LintReport {
    #[must_use]
    pub const fn failed(&self, strict: bool) -> bool {
        self.summary.errors > 0 || (strict && self.summary.warnings > 0)
    }

    #[must_use]
    pub fn text(&self, grouped: bool) -> String {
        let mut output = format!(
            "lint: {} error(s), {} warning(s), {} info\n",
            self.summary.errors, self.summary.warnings, self.summary.info
        );
        if grouped {
            append_diagnostic_groups(&self.diagnostic_groups, &mut output);
        } else {
            append_diagnostics(&self.diagnostics, &mut output);
        }
        output
    }
}

#[derive(Debug)]
pub struct CompileResult {
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RootCompileStatus {
    Success,
    NormalizationFailed,
    UnsupportedRuntimeRoute,
    CodegenFailed,
}

impl RootCompileStatus {
    #[must_use]
    pub const fn succeeded(self) -> bool {
        matches!(self, Self::Success)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RootCompileOutcome {
    pub index: usize,
    pub root_id: String,
    pub root_name: String,
    pub root_fingerprint: ExtractionFingerprint,
    pub status: RootCompileStatus,
    pub diagnostic_summary: DiagnosticSummary,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RootCompileSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RootCompileReport {
    pub schema_version: u32,
    pub mode: String,
    pub input_fingerprint: ExtractionFingerprint,
    pub summary: RootCompileSummary,
    pub roots: Vec<RootCompileOutcome>,
}

impl RootCompileReport {
    #[must_use]
    pub const fn failed(&self) -> bool {
        self.summary.failed > 0
    }
}

#[derive(Debug, Serialize)]
pub struct CompilerResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub diagnostic_groups: Vec<DiagnosticGroup>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_map: Option<figma_rust_codegen::SourceMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn inspect_file(path: &Path) -> Result<InspectReport, CliError> {
    let bundle = read_bundle(path)?;
    inspect_bundle(&bundle)
}

fn inspect_bundle(
    bundle: &figma_rust_core::raw::ExtractionBundle,
) -> Result<InspectReport, CliError> {
    let normalization = normalize_bundle(bundle);
    let diagnostics = normalization.diagnostics;
    let diagnostic_groups = group_diagnostics(bundle, &diagnostics);
    let tree = bundle
        .roots
        .iter()
        .map(InspectNode::from_raw)
        .collect::<Vec<_>>();
    let node_count = tree.iter().map(InspectNode::node_count).sum();
    Ok(InspectReport {
        schema_version: bundle.schema_version,
        schema_compatibility: schema_compatibility(bundle.schema_version),
        extraction_fingerprint: extraction_fingerprint(bundle).map_err(|source| {
            CliError::Serialize {
                label: "extraction fingerprint",
                source,
            }
        })?,
        root_count: tree.len(),
        node_count,
        diagnostic_summary: DiagnosticSummary::from_diagnostics(&diagnostics),
        tree,
        diagnostic_groups,
        diagnostics,
    })
}

pub fn lint_file(path: &Path) -> Result<LintReport, CliError> {
    let bundle = read_bundle(path)?;
    Ok(lint_bundle(&bundle))
}

fn lint_bundle(bundle: &figma_rust_core::raw::ExtractionBundle) -> LintReport {
    let normalization = normalize_bundle(bundle);
    let diagnostics = normalization.diagnostics;
    LintReport {
        summary: DiagnosticSummary::from_diagnostics(&diagnostics),
        diagnostic_groups: group_diagnostics(bundle, &diagnostics),
        diagnostics,
    }
}

pub fn compile_file(path: &Path, output_directory: &Path) -> Result<CompileResult, CliError> {
    let bundle = read_bundle(path)?;
    compile_bundle(&bundle, output_directory)
}

fn compile_bundle(
    bundle: &figma_rust_core::raw::ExtractionBundle,
    output_directory: &Path,
) -> Result<CompileResult, CliError> {
    let normalization = normalize_bundle(bundle);
    if normalization.has_errors() {
        invalidate_outputs(output_directory)?;
        return Ok(CompileResult {
            diagnostics: normalization.diagnostics,
            error: Some("normalization failed".to_owned()),
        });
    }

    match figma_rust_codegen::generate(&normalization.document) {
        Ok(generated) => {
            write_outputs(output_directory, &normalization, &generated)?;
            Ok(CompileResult {
                diagnostics: normalization.diagnostics,
                error: None,
            })
        }
        Err(error) => {
            let message = error.to_string();
            let mut diagnostics = normalization.diagnostics;
            diagnostics.push(codegen_diagnostic(&error));
            invalidate_outputs(output_directory)?;
            Ok(CompileResult {
                diagnostics,
                error: Some(message),
            })
        }
    }
}

pub fn compile_file_root_scoped(
    path: &Path,
    output_directory: &Path,
) -> Result<RootCompileReport, CliError> {
    let bundle = read_bundle(path)?;
    if bundle.roots.len() > MAX_ROOT_SCOPED_ROOTS {
        return Err(CliError::UnsafeOutput {
            path: output_directory.display().to_string(),
            reason: format!(
                "root-scoped compile has {} roots, exceeding {MAX_ROOT_SCOPED_ROOTS}",
                bundle.roots.len()
            ),
        });
    }
    let mut root_ids = BTreeSet::new();
    for root in &bundle.roots {
        if !root_ids.insert(root.id.as_str()) {
            return Err(CliError::UnsafeOutput {
                path: output_directory.display().to_string(),
                reason: format!(
                    "root-scoped compile requires unique root IDs; repeated {}",
                    root.id
                ),
            });
        }
    }
    let input_fingerprint =
        extraction_fingerprint(&bundle).map_err(|source| CliError::Serialize {
            label: "root-scoped input fingerprint",
            source,
        })?;
    let created_directory = ensure_output_directory(output_directory)?;
    let result = with_artifact_lock(output_directory, || {
        recover_generation(output_directory)?;
        let roots_directory = output_directory.join(ROOT_OUTPUTS_DIRECTORY);
        ensure_output_directory(&roots_directory)?;
        cleanup_root_temporary_directories(&roots_directory)?;

        let mut outcomes = Vec::with_capacity(bundle.roots.len());
        let mut successful_directories = BTreeSet::new();
        for (index, root) in bundle.roots.iter().enumerate() {
            let (outcome, successful_directory) =
                compile_root_outcome(&bundle, root, index, &roots_directory)?;
            successful_directories.extend(successful_directory);
            outcomes.push(outcome);
        }

        let succeeded = outcomes
            .iter()
            .filter(|outcome| outcome.status.succeeded())
            .count();
        let report = RootCompileReport {
            schema_version: 1,
            mode: "ROOT_SCOPED".to_owned(),
            input_fingerprint,
            summary: RootCompileSummary {
                total: outcomes.len(),
                succeeded,
                failed: outcomes.len() - succeeded,
            },
            roots: outcomes,
        };
        let outputs = [OutputArtifact {
            name: ROOT_STATUS_JSON.to_owned(),
            content: serialize_json("root-scoped compile report", &report)?.into_bytes(),
            cache_key: None,
        }];
        commit_artifact_set_locked(output_directory, &outputs)?;
        cleanup_root_directories_except(&roots_directory, &successful_directories)?;
        Ok(report)
    });
    finish_output_directory(result, output_directory, created_directory)
}

fn compile_root_outcome(
    bundle: &figma_rust_core::raw::ExtractionBundle,
    root: &figma_rust_core::raw::RawNode,
    index: usize,
    roots_directory: &Path,
) -> Result<(RootCompileOutcome, Option<PathBuf>), CliError> {
    let root_bundle = root_bundle(bundle, root);
    let root_fingerprint =
        extraction_fingerprint(&root_bundle).map_err(|source| CliError::Serialize {
            label: "root-scoped root fingerprint",
            source,
        })?;
    let identity_hash = root_identity_hash(&root.id);
    let temporary_output = roots_directory.join(format!(
        ".tmp-root-{identity_hash}-{}-{}",
        std::process::id(),
        ROOT_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = compile_bundle(&root_bundle, &temporary_output)?;
    let status = root_compile_status(&result);
    let (artifact_directory, generation_id, successful_directory) = if status.succeeded() {
        let generation_id = crate::generation::current_generation_id(&temporary_output)
            .map_err(|error| transaction_error(error, &[]))?;
        let directory_name =
            root_directory_name(&identity_hash, &root_fingerprint.value, &generation_id);
        let relative_directory = format!("{ROOT_OUTPUTS_DIRECTORY}/{directory_name}");
        let root_output = roots_directory.join(directory_name);
        publish_root_output(
            &temporary_output,
            &root_output,
            &generation_id,
            roots_directory,
        )?;
        (
            Some(relative_directory),
            Some(generation_id),
            Some(root_output),
        )
    } else {
        cleanup_inactive_root_output(&temporary_output)?;
        (None, None, None)
    };
    Ok((
        RootCompileOutcome {
            index,
            root_id: root.id.clone(),
            root_name: root.name.clone(),
            root_fingerprint,
            status,
            diagnostic_summary: DiagnosticSummary::from_diagnostics(&result.diagnostics),
            diagnostics: result.diagnostics,
            error: result.error,
            artifact_directory,
            generation_id,
        },
        successful_directory,
    ))
}

fn publish_root_output(
    temporary_output: &Path,
    root_output: &Path,
    generation_id: &str,
    roots_directory: &Path,
) -> Result<(), CliError> {
    if validate_existing_output_directory(root_output)? {
        with_artifact_lock(root_output, || recover_generation(root_output))?;
        let existing_id = crate::generation::current_generation_id(root_output)
            .map_err(|error| transaction_error(error, &[]))?;
        if existing_id != generation_id {
            return Err(CliError::UnsafeOutput {
                path: root_output.display().to_string(),
                reason: "root output directory generation does not match its name".to_owned(),
            });
        }
        cleanup_inactive_root_output(temporary_output)
    } else {
        fs::rename(temporary_output, root_output).map_err(|source| CliError::Write {
            path: root_output.display().to_string(),
            source,
        })?;
        sync_directory(roots_directory)
    }
}

fn root_bundle(
    bundle: &figma_rust_core::raw::ExtractionBundle,
    root: &figma_rust_core::raw::RawNode,
) -> figma_rust_core::raw::ExtractionBundle {
    let mut node_ids = BTreeSet::new();
    collect_node_ids(root, &mut node_ids);
    let mut scoped = bundle.clone();
    scoped.source.selected_node_ids = vec![root.id.clone()];
    scoped.roots = vec![root.clone()];
    scoped
        .assets
        .retain(|asset| node_ids.contains(asset.source_node_id.as_str()));
    scoped.extraction_diagnostics.retain(|diagnostic| {
        diagnostic
            .node_id
            .as_deref()
            .is_none_or(|node_id| node_ids.contains(node_id))
    });
    scoped.extraction_manifest = None;
    scoped
}

fn collect_node_ids<'a>(node: &'a figma_rust_core::raw::RawNode, ids: &mut BTreeSet<&'a str>) {
    ids.insert(&node.id);
    for child in &node.children {
        collect_node_ids(child, ids);
    }
}

fn root_compile_status(result: &CompileResult) -> RootCompileStatus {
    if result.error.is_none() {
        RootCompileStatus::Success
    } else if result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == figma_rust_core::diagnostic::codes::RUNTIME_FALLBACK)
    {
        RootCompileStatus::UnsupportedRuntimeRoute
    } else if result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code.starts_with("FR-CODEGEN-"))
    {
        RootCompileStatus::CodegenFailed
    } else {
        RootCompileStatus::NormalizationFailed
    }
}

fn root_identity_hash(root_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"figma-rust-root-directory-v1\0");
    hasher.update(root_id.as_bytes());
    let digest = hasher.finalize();
    let mut identity = String::with_capacity(64);
    for byte in digest {
        write!(identity, "{byte:02x}").expect("writing a SHA-256 digest to String cannot fail");
    }
    identity
}

fn root_directory_name(identity_hash: &str, root_fingerprint: &str, generation_id: &str) -> String {
    format!("root-{identity_hash}-{root_fingerprint}-{generation_id}")
}

fn cleanup_inactive_root_output(output_directory: &Path) -> Result<(), CliError> {
    if !validate_existing_output_directory(output_directory)? {
        return Ok(());
    }
    invalidate_outputs(output_directory)?;
    with_artifact_lock(output_directory, || recover_generation(output_directory))?;
    remove_compiler_owned_tree(output_directory)
}

fn cleanup_root_directories_except(
    roots_directory: &Path,
    retained: &BTreeSet<PathBuf>,
) -> Result<(), CliError> {
    if !validate_existing_output_directory(roots_directory)? {
        return Ok(());
    }
    let entries = fs::read_dir(roots_directory).map_err(|source| CliError::Read {
        path: roots_directory.display().to_string(),
        source,
    })?;
    let mut seen = 0_usize;
    for entry in entries {
        seen += 1;
        if seen > MAX_ROOT_DIRECTORY_ENTRIES {
            return Err(CliError::UnsafeOutput {
                path: roots_directory.display().to_string(),
                reason: format!(
                    "root output directory exceeds {MAX_ROOT_DIRECTORY_ENTRIES} entries"
                ),
            });
        }
        let entry = entry.map_err(|source| CliError::Read {
            path: roots_directory.display().to_string(),
            source,
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| CliError::UnsafeOutput {
                path: entry.path().display().to_string(),
                reason: "root output directory name is not UTF-8".to_owned(),
            })?;
        if name.starts_with(".tmp-root-") {
            remove_compiler_owned_tree(&entry.path())?;
            continue;
        }
        if !valid_root_directory_name(&name) {
            return Err(CliError::UnsafeOutput {
                path: entry.path().display().to_string(),
                reason: "unexpected path in compiler-owned roots directory".to_owned(),
            });
        }
        if !retained.contains(&entry.path()) {
            remove_compiler_owned_tree(&entry.path())?;
        }
    }
    if retained.is_empty() {
        fs::remove_dir(roots_directory).map_err(|source| CliError::Write {
            path: roots_directory.display().to_string(),
            source,
        })?;
    }
    Ok(())
}

fn cleanup_root_temporary_directories(roots_directory: &Path) -> Result<(), CliError> {
    let entries = fs::read_dir(roots_directory).map_err(|source| CliError::Read {
        path: roots_directory.display().to_string(),
        source,
    })?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_ROOT_DIRECTORY_ENTRIES {
            return Err(CliError::UnsafeOutput {
                path: roots_directory.display().to_string(),
                reason: format!(
                    "root output directory exceeds {MAX_ROOT_DIRECTORY_ENTRIES} entries"
                ),
            });
        }
        let entry = entry.map_err(|source| CliError::Read {
            path: roots_directory.display().to_string(),
            source,
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| CliError::UnsafeOutput {
                path: entry.path().display().to_string(),
                reason: "root output directory name is not UTF-8".to_owned(),
            })?;
        if name.starts_with(".tmp-root-") {
            remove_compiler_owned_tree(&entry.path())?;
        } else if !valid_root_directory_name(&name) {
            return Err(CliError::UnsafeOutput {
                path: entry.path().display().to_string(),
                reason: "unexpected path in compiler-owned roots directory".to_owned(),
            });
        }
    }
    Ok(())
}

fn valid_root_directory_name(name: &str) -> bool {
    let hashes = name
        .strip_prefix("root-")
        .unwrap_or_default()
        .split('-')
        .collect::<Vec<_>>();
    hashes.len() == 3
        && hashes.iter().all(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

fn sync_directory(path: &Path) -> Result<(), CliError> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| CliError::Write {
            path: path.display().to_string(),
            source,
        })
}

fn remove_compiler_owned_tree(path: &Path) -> Result<(), CliError> {
    let mut entries = 0_usize;
    validate_compiler_owned_tree(path, &mut entries)?;
    fs::remove_dir_all(path).map_err(|source| CliError::Write {
        path: path.display().to_string(),
        source,
    })
}

fn validate_compiler_owned_tree(path: &Path, entries: &mut usize) -> Result<(), CliError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| CliError::Read {
        path: path.display().to_string(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(CliError::UnsafeOutput {
            path: path.display().to_string(),
            reason: "compiler-owned root output contains a symbolic link".to_owned(),
        });
    }
    *entries += 1;
    if *entries > MAX_ROOT_OUTPUT_ENTRIES {
        return Err(CliError::UnsafeOutput {
            path: path.display().to_string(),
            reason: format!("compiler-owned root output exceeds {MAX_ROOT_OUTPUT_ENTRIES} entries"),
        });
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|source| CliError::Read {
            path: path.display().to_string(),
            source,
        })? {
            let entry = entry.map_err(|source| CliError::Read {
                path: path.display().to_string(),
                source,
            })?;
            validate_compiler_owned_tree(&entry.path(), entries)?;
        }
    } else if !metadata.is_file() {
        return Err(CliError::UnsafeOutput {
            path: path.display().to_string(),
            reason: "compiler-owned root output contains a non-file entry".to_owned(),
        });
    }
    Ok(())
}

pub fn lint_source(input: &str) -> Result<CompilerResponse, String> {
    let bundle = parse_bundle(input).map_err(|error| error.to_string())?;
    let normalization = normalize_bundle(&bundle);
    let has_errors = normalization.has_errors();
    let diagnostics = normalization.diagnostics;
    Ok(CompilerResponse {
        code: None,
        diagnostic_groups: group_diagnostics(&bundle, &diagnostics),
        diagnostics,
        source_map: None,
        error: has_errors.then(|| "normalization failed".to_owned()),
    })
}

pub fn compile_source(input: &str) -> Result<CompilerResponse, String> {
    let bundle = parse_bundle(input).map_err(|error| error.to_string())?;
    let normalization = normalize_bundle(&bundle);
    if normalization.has_errors() {
        let diagnostics = normalization.diagnostics;
        return Ok(CompilerResponse {
            code: None,
            diagnostic_groups: group_diagnostics(&bundle, &diagnostics),
            diagnostics,
            source_map: None,
            error: Some("normalization failed".to_owned()),
        });
    }

    match figma_rust_codegen::generate(&normalization.document) {
        Ok(generated) => {
            let diagnostics = normalization.diagnostics;
            Ok(CompilerResponse {
                code: Some(generated.rust),
                diagnostic_groups: group_diagnostics(&bundle, &diagnostics),
                diagnostics,
                source_map: Some(generated.source_map),
                error: None,
            })
        }
        Err(error) => {
            let message = error.to_string();
            let mut diagnostics = normalization.diagnostics;
            diagnostics.push(codegen_diagnostic(&error));
            Ok(CompilerResponse {
                code: None,
                diagnostic_groups: group_diagnostics(&bundle, &diagnostics),
                diagnostics,
                source_map: None,
                error: Some(message),
            })
        }
    }
}

fn group_diagnostics(
    bundle: &figma_rust_core::raw::ExtractionBundle,
    diagnostics: &[Diagnostic],
) -> Vec<DiagnosticGroup> {
    let root_owners = selected_root_owners(bundle);
    let mut grouped = BTreeMap::<DiagnosticGroupKey, (usize, BTreeSet<DiagnosticSample>)>::new();
    for diagnostic in diagnostics {
        let selected_root_id = diagnostic
            .node_id
            .as_ref()
            .and_then(|node_id| root_owners.get(node_id))
            .cloned()
            .flatten();
        let key = DiagnosticGroupKey {
            severity_rank: severity_rank(diagnostic.severity),
            code: diagnostic.code.clone(),
            property_path: diagnostic.property_path.clone(),
            selected_root_id,
        };
        let (count, samples) = grouped.entry(key).or_default();
        *count += 1;
        samples.insert(DiagnosticSample {
            node_id: diagnostic.node_id.clone(),
            message: diagnostic.message.clone(),
            help: diagnostic.help.clone(),
        });
        if samples.len() > DIAGNOSTIC_SAMPLE_LIMIT {
            samples.pop_last();
        }
    }
    grouped
        .into_iter()
        .map(|(key, (count, samples))| DiagnosticGroup {
            severity: severity_from_rank(key.severity_rank),
            code: key.code,
            property_path: key.property_path,
            selected_root_id: key.selected_root_id,
            count,
            samples: samples.into_iter().collect(),
        })
        .collect()
}

fn selected_root_owners(
    bundle: &figma_rust_core::raw::ExtractionBundle,
) -> BTreeMap<String, Option<String>> {
    let mut owners = BTreeMap::new();
    for root in &bundle.roots {
        collect_root_owners(root, &root.id, &mut owners);
    }
    owners
}

fn collect_root_owners(
    node: &figma_rust_core::raw::RawNode,
    root_id: &str,
    owners: &mut BTreeMap<String, Option<String>>,
) {
    owners
        .entry(node.id.clone())
        .and_modify(|owner| {
            if owner.as_deref() != Some(root_id) {
                *owner = None;
            }
        })
        .or_insert_with(|| Some(root_id.to_owned()));
    for child in &node.children {
        collect_root_owners(child, root_id, owners);
    }
}

const fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

const fn severity_from_rank(rank: u8) -> Severity {
    match rank {
        0 => Severity::Error,
        1 => Severity::Warning,
        _ => Severity::Info,
    }
}

fn append_diagnostics(diagnostics: &[Diagnostic], output: &mut String) {
    for diagnostic in diagnostics {
        output.push_str(&format_diagnostic(diagnostic));
        output.push('\n');
    }
}

fn append_diagnostic_groups(groups: &[DiagnosticGroup], output: &mut String) {
    for group in groups {
        write!(
            output,
            "[{:?}] {} count={}",
            group.severity, group.code, group.count
        )
        .expect("writing diagnostic group to String cannot fail");
        if let Some(root_id) = &group.selected_root_id {
            output.push_str(" root=");
            output.push_str(root_id);
        }
        if let Some(property) = &group.property_path {
            output.push_str(" property=");
            output.push_str(property);
        }
        output.push('\n');
        for sample in &group.samples {
            output.push_str("  sample");
            if let Some(node_id) = &sample.node_id {
                output.push_str(" node=");
                output.push_str(node_id);
            }
            output.push_str(": ");
            output.push_str(&sample.message);
            output.push('\n');
        }
    }
}

pub fn pretty_json<T: Serialize>(value: &T) -> Result<String, CliError> {
    let mut output = serde_json::to_string_pretty(value).map_err(|source| CliError::Serialize {
        label: "JSON output",
        source,
    })?;
    output.push('\n');
    Ok(output)
}

#[must_use]
pub fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let mut location = String::new();
    if let Some(node_id) = &diagnostic.node_id {
        location.push_str(" node=");
        location.push_str(node_id);
    }
    if let Some(property) = &diagnostic.property_path {
        location.push_str(" property=");
        location.push_str(property);
    }
    format!(
        "[{:?}] {}{}: {}",
        diagnostic.severity, diagnostic.code, location, diagnostic.message
    )
}

fn read_bundle(path: &Path) -> Result<figma_rust_core::raw::ExtractionBundle, CliError> {
    let path_text = path.display().to_string();
    let input = fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path_text.clone(),
        source,
    })?;
    parse_bundle(&input).map_err(|error| CliError::Parse {
        path: path_text,
        message: error.to_string(),
    })
}

fn codegen_diagnostic(error: &CodegenError) -> Diagnostic {
    let (code, node_id, property_path) = match error {
        CodegenError::Unsupported { location, .. } => (
            "FR-CODEGEN-001",
            Some(location.node_id.clone()),
            Some(location.property.clone()),
        ),
        CodegenError::InvalidValue { location, .. } => (
            "FR-CODEGEN-002",
            Some(location.node_id.clone()),
            Some(location.property.clone()),
        ),
        CodegenError::DuplicateSourceId { node_id } => (
            "FR-CODEGEN-003",
            Some(node_id.clone()),
            Some("source_id".to_owned()),
        ),
        CodegenError::InvalidRust { .. } => ("FR-CODEGEN-004", None, None),
        CodegenError::MissingSourceSpan { .. } => ("FR-CODEGEN-005", None, None),
    };
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message: error.to_string(),
        node_id,
        property_path,
        help: None,
    }
}

fn write_outputs(
    output_directory: &Path,
    normalization: &NormalizationOutput,
    generated: &GeneratedOutput,
) -> Result<(), CliError> {
    let outputs = output_artifacts(normalization, generated)?;

    let created_directory = ensure_output_directory(output_directory)?;
    let result = with_artifact_lock(output_directory, || {
        recover_generation(output_directory)?;
        commit_artifact_set_locked(output_directory, &outputs)?;
        cleanup_root_directories_except(
            &output_directory.join(ROOT_OUTPUTS_DIRECTORY),
            &BTreeSet::new(),
        )
    });
    finish_output_directory(result, output_directory, created_directory)
}

fn commit_artifact_set_locked(
    output_directory: &Path,
    outputs: &[OutputArtifact],
) -> Result<(), CliError> {
    let prepared = crate::generation::prepare(output_directory, outputs)
        .map_err(|error| transaction_error(error, &[]))?;
    if let Err(error) = project_outputs(output_directory, outputs) {
        let recovery_error = recover_generation(output_directory).err();
        return Err(match recovery_error {
            Some(recovery_error) => {
                transaction_error(error.to_string(), &[recovery_error.to_string()])
            }
            None => error,
        });
    }
    crate::generation::commit(output_directory, &prepared)
        .map_err(|error| transaction_error(error, &[]))
}

fn recover_generation(output_directory: &Path) -> Result<(), CliError> {
    crate::generation::recover(output_directory, |outputs, owned_names| {
        project_outputs_with_owned(output_directory, outputs, owned_names.clone())
            .map_err(|error| error.to_string())
    })
    .map_err(|error| transaction_error(error, &[]))
}

fn project_outputs(output_directory: &Path, outputs: &[OutputArtifact]) -> Result<(), CliError> {
    let paths = prepare_publish(output_directory, outputs)?;
    stage_and_publish(&paths, outputs)
}

fn project_outputs_with_owned(
    output_directory: &Path,
    outputs: &[OutputArtifact],
    owned_names: BTreeSet<String>,
) -> Result<(), CliError> {
    cleanup_projection_work_files(output_directory, &owned_names)?;
    let paths = prepare_publish_with_owned(output_directory, outputs, owned_names)?;
    stage_and_publish(&paths, outputs)
}

fn cleanup_projection_work_files(
    output_directory: &Path,
    owned_names: &BTreeSet<String>,
) -> Result<(), CliError> {
    let mut cleanup_errors = Vec::new();
    for name in owned_names {
        validate_artifact_name(name)?;
        for suffix in ["tmp", "backup"] {
            let path = work_path(output_directory, name, suffix);
            if let Err(error) = remove_regular_if_exists(&path) {
                cleanup_errors.push(error);
            }
        }
    }
    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(transaction_error(
            "cleaning interrupted artifact projection",
            &cleanup_errors,
        ))
    }
}

fn stage_and_publish(paths: &[ArtifactPaths], outputs: &[OutputArtifact]) -> Result<(), CliError> {
    if let Err(error) = stage_outputs(paths, outputs) {
        let cleanup_errors = cleanup_temps(paths);
        return Err(transaction_error(error, &cleanup_errors));
    }
    publish_outputs(paths)
}

fn output_artifacts(
    normalization: &NormalizationOutput,
    generated: &GeneratedOutput,
) -> Result<Vec<OutputArtifact>, CliError> {
    let mut document_without_payloads = normalization.document.clone();
    for asset in &mut document_without_payloads.assets {
        asset.payload_base64 = None;
    }
    let mut manifest = AssetManifest {
        schema_version: 1,
        assets: Vec::with_capacity(normalization.document.assets.len()),
    };
    let mut outputs = vec![
        OutputArtifact {
            name: GENERATED_RUST.to_owned(),
            content: generated.rust.as_bytes().to_vec(),
            cache_key: None,
        },
        OutputArtifact {
            name: SOURCE_MAP_JSON.to_owned(),
            content: serialize_json("source map", &generated.source_map)?.into_bytes(),
            cache_key: None,
        },
        OutputArtifact {
            name: IR_JSON.to_owned(),
            content: serialize_json("normalized IR", &document_without_payloads)?.into_bytes(),
            cache_key: None,
        },
        OutputArtifact {
            name: DIAGNOSTICS_JSON.to_owned(),
            content: serialize_json("diagnostics", &normalization.diagnostics)?.into_bytes(),
            cache_key: None,
        },
    ];
    for asset in &normalization.document.assets {
        let file_name = asset
            .payload_base64
            .as_ref()
            .and_then(|_| asset.file_name());
        let mut cache_key = None;
        if let (Some(payload), Some(file_name)) = (&asset.payload_base64, file_name.clone()) {
            let content = BASE64
                .decode(payload)
                .map_err(|source| CliError::DecodeAsset {
                    id: asset.id.clone(),
                    source,
                })?;
            let key = crate::generation::asset_cache_key(
                &content,
                &asset.media_type,
                &asset.export_settings,
            );
            outputs.push(OutputArtifact {
                name: file_name,
                cache_key: Some(key.clone()),
                content,
            });
            cache_key = Some(key);
        }
        manifest.assets.push(AssetManifestEntry {
            id: asset.id.clone(),
            source_node_id: asset.source_node_id.clone(),
            media_type: asset.media_type.clone(),
            content_hash: asset.content_hash.clone(),
            export_settings: asset.export_settings.clone(),
            file_name: file_name.clone(),
            cache_key,
            payload_available: asset.payload_base64.is_some(),
        });
    }
    outputs.push(OutputArtifact {
        name: ASSET_MANIFEST_JSON.to_owned(),
        content: serialize_json("asset manifest", &manifest)?.into_bytes(),
        cache_key: None,
    });
    outputs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(outputs)
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetManifest {
    schema_version: u32,
    assets: Vec<AssetManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetManifestEntry {
    id: String,
    source_node_id: String,
    media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(default)]
    export_settings: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
    #[serde(default)]
    payload_available: bool,
}

pub(crate) struct OutputArtifact {
    pub(crate) name: String,
    pub(crate) content: Vec<u8>,
    pub(crate) cache_key: Option<String>,
}

struct ArtifactPaths {
    name: String,
    final_path: PathBuf,
    temp_path: PathBuf,
    backup_path: PathBuf,
    had_previous: bool,
    publish: bool,
}

fn prepare_publish(
    output_directory: &Path,
    outputs: &[OutputArtifact],
) -> Result<Vec<ArtifactPaths>, CliError> {
    let mut owned_names = existing_owned_artifact_names(output_directory)?;
    owned_names.extend(outputs.iter().map(|output| output.name.clone()));
    prepare_publish_with_owned(output_directory, outputs, owned_names)
}

fn prepare_publish_with_owned(
    output_directory: &Path,
    outputs: &[OutputArtifact],
    mut owned_names: BTreeSet<String>,
) -> Result<Vec<ArtifactPaths>, CliError> {
    let output_names = outputs
        .iter()
        .map(|output| output.name.as_str())
        .collect::<BTreeSet<_>>();
    owned_names.extend(outputs.iter().map(|output| output.name.clone()));
    owned_names
        .into_iter()
        .map(|name| {
            validate_artifact_name(&name)?;
            let final_path = output_directory.join(&name);
            let had_previous = validate_regular_or_missing(&final_path, "artifact destination")?;
            let temp_path = work_path(output_directory, &name, "tmp");
            let backup_path = work_path(output_directory, &name, "backup");
            let publish = output_names.contains(name.as_str());
            if publish {
                validate_missing(&temp_path, "temporary artifact path")?;
            }
            validate_missing(&backup_path, "artifact backup path")?;
            Ok(ArtifactPaths {
                name,
                final_path,
                temp_path,
                backup_path,
                had_previous,
                publish,
            })
        })
        .collect()
}

fn stage_outputs(paths: &[ArtifactPaths], outputs: &[OutputArtifact]) -> Result<(), String> {
    for output in outputs {
        let path = paths
            .iter()
            .find(|path| path.name == output.name)
            .ok_or_else(|| format!("missing publication path for {}", output.name))?;
        crate::generation::stage_projection_artifact(&path.temp_path, output)?;
    }
    Ok(())
}

fn publish_outputs(paths: &[ArtifactPaths]) -> Result<(), CliError> {
    publish_outputs_with(paths, |_, source, destination| {
        fs::rename(source, destination)
    })
}

fn publish_outputs_with(
    paths: &[ArtifactPaths],
    mut publish: impl FnMut(usize, &Path, &Path) -> std::io::Result<()>,
) -> Result<(), CliError> {
    for (backed_up, path) in paths.iter().filter(|path| path.had_previous).enumerate() {
        if let Err(error) = fs::rename(&path.final_path, &path.backup_path) {
            let operation = format!("backing up {}: {error}", path.final_path.display());
            let cleanup_errors = rollback_before_publish(paths, backed_up);
            return Err(transaction_error(operation, &cleanup_errors));
        }
    }

    for (published, path) in paths.iter().filter(|path| path.publish).enumerate() {
        if let Err(error) = publish(published, &path.temp_path, &path.final_path) {
            let operation = format!("publishing {}: {error}", path.final_path.display());
            let cleanup_errors = rollback_after_publish(paths, published);
            return Err(transaction_error(operation, &cleanup_errors));
        }
    }

    let cleanup_errors = paths
        .iter()
        .filter(|path| path.had_previous)
        .filter_map(|path| remove_regular_if_exists(&path.backup_path).err())
        .collect::<Vec<_>>();
    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(transaction_error(
            "cleaning committed artifact backups",
            &cleanup_errors,
        ))
    }
}

fn rollback_before_publish(paths: &[ArtifactPaths], backed_up: usize) -> Vec<String> {
    let mut errors = Vec::new();
    let previous = paths
        .iter()
        .filter(|path| path.had_previous)
        .collect::<Vec<_>>();
    for path in previous[..backed_up].iter().rev() {
        if let Err(error) = fs::rename(&path.backup_path, &path.final_path) {
            errors.push(format!(
                "restoring {} from {}: {error}",
                path.final_path.display(),
                path.backup_path.display()
            ));
        }
    }
    errors.extend(cleanup_temps(paths));
    errors
}

fn rollback_after_publish(paths: &[ArtifactPaths], published: usize) -> Vec<String> {
    let mut errors = Vec::new();
    for path in paths.iter().filter(|path| path.publish).take(published) {
        if let Err(error) = remove_regular_if_exists(&path.final_path) {
            errors.push(error);
        }
    }
    for path in paths.iter().filter(|path| path.had_previous) {
        if let Err(error) = fs::rename(&path.backup_path, &path.final_path) {
            errors.push(format!(
                "restoring {} from {}: {error}",
                path.final_path.display(),
                path.backup_path.display()
            ));
        }
    }
    errors.extend(cleanup_temps(paths));
    errors
}

fn invalidate_outputs(output_directory: &Path) -> Result<(), CliError> {
    if !validate_existing_output_directory(output_directory)? {
        return Ok(());
    }
    with_artifact_lock(output_directory, || {
        invalidate_outputs_locked(output_directory)?;
        cleanup_root_directories_except(
            &output_directory.join(ROOT_OUTPUTS_DIRECTORY),
            &BTreeSet::new(),
        )
    })
}

fn invalidate_outputs_locked(output_directory: &Path) -> Result<(), CliError> {
    let paths = existing_owned_artifact_names(output_directory)?
        .into_iter()
        .map(|name| {
            validate_artifact_name(&name)?;
            let final_path = output_directory.join(&name);
            let had_previous = validate_regular_or_missing(&final_path, "artifact destination")?;
            let quarantine_path = work_path(output_directory, &name, "invalid");
            validate_missing(&quarantine_path, "artifact invalidation path")?;
            Ok((final_path, quarantine_path, had_previous))
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    for (moved, (final_path, quarantine_path, _)) in paths
        .iter()
        .filter(|(_, _, had_previous)| *had_previous)
        .enumerate()
    {
        if let Err(error) = fs::rename(final_path, quarantine_path) {
            let mut rollback_errors = Vec::new();
            let previous = paths
                .iter()
                .filter(|(_, _, had_previous)| *had_previous)
                .collect::<Vec<_>>();
            for (final_path, quarantine_path, _) in previous[..moved].iter().rev() {
                if let Err(rollback_error) = fs::rename(quarantine_path, final_path) {
                    rollback_errors.push(format!(
                        "restoring {} from {}: {rollback_error}",
                        final_path.display(),
                        quarantine_path.display()
                    ));
                }
            }
            return Err(transaction_error(
                format!("invalidating {}: {error}", final_path.display()),
                &rollback_errors,
            ));
        }
    }

    let cleanup_errors = paths
        .iter()
        .filter(|(_, _, had_previous)| *had_previous)
        .filter_map(|(_, quarantine_path, _)| remove_regular_if_exists(quarantine_path).err())
        .collect::<Vec<_>>();
    let mut cleanup_errors = cleanup_errors;
    cleanup_errors.extend(crate::generation::invalidate(output_directory));
    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(transaction_error(
            "cleaning invalidated artifacts",
            &cleanup_errors,
        ))
    }
}

fn serialize_json<T: Serialize>(label: &'static str, value: &T) -> Result<String, CliError> {
    let mut output = serde_json::to_string_pretty(value)
        .map_err(|source| CliError::Serialize { label, source })?;
    output.push('\n');
    Ok(output)
}

fn ensure_output_directory(output_directory: &Path) -> Result<bool, CliError> {
    match fs::symlink_metadata(output_directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CliError::UnsafeOutput {
            path: output_directory.display().to_string(),
            reason: "output directory is a symbolic link".to_owned(),
        }),
        Ok(metadata) if !metadata.is_dir() => Err(CliError::UnsafeOutput {
            path: output_directory.display().to_string(),
            reason: "output path is not a directory".to_owned(),
        }),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(output_directory).map_err(|source| CliError::CreateOutput {
                path: output_directory.display().to_string(),
                source,
            })?;
            Ok(true)
        }
        Err(source) => Err(CliError::CreateOutput {
            path: output_directory.display().to_string(),
            source,
        }),
    }
}

fn validate_existing_output_directory(output_directory: &Path) -> Result<bool, CliError> {
    match fs::symlink_metadata(output_directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CliError::UnsafeOutput {
            path: output_directory.display().to_string(),
            reason: "output directory is a symbolic link".to_owned(),
        }),
        Ok(metadata) if !metadata.is_dir() => Err(CliError::UnsafeOutput {
            path: output_directory.display().to_string(),
            reason: "output path is not a directory".to_owned(),
        }),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(CliError::CreateOutput {
            path: output_directory.display().to_string(),
            source,
        }),
    }
}

fn validate_regular_or_missing(path: &Path, label: &str) -> Result<bool, CliError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(CliError::UnsafeOutput {
            path: path.display().to_string(),
            reason: format!("{label} is not a regular file"),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(CliError::Write {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn validate_missing(path: &Path, label: &str) -> Result<(), CliError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(CliError::UnsafeOutput {
            path: path.display().to_string(),
            reason: format!("{label} already exists"),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CliError::Write {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn existing_owned_artifact_names(output_directory: &Path) -> Result<BTreeSet<String>, CliError> {
    let mut names = ARTIFACT_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    names.extend(
        crate::generation::owned_artifact_names(output_directory)
            .map_err(|error| transaction_error(error, &[]))?,
    );
    let manifest_path = output_directory.join(ASSET_MANIFEST_JSON);
    let manifest_exists = validate_regular_or_missing(&manifest_path, "asset manifest")?;
    if !manifest_exists {
        return Ok(names);
    }
    let input = fs::read_to_string(&manifest_path).map_err(|source| CliError::Read {
        path: manifest_path.display().to_string(),
        source,
    })?;
    let manifest =
        serde_json::from_str::<AssetManifest>(&input).map_err(|error| CliError::AssetManifest {
            path: manifest_path.display().to_string(),
            message: error.to_string(),
        })?;
    if manifest.schema_version != 1 {
        return Err(CliError::AssetManifest {
            path: manifest_path.display().to_string(),
            message: format!(
                "unsupported schema version {}; expected 1",
                manifest.schema_version
            ),
        });
    }
    for file_name in manifest
        .assets
        .into_iter()
        .filter_map(|asset| asset.file_name)
    {
        validate_artifact_name(&file_name)?;
        names.insert(file_name);
    }
    Ok(names)
}

fn validate_artifact_name(name: &str) -> Result<(), CliError> {
    let mut components = Path::new(name).components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.starts_with('.');
    if valid {
        Ok(())
    } else {
        Err(CliError::UnsafeOutput {
            path: name.to_owned(),
            reason: "artifact name must be one non-hidden file name".to_owned(),
        })
    }
}

fn work_path(output_directory: &Path, name: &str, suffix: &str) -> PathBuf {
    output_directory.join(format!(".figma-rust-{name}.{suffix}"))
}

fn remove_regular_if_exists(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|error| format!("removing {}: {error}", path.display()))
        }
        Ok(_) => Err(format!(
            "refusing to remove non-regular path {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspecting {}: {error}", path.display())),
    }
}

fn cleanup_temps(paths: &[ArtifactPaths]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| path.publish)
        .filter_map(|path| remove_regular_if_exists(&path.temp_path).err())
        .collect()
}

struct ArtifactLock {
    path: Option<PathBuf>,
    file: Option<fs::File>,
}

impl ArtifactLock {
    fn acquire(output_directory: &Path) -> Result<Self, CliError> {
        let path = output_directory.join(ARTIFACT_LOCK);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                transaction_error(
                    format!("acquiring artifact lock {}: {error}", path.display()),
                    &[],
                )
            })?;
        let lock_content = format!("pid={}\n", std::process::id());
        if let Err(error) = file
            .write_all(lock_content.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let cleanup_errors = remove_regular_if_exists(&path)
                .err()
                .into_iter()
                .collect::<Vec<_>>();
            return Err(transaction_error(
                format!("initializing artifact lock {}: {error}", path.display()),
                &cleanup_errors,
            ));
        }
        Ok(Self {
            path: Some(path),
            file: Some(file),
        })
    }

    fn release(mut self) -> Result<(), String> {
        self.file.take();
        let path = self
            .path
            .take()
            .ok_or_else(|| "artifact lock ownership was already released".to_owned())?;
        remove_regular_if_exists(&path)
    }
}

impl Drop for ArtifactLock {
    fn drop(&mut self) {
        self.file.take();
        if let Some(path) = self.path.take() {
            let _ = remove_regular_if_exists(&path);
        }
    }
}

fn with_artifact_lock<T>(
    output_directory: &Path,
    operation: impl FnOnce() -> Result<T, CliError>,
) -> Result<T, CliError> {
    let lock = ArtifactLock::acquire(output_directory)?;
    let result = operation();
    let release_error = lock.release().err();
    match (result, release_error) {
        (Ok(value), None) => Ok(value),
        (Ok(_), Some(error)) => Err(transaction_error("releasing artifact lock", &[error])),
        (Err(error), None) => Err(error),
        (Err(error), Some(release_error)) => {
            Err(transaction_error(error.to_string(), &[release_error]))
        }
    }
}

fn finish_output_directory<T>(
    result: Result<T, CliError>,
    output_directory: &Path,
    created_directory: bool,
) -> Result<T, CliError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let mut cleanup_errors = Vec::new();
            cleanup_new_directory(output_directory, created_directory, &mut cleanup_errors);
            if cleanup_errors.is_empty() {
                Err(error)
            } else {
                Err(transaction_error(error.to_string(), &cleanup_errors))
            }
        }
    }
}

fn cleanup_new_directory(
    output_directory: &Path,
    created_directory: bool,
    errors: &mut Vec<String>,
) {
    if created_directory
        && let Err(error) = fs::remove_dir(output_directory)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        errors.push(format!(
            "removing newly-created output directory {}: {error}",
            output_directory.display()
        ));
    }
}

fn transaction_error(operation: impl Into<String>, cleanup_errors: &[String]) -> CliError {
    let rollback = if cleanup_errors.is_empty() {
        String::new()
    } else {
        format!("; rollback incomplete: {}", cleanup_errors.join("; "))
    };
    CliError::ArtifactTransaction {
        operation: operation.into(),
        rollback,
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, ffi::OsString, fs, path::PathBuf};

    use figma_rust_core::normalize_bundle;

    use super::{
        ARTIFACT_NAMES, ASSET_MANIFEST_JSON, ArtifactLock, GENERATED_RUST, ROOT_STATUS_JSON,
        RootCompileStatus, compile_file, compile_file_root_scoped, compile_source, inspect_bundle,
        lint_source, output_artifacts, parse_bundle, prepare_publish, publish_outputs_with,
        stage_outputs, work_path,
    };

    const BASIC: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.raw.json");
    const BASIC_V1: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.v1.raw.json");
    const DIAGNOSTIC_GROUPS: &str =
        include_str!("../../../fixtures/diagnostic-groups/extraction.json");

    #[test]
    fn inspect_is_deterministic_and_counts_preorder_tree() -> Result<(), Box<dyn std::error::Error>>
    {
        let bundle = parse_bundle(BASIC)?;
        let first = serde_json::to_string_pretty(&inspect_bundle(&bundle)?)?;
        let second = serde_json::to_string_pretty(&inspect_bundle(&bundle)?)?;
        assert_eq!(first, second);
        assert!(first.contains("\"schema_compatibility\": \"CURRENT\""));
        assert!(first.contains("\"algorithm\": \"SHA-256\""));
        assert!(first.contains("\"value\":"));
        assert!(first.contains("\"node_count\": 1"));
        assert!(first.contains("\"id\": \"1:1\""));
        Ok(())
    }

    #[test]
    fn diagnostic_groups_are_bounded_blocker_first_and_root_scoped()
    -> Result<(), Box<dyn std::error::Error>> {
        let bundle = parse_bundle(DIAGNOSTIC_GROUPS)?;
        let first = inspect_bundle(&bundle)?;
        let second = inspect_bundle(&bundle)?;
        assert_eq!(
            serde_json::to_vec_pretty(&first)?,
            serde_json::to_vec_pretty(&second)?
        );
        assert_eq!(first.diagnostics.len(), 12);
        assert!(first.diagnostic_groups.windows(2).all(|groups| {
            super::severity_rank(groups[0].severity) <= super::severity_rank(groups[1].severity)
        }));
        let repeated = first
            .diagnostic_groups
            .iter()
            .find(|group| {
                group.code == "FR-EXTRACT-LOSS-001"
                    && group.selected_root_id.as_deref() == Some("9:a")
            })
            .ok_or("missing root-A extraction-loss group")?;
        assert_eq!(repeated.count, 4);
        assert_eq!(repeated.samples.len(), 3);
        assert_eq!(repeated.samples[0].node_id.as_deref(), Some("9:a:1"));
        assert_eq!(repeated.samples[2].node_id.as_deref(), Some("9:a:3"));
        let grouped_text = first.text(true);
        assert!(!grouped_text.contains("Repeated extraction loss A4"));
        let blocker = grouped_text
            .find("[Error] FR-FATAL-001")
            .ok_or("missing grouped blocker")?;
        let tree = grouped_text
            .find("- [FRAME] \"Diagnostic root A\"")
            .ok_or("missing inspect tree")?;
        assert!(blocker < tree);
        assert!(first.text(false).contains("Repeated extraction loss A4"));

        let response = lint_source(DIAGNOSTIC_GROUPS)?;
        assert_eq!(response.diagnostic_groups, first.diagnostic_groups);
        assert_eq!(response.diagnostics, first.diagnostics);
        Ok(())
    }

    #[test]
    fn lint_and_compile_http_contracts_expose_expected_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let lint = lint_source(BASIC)?;
        assert!(lint.error.is_none());
        assert!(lint.code.is_none());

        let compile = compile_source(BASIC)?;
        assert!(compile.error.is_none());
        assert!(
            compile
                .code
                .as_deref()
                .is_some_and(|code| code.contains("generated_view"))
        );
        assert!(compile.source_map.is_some());
        Ok(())
    }

    #[test]
    fn schema_one_source_returns_an_explicit_normalization_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = lint_source(BASIC_V1)?;
        assert_eq!(response.error.as_deref(), Some("normalization failed"));
        assert!(response.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == figma_rust_core::diagnostic::codes::SCHEMA_VERSION
        }));
        Ok(())
    }

    #[test]
    fn codegen_error_is_node_and_property_scoped() -> Result<(), Box<dyn std::error::Error>> {
        let grid = BASIC.replace("\"mode\": \"HORIZONTAL\"", "\"mode\": \"GRID\"");
        let response = compile_source(&grid)?;
        assert!(response.code.is_none());
        assert!(response.error.is_some());
        let diagnostic = response
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "FR-CODEGEN-001")
            .ok_or("missing codegen diagnostic")?;
        assert_eq!(diagnostic.node_id.as_deref(), Some("1:1"));
        assert_eq!(diagnostic.property_path.as_deref(), Some("layout"));
        Ok(())
    }

    #[test]
    fn compile_writes_only_fixed_newline_terminated_artifacts()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("compile-success");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;

        let result = compile_file(&raw, &out)?;
        assert!(result.error.is_none());
        let mut names = fs::read_dir(&out)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        names.sort();
        assert_eq!(
            names,
            [
                ".figma-rust-asset-cache",
                ".figma-rust-generations",
                "asset-manifest.json",
                "current-generation.json",
                "diagnostics.json",
                "generated.rs",
                "ir.json",
                "source-map.json"
            ]
            .map(OsString::from)
        );
        for name in ARTIFACT_NAMES {
            assert!(fs::read(out.join(name))?.ends_with(b"\n"));
        }
        assert!(fs::read(out.join(crate::generation::CURRENT_GENERATION))?.ends_with(b"\n"));
        assert!(out.join(crate::generation::GENERATION_STORE).is_dir());
        assert!(out.join(crate::generation::ASSET_CACHE).is_dir());
        let first_pointer = fs::read(out.join(crate::generation::CURRENT_GENERATION))?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        assert_eq!(
            fs::read(out.join(crate::generation::CURRENT_GENERATION))?,
            first_pointer
        );
        assert_eq!(
            fs::read_dir(out.join(crate::generation::GENERATION_STORE))?.count(),
            1
        );
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn root_scoped_compile_isolates_success_and_retains_each_failure()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("root-scoped");
        fs::create_dir_all(&base)?;
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/root-scoped/extraction.json");
        let strict_out = base.join("strict");
        let strict = compile_file(&fixture, &strict_out)?;
        assert_eq!(strict.error.as_deref(), Some("normalization failed"));
        assert!(!strict_out.exists());

        let out = base.join("scoped");
        let first = compile_file_root_scoped(&fixture, &out)?;
        assert_eq!(first.summary.total, 3);
        assert_eq!(first.summary.succeeded, 1);
        assert_eq!(first.summary.failed, 2);
        assert_eq!(first.roots[0].status, RootCompileStatus::Success);
        assert_eq!(
            first.roots[1].status,
            RootCompileStatus::NormalizationFailed
        );
        assert_eq!(
            first.roots[2].status,
            RootCompileStatus::UnsupportedRuntimeRoute
        );
        assert!(first.roots[1].diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "FR-LAYOUT-004"
                && diagnostic.node_id.as_deref() == Some("28:invalid")
                && diagnostic.property_path.as_deref() == Some("layout.child_counter_alignment")
        }));
        assert!(first.roots[2].diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "FR-ASSET-001"
                && diagnostic.node_id.as_deref() == Some("28:runtime")
                && diagnostic.property_path.as_deref() == Some("asset_decision.route")
        }));
        let artifact_directory = first.roots[0]
            .artifact_directory
            .as_deref()
            .ok_or("successful root has no artifact directory")?;
        let root_output = out.join(artifact_directory);
        assert!(root_output.join(GENERATED_RUST).is_file());
        assert!(root_output.join(ASSET_MANIFEST_JSON).is_file());
        assert_eq!(
            fs::read_dir(out.join(super::ROOT_OUTPUTS_DIRECTORY))?.count(),
            1
        );
        let first_status = fs::read(out.join(ROOT_STATUS_JSON))?;
        let first_pointer = fs::read(out.join(crate::generation::CURRENT_GENERATION))?;

        let second = compile_file_root_scoped(&fixture, &out)?;
        assert_eq!(second.summary.succeeded, 1);
        assert_eq!(fs::read(out.join(ROOT_STATUS_JSON))?, first_status);
        assert_eq!(
            fs::read(out.join(crate::generation::CURRENT_GENERATION))?,
            first_pointer
        );

        let strict_failure = compile_file(&fixture, &out)?;
        assert_eq!(
            strict_failure.error.as_deref(),
            Some("normalization failed")
        );
        assert!(!out.join(ROOT_STATUS_JSON).exists());
        assert!(!out.join(super::ROOT_OUTPUTS_DIRECTORY).exists());

        let strict_raw = base.join("basic.json");
        fs::write(&strict_raw, BASIC)?;
        assert!(compile_file(&strict_raw, &out)?.error.is_none());
        assert!(out.join(GENERATED_RUST).is_file());
        assert!(!out.join(ROOT_STATUS_JSON).exists());
        assert!(!out.join(super::ROOT_OUTPUTS_DIRECTORY).exists());

        let third = compile_file_root_scoped(&fixture, &out)?;
        assert_eq!(third.summary.succeeded, 1);
        assert_eq!(fs::read(out.join(ROOT_STATUS_JSON))?, first_status);
        assert!(!out.join(GENERATED_RUST).exists());

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn tampered_generation_fails_closed_without_replacing_the_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("tampered-generation");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        let projection = artifact_contents(&out)?;
        let pointer = serde_json::from_slice::<serde_json::Value>(&fs::read(
            out.join(crate::generation::CURRENT_GENERATION),
        )?)?;
        let generation_id = pointer["generation_id"]
            .as_str()
            .ok_or("generation pointer has no ID")?;
        fs::write(
            out.join(crate::generation::GENERATION_STORE)
                .join(generation_id)
                .join(GENERATED_RUST),
            "tampered\n",
        )?;

        let error = compile_file(&raw, &out).expect_err("tampered generation must fail");
        assert!(error.to_string().contains("hash/size validation"));
        assert_eq!(artifact_contents(&out)?, projection);

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn compile_publishes_fallback_payload_and_deterministic_manifest()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("asset-manifest");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        let mut bundle = serde_json::from_str::<serde_json::Value>(BASIC)?;
        bundle["roots"][0]["kind"] = serde_json::json!("VECTOR");
        bundle["assets"] = serde_json::json!([{
            "id": "node:1:1:svg",
            "source_node_id": "1:1",
            "media_type": "image/svg+xml",
            "export_settings": {"format": "SVG"},
            "payload_base64": "PHN2Zy8+"
        }]);
        fs::write(&raw, serde_json::to_vec_pretty(&bundle)?)?;

        let result = compile_file(&raw, &out)?;
        assert!(result.error.is_none(), "{:?}", result.diagnostics);
        let asset_name = "asset-6e6f64653a313a313a737667.svg";
        assert_eq!(fs::read(out.join(asset_name))?, b"<svg/>");
        let manifest = fs::read_to_string(out.join("asset-manifest.json"))?;
        assert!(manifest.ends_with('\n'));
        assert!(manifest.contains(&format!("\"file_name\": \"{asset_name}\"")));
        assert!(manifest.contains("\"cache_key\":"));
        assert!(!manifest.contains("payload_base64"));

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn asset_cache_fixture_reuses_exact_blobs_collects_stale_content_and_is_deterministic()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("asset-cache-fixture");
        fs::create_dir_all(&base)?;
        let out = base.join("out");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/asset-cache/extraction.v1.json");
        let changed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/asset-cache/extraction.v2.json");

        assert!(compile_file(&fixture, &out)?.error.is_none());
        let first_pointer = fs::read(out.join(crate::generation::CURRENT_GENERATION))?;
        let first_cache = sorted_file_names(&out.join(crate::generation::ASSET_CACHE))?;
        assert_eq!(
            first_cache.len(),
            2,
            "three assets should use two exact blobs"
        );
        assert!(compile_file(&fixture, &out)?.error.is_none());
        assert_eq!(
            fs::read(out.join(crate::generation::CURRENT_GENERATION))?,
            first_pointer
        );
        assert_eq!(
            sorted_file_names(&out.join(crate::generation::ASSET_CACHE))?,
            first_cache
        );

        assert!(compile_file(&changed, &out)?.error.is_none());
        let second_pointer = fs::read(out.join(crate::generation::CURRENT_GENERATION))?;
        let second_cache = sorted_file_names(&out.join(crate::generation::ASSET_CACHE))?;
        assert_ne!(second_pointer, first_pointer);
        assert_eq!(second_cache.len(), 2);
        assert_eq!(
            first_cache
                .iter()
                .filter(|name| second_cache.contains(name))
                .count(),
            1,
            "only the shared exact blob should remain reachable"
        );
        assert!(compile_file(&changed, &out)?.error.is_none());
        assert_eq!(
            fs::read(out.join(crate::generation::CURRENT_GENERATION))?,
            second_pointer
        );
        assert_eq!(
            sorted_file_names(&out.join(crate::generation::ASSET_CACHE))?,
            second_cache
        );

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn tampered_asset_cache_fails_closed_without_mutating_the_flat_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("tampered-asset-cache");
        fs::create_dir_all(&base)?;
        let out = base.join("out");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/asset-cache/extraction.v1.json");
        assert!(compile_file(&fixture, &out)?.error.is_none());
        let projection = artifact_contents(&out)?;
        let projected_assets = flat_asset_contents(&out)?;
        let cache_entry = fs::read_dir(out.join(crate::generation::ASSET_CACHE))?
            .next()
            .ok_or("missing cache entry")??
            .path();
        fs::write(cache_entry, b"tampered")?;

        let error = compile_file(&fixture, &out).expect_err("tampered cache must fail");
        assert!(error.to_string().contains("hash/size validation"));
        assert_eq!(artifact_contents(&out)?, projection);
        assert_eq!(flat_asset_contents(&out)?, projected_assets);

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn next_compile_removes_assets_no_longer_listed_by_the_manifest()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("stale-asset");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        let mut bundle = serde_json::from_str::<serde_json::Value>(BASIC)?;
        bundle["roots"][0]["kind"] = serde_json::json!("VECTOR");
        bundle["assets"] = serde_json::json!([{
            "id": "node:1:1:svg",
            "source_node_id": "1:1",
            "media_type": "image/svg+xml",
            "export_settings": {"format": "SVG"},
            "payload_base64": "PHN2Zy8+"
        }]);
        fs::write(&raw, serde_json::to_vec_pretty(&bundle)?)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        let stale_asset = out.join("asset-6e6f64653a313a313a737667.svg");
        assert!(stale_asset.is_file());
        fs::write(out.join("keep.txt"), "unrelated")?;

        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        assert!(!stale_asset.exists());
        assert_eq!(fs::read_to_string(out.join("keep.txt"))?, "unrelated");
        let manifest = fs::read_to_string(out.join(ASSET_MANIFEST_JSON))?;
        assert!(manifest.contains("\"assets\": []"));

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn domain_failure_removes_stale_artifacts_but_preserves_unrelated_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("stale-domain-output");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        fs::write(out.join("keep.txt"), "unrelated")?;

        fs::write(
            &raw,
            BASIC.replace("\"schema_version\": 2", "\"schema_version\": 999"),
        )?;
        let result = compile_file(&raw, &out)?;
        assert_eq!(result.error.as_deref(), Some("normalization failed"));
        assert!(ARTIFACT_NAMES.iter().all(|name| !out.join(name).exists()));
        assert!(!out.join(crate::generation::CURRENT_GENERATION).exists());
        assert_eq!(fs::read_to_string(out.join("keep.txt"))?, "unrelated");

        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        fs::write(
            &raw,
            BASIC.replace("\"mode\": \"HORIZONTAL\"", "\"mode\": \"GRID\""),
        )?;
        let result = compile_file(&raw, &out)?;
        assert!(result.error.is_some());
        assert!(ARTIFACT_NAMES.iter().all(|name| !out.join(name).exists()));
        assert!(!out.join(crate::generation::CURRENT_GENERATION).exists());
        assert_eq!(fs::read_to_string(out.join("keep.txt"))?, "unrelated");

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn publish_preparation_failure_preserves_previous_artifact_set()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("publish-preparation-error");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        let previous = artifact_contents(&out)?;

        let blocking_temp = work_path(&out, GENERATED_RUST, "tmp");
        fs::create_dir(&blocking_temp)?;
        assert!(compile_file(&raw, &out).is_err());
        assert_eq!(artifact_contents(&out)?, previous);
        assert!(blocking_temp.is_dir());

        fs::remove_dir(blocking_temp)?;
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn publish_failure_rolls_back_previous_set_and_cleans_work_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("publish-rollback");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        let previous = artifact_contents(&out)?;

        let changed_bundle = parse_bundle(&BASIC.replace("\"r\": 0.125", "\"r\": 0.75"))?;
        let normalization = normalize_bundle(&changed_bundle);
        let generated = figma_rust_codegen::generate(&normalization.document)?;
        let outputs = output_artifacts(&normalization, &generated)?;
        let generated_output = outputs
            .iter()
            .find(|output| output.name == GENERATED_RUST)
            .ok_or("missing generated output")?;
        assert_ne!(
            generated_output.content,
            *previous
                .get(GENERATED_RUST)
                .ok_or("missing previous generated Rust")?
        );
        let lock = ArtifactLock::acquire(&out)?;
        let paths = prepare_publish(&out, &outputs)?;
        stage_outputs(&paths, &outputs).map_err(std::io::Error::other)?;
        let result = publish_outputs_with(&paths, |index, source, destination| {
            if index == 3 {
                Err(std::io::Error::other("injected publish failure"))
            } else {
                fs::rename(source, destination)
            }
        });
        assert!(result.is_err());
        assert_eq!(artifact_contents(&out)?, previous);
        lock.release().map_err(std::io::Error::other)?;
        assert_eq!(directory_names(&out)?, final_artifact_names());

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn restart_rolls_pending_generation_forward_from_a_mixed_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("projection-crash-recovery");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());

        let changed_bundle = parse_bundle(&BASIC.replace("\"r\": 0.125", "\"r\": 0.75"))?;
        let normalization = normalize_bundle(&changed_bundle);
        let generated = figma_rust_codegen::generate(&normalization.document)?;
        let outputs = output_artifacts(&normalization, &generated)?;
        let expected = outputs
            .iter()
            .map(|output| (output.name.clone(), output.content.clone()))
            .collect::<BTreeMap<_, _>>();
        let _prepared =
            crate::generation::prepare(&out, &outputs).map_err(std::io::Error::other)?;
        let paths = prepare_publish(&out, &outputs)?;
        stage_outputs(&paths, &outputs).map_err(std::io::Error::other)?;
        for path in paths.iter().filter(|path| path.had_previous) {
            fs::rename(&path.final_path, &path.backup_path)?;
        }
        for path in paths.iter().filter(|path| path.publish).take(2) {
            fs::rename(&path.temp_path, &path.final_path)?;
        }

        super::recover_generation(&out)?;
        assert_eq!(artifact_contents(&out)?, expected);
        assert_eq!(directory_names(&out)?, final_artifact_names());

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn artifact_lock_rejects_a_second_writer_without_touching_previous_set()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("single-writer-lock");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        let previous = artifact_contents(&out)?;

        let lock = ArtifactLock::acquire(&out)?;
        assert!(compile_file(&raw, &out).is_err());
        assert_eq!(artifact_contents(&out)?, previous);
        lock.release().map_err(std::io::Error::other)?;
        assert_eq!(directory_names(&out)?, final_artifact_names());

        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn normalization_failure_does_not_create_output_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("normalization-error");
        fs::create_dir_all(&base)?;
        let raw = base.join("raw.json");
        let out = base.join("out");
        fs::write(
            &raw,
            BASIC.replace("\"schema_version\": 2", "\"schema_version\": 999"),
        )?;

        let result = compile_file(&raw, &out)?;
        assert_eq!(result.error.as_deref(), Some("normalization failed"));
        assert!(!out.exists());
        fs::remove_dir_all(base)?;
        Ok(())
    }

    fn unique_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "figma-rust-cli-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    fn artifact_contents(
        output_directory: &std::path::Path,
    ) -> Result<BTreeMap<String, Vec<u8>>, std::io::Error> {
        ARTIFACT_NAMES
            .iter()
            .map(|name| {
                fs::read(output_directory.join(name)).map(|content| ((*name).to_owned(), content))
            })
            .collect()
    }

    fn directory_names(
        output_directory: &std::path::Path,
    ) -> Result<Vec<OsString>, std::io::Error> {
        let mut names = fs::read_dir(output_directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()?;
        names.sort();
        Ok(names)
    }

    fn sorted_file_names(directory: &std::path::Path) -> Result<Vec<String>, std::io::Error> {
        let mut names = fs::read_dir(directory)?
            .map(|entry| {
                entry.and_then(|entry| {
                    entry.file_name().into_string().map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "non-UTF-8 fixture file name",
                        )
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        names.sort();
        Ok(names)
    }

    fn flat_asset_contents(
        output_directory: &std::path::Path,
    ) -> Result<BTreeMap<String, Vec<u8>>, std::io::Error> {
        fs::read_dir(output_directory)?
            .filter_map(|entry| match entry {
                Ok(entry)
                    if entry.file_type().is_ok_and(|kind| kind.is_file())
                        && entry.file_name().to_string_lossy().starts_with("asset-")
                        && entry.file_name() != ASSET_MANIFEST_JSON =>
                {
                    Some(Ok(entry))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .map(|entry| {
                let entry = entry?;
                let name = entry.file_name().into_string().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "non-UTF-8 fixture file name",
                    )
                })?;
                fs::read(entry.path()).map(|content| (name, content))
            })
            .collect()
    }

    fn final_artifact_names() -> Vec<OsString> {
        let mut names = ARTIFACT_NAMES.map(OsString::from).to_vec();
        names.push(OsString::from(crate::generation::CURRENT_GENERATION));
        names.push(OsString::from(crate::generation::GENERATION_STORE));
        names.push(OsString::from(crate::generation::ASSET_CACHE));
        names.sort();
        names
    }
}
