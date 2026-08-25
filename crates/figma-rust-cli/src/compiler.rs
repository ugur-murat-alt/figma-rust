use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use figma_rust_codegen::{CodegenError, GeneratedOutput};
use figma_rust_core::{Diagnostic, NormalizationOutput, Severity, normalize_bundle, parse_bundle};
use serde::Serialize;
use thiserror::Error;

const GENERATED_RUST: &str = "generated.rs";
const SOURCE_MAP_JSON: &str = "source-map.json";
const IR_JSON: &str = "ir.json";
const DIAGNOSTICS_JSON: &str = "diagnostics.json";
const ARTIFACT_NAMES: [&str; 4] = [GENERATED_RUST, SOURCE_MAP_JSON, IR_JSON, DIAGNOSTICS_JSON];
const ARTIFACT_LOCK: &str = ".figma-rust.lock";

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

#[derive(Clone, Copy, Debug, Default, Serialize)]
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
    pub root_count: usize,
    pub node_count: usize,
    pub diagnostic_summary: DiagnosticSummary,
    pub tree: Vec<InspectNode>,
    pub diagnostics: Vec<Diagnostic>,
}

impl InspectReport {
    #[must_use]
    pub fn text(&self) -> String {
        let mut output = format!(
            "schema {}: {} root(s), {} node(s)\ndiagnostics: {} error(s), {} warning(s), {} info\n",
            self.schema_version,
            self.root_count,
            self.node_count,
            self.diagnostic_summary.errors,
            self.diagnostic_summary.warnings,
            self.diagnostic_summary.info
        );
        for root in &self.tree {
            root.append_text(0, &mut output);
        }
        for diagnostic in &self.diagnostics {
            output.push_str(&format_diagnostic(diagnostic));
            output.push('\n');
        }
        output
    }
}

#[derive(Debug, Serialize)]
pub struct LintReport {
    pub summary: DiagnosticSummary,
    pub diagnostics: Vec<Diagnostic>,
}

impl LintReport {
    #[must_use]
    pub const fn failed(&self, strict: bool) -> bool {
        self.summary.errors > 0 || (strict && self.summary.warnings > 0)
    }

    #[must_use]
    pub fn text(&self) -> String {
        let mut output = format!(
            "lint: {} error(s), {} warning(s), {} info\n",
            self.summary.errors, self.summary.warnings, self.summary.info
        );
        for diagnostic in &self.diagnostics {
            output.push_str(&format_diagnostic(diagnostic));
            output.push('\n');
        }
        output
    }
}

#[derive(Debug)]
pub struct CompileResult {
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompilerResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_map: Option<figma_rust_codegen::SourceMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn inspect_file(path: &Path) -> Result<InspectReport, CliError> {
    let bundle = read_bundle(path)?;
    Ok(inspect_bundle(&bundle))
}

fn inspect_bundle(bundle: &figma_rust_core::raw::ExtractionBundle) -> InspectReport {
    let normalization = normalize_bundle(bundle);
    let tree = bundle
        .roots
        .iter()
        .map(InspectNode::from_raw)
        .collect::<Vec<_>>();
    let node_count = tree.iter().map(InspectNode::node_count).sum();
    InspectReport {
        schema_version: bundle.schema_version,
        root_count: tree.len(),
        node_count,
        diagnostic_summary: DiagnosticSummary::from_diagnostics(&normalization.diagnostics),
        tree,
        diagnostics: normalization.diagnostics,
    }
}

pub fn lint_file(path: &Path) -> Result<LintReport, CliError> {
    let bundle = read_bundle(path)?;
    Ok(lint_bundle(&bundle))
}

fn lint_bundle(bundle: &figma_rust_core::raw::ExtractionBundle) -> LintReport {
    let normalization = normalize_bundle(bundle);
    LintReport {
        summary: DiagnosticSummary::from_diagnostics(&normalization.diagnostics),
        diagnostics: normalization.diagnostics,
    }
}

pub fn compile_file(path: &Path, output_directory: &Path) -> Result<CompileResult, CliError> {
    let bundle = read_bundle(path)?;
    let normalization = normalize_bundle(&bundle);
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

pub fn lint_source(input: &str) -> Result<CompilerResponse, String> {
    let bundle = parse_bundle(input).map_err(|error| error.to_string())?;
    let normalization = normalize_bundle(&bundle);
    let has_errors = normalization.has_errors();
    Ok(CompilerResponse {
        code: None,
        diagnostics: normalization.diagnostics,
        source_map: None,
        error: has_errors.then(|| "normalization failed".to_owned()),
    })
}

pub fn compile_source(input: &str) -> Result<CompilerResponse, String> {
    let bundle = parse_bundle(input).map_err(|error| error.to_string())?;
    let normalization = normalize_bundle(&bundle);
    if normalization.has_errors() {
        return Ok(CompilerResponse {
            code: None,
            diagnostics: normalization.diagnostics,
            source_map: None,
            error: Some("normalization failed".to_owned()),
        });
    }

    match figma_rust_codegen::generate(&normalization.document) {
        Ok(generated) => Ok(CompilerResponse {
            code: Some(generated.rust),
            diagnostics: normalization.diagnostics,
            source_map: Some(generated.source_map),
            error: None,
        }),
        Err(error) => {
            let message = error.to_string();
            let mut diagnostics = normalization.diagnostics;
            diagnostics.push(codegen_diagnostic(&error));
            Ok(CompilerResponse {
                code: None,
                diagnostics,
                source_map: None,
                error: Some(message),
            })
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
        let paths = prepare_publish(output_directory)?;
        if let Err(error) = stage_outputs(&paths, &outputs) {
            let cleanup_errors = cleanup_temps(&paths);
            return Err(transaction_error(error, &cleanup_errors));
        }
        publish_outputs(&paths)
    });
    finish_output_directory(result, output_directory, created_directory)
}

fn output_artifacts(
    normalization: &NormalizationOutput,
    generated: &GeneratedOutput,
) -> Result<[OutputArtifact; 4], CliError> {
    Ok([
        OutputArtifact {
            name: GENERATED_RUST,
            content: generated.rust.as_bytes().to_vec(),
        },
        OutputArtifact {
            name: SOURCE_MAP_JSON,
            content: serialize_json("source map", &generated.source_map)?.into_bytes(),
        },
        OutputArtifact {
            name: IR_JSON,
            content: serialize_json("normalized IR", &normalization.document)?.into_bytes(),
        },
        OutputArtifact {
            name: DIAGNOSTICS_JSON,
            content: serialize_json("diagnostics", &normalization.diagnostics)?.into_bytes(),
        },
    ])
}

struct OutputArtifact {
    name: &'static str,
    content: Vec<u8>,
}

struct ArtifactPaths {
    name: &'static str,
    final_path: PathBuf,
    temp_path: PathBuf,
    backup_path: PathBuf,
    had_previous: bool,
}

fn prepare_publish(output_directory: &Path) -> Result<Vec<ArtifactPaths>, CliError> {
    ARTIFACT_NAMES
        .iter()
        .copied()
        .map(|name| {
            let final_path = output_directory.join(name);
            let had_previous = validate_regular_or_missing(&final_path, "artifact destination")?;
            let temp_path = work_path(output_directory, name, "tmp");
            let backup_path = work_path(output_directory, name, "backup");
            validate_missing(&temp_path, "temporary artifact path")?;
            validate_missing(&backup_path, "artifact backup path")?;
            Ok(ArtifactPaths {
                name,
                final_path,
                temp_path,
                backup_path,
                had_previous,
            })
        })
        .collect()
}

fn stage_outputs(paths: &[ArtifactPaths], outputs: &[OutputArtifact]) -> Result<(), String> {
    for (path, output) in paths.iter().zip(outputs) {
        if path.name != output.name {
            return Err("internal artifact ordering mismatch".to_owned());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path.temp_path)
            .map_err(|error| format!("creating {}: {error}", path.temp_path.display()))?;
        file.write_all(&output.content)
            .map_err(|error| format!("writing {}: {error}", path.temp_path.display()))?;
        file.sync_all()
            .map_err(|error| format!("syncing {}: {error}", path.temp_path.display()))?;
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

    for (published, path) in paths.iter().enumerate() {
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
    for path in paths.iter().take(published) {
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
        invalidate_outputs_locked(output_directory)
    })
}

fn invalidate_outputs_locked(output_directory: &Path) -> Result<(), CliError> {
    let paths = ARTIFACT_NAMES
        .iter()
        .copied()
        .map(|name| {
            let final_path = output_directory.join(name);
            let had_previous = validate_regular_or_missing(&final_path, "artifact destination")?;
            let quarantine_path = work_path(output_directory, name, "invalid");
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
        ARTIFACT_NAMES, ArtifactLock, GENERATED_RUST, compile_file, compile_source, inspect_bundle,
        lint_source, output_artifacts, parse_bundle, prepare_publish, publish_outputs_with,
        stage_outputs, work_path,
    };

    const BASIC: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.raw.json");

    #[test]
    fn inspect_is_deterministic_and_counts_preorder_tree() -> Result<(), Box<dyn std::error::Error>>
    {
        let bundle = parse_bundle(BASIC)?;
        let first = serde_json::to_string_pretty(&inspect_bundle(&bundle))?;
        let second = serde_json::to_string_pretty(&inspect_bundle(&bundle))?;
        assert_eq!(first, second);
        assert!(first.contains("\"node_count\": 1"));
        assert!(first.contains("\"id\": \"1:1\""));
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
    fn codegen_error_is_node_and_property_scoped() -> Result<(), Box<dyn std::error::Error>> {
        let grid = BASIC.replace(
            "\"kind\": \"RECTANGLE\",",
            "\"kind\": \"RECTANGLE\", \"layout\": { \"mode\": \"GRID\" },",
        );
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
                "diagnostics.json",
                "generated.rs",
                "ir.json",
                "source-map.json"
            ]
            .map(OsString::from)
        );
        for name in names {
            assert!(fs::read(out.join(name))?.ends_with(b"\n"));
        }
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
            BASIC.replace("\"schema_version\": 1", "\"schema_version\": 999"),
        )?;
        let result = compile_file(&raw, &out)?;
        assert_eq!(result.error.as_deref(), Some("normalization failed"));
        assert!(ARTIFACT_NAMES.iter().all(|name| !out.join(name).exists()));
        assert_eq!(fs::read_to_string(out.join("keep.txt"))?, "unrelated");

        fs::write(&raw, BASIC)?;
        assert!(compile_file(&raw, &out)?.error.is_none());
        fs::write(
            &raw,
            BASIC.replace(
                "\"kind\": \"RECTANGLE\",",
                "\"kind\": \"RECTANGLE\", \"layout\": { \"mode\": \"GRID\" },",
            ),
        )?;
        let result = compile_file(&raw, &out)?;
        assert!(result.error.is_some());
        assert!(ARTIFACT_NAMES.iter().all(|name| !out.join(name).exists()));
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
        assert_ne!(
            outputs[0].content,
            *previous
                .get(GENERATED_RUST)
                .ok_or("missing previous generated Rust")?
        );
        let lock = ArtifactLock::acquire(&out)?;
        let paths = prepare_publish(&out)?;
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
            BASIC.replace("\"schema_version\": 1", "\"schema_version\": 999"),
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

    fn final_artifact_names() -> Vec<OsString> {
        let mut names = ARTIFACT_NAMES.map(OsString::from).to_vec();
        names.sort();
        names
    }
}
