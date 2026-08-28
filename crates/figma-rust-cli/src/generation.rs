use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::compiler::OutputArtifact;

pub(crate) const CURRENT_GENERATION: &str = "current-generation.json";
const PENDING_GENERATION: &str = ".figma-rust-current.pending";
pub(crate) const GENERATION_STORE: &str = ".figma-rust-generations";
pub(crate) const ASSET_CACHE: &str = ".figma-rust-asset-cache";
const GENERATION_MANIFEST: &str = "generation.json";
const MAX_RECOVERY_ENTRIES: usize = 32;
const MAX_GENERATION_ARTIFACTS: usize = 10_000;
const MAX_CACHE_SCAN_ENTRIES: usize = MAX_GENERATION_ARTIFACTS * 2 + MAX_RECOVERY_ENTRIES;
const MAX_GENERATION_BYTES: u64 = 512 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct GenerationManifest {
    schema_version: u32,
    generation_id: String,
    artifacts: Vec<GenerationArtifact>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct GenerationArtifact {
    name: String,
    bytes: u64,
    sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

pub(crate) struct PreparedGeneration {
    manifest: GenerationManifest,
}

pub(crate) fn recover(
    output_directory: &Path,
    mut project: impl FnMut(&[OutputArtifact], &BTreeSet<String>) -> Result<(), String>,
) -> Result<(), String> {
    cleanup_temporary_paths(output_directory)?;
    let pending_path = output_directory.join(PENDING_GENERATION);
    let current_path = output_directory.join(CURRENT_GENERATION);
    if pending_path.exists() {
        let pending = read_manifest(&pending_path)?;
        let outputs = read_generation(output_directory, &pending)?;
        let mut owned_names = manifest_names(&pending);
        if current_path.exists() {
            let current = read_manifest(&current_path)?;
            validate_generation(output_directory, &current)?;
            owned_names.extend(manifest_names(&current));
        }
        if !projection_matches(output_directory, &outputs)? {
            project(&outputs, &owned_names)?;
        }
        let mut checkpoint = |_: &'static str| Ok::<(), String>(());
        commit_pending(output_directory, &mut checkpoint)?;
        cleanup_orphan_generations(output_directory)?;
        cleanup_asset_cache(output_directory)?;
        return Ok(());
    }
    if current_path.exists() {
        let current = read_manifest(&current_path)?;
        let outputs = read_generation(output_directory, &current)?;
        if !projection_matches(output_directory, &outputs)? {
            project(&outputs, &manifest_names(&current))?;
        }
    }
    cleanup_orphan_generations(output_directory)?;
    cleanup_asset_cache(output_directory)
}

pub(crate) fn prepare(
    output_directory: &Path,
    outputs: &[OutputArtifact],
) -> Result<PreparedGeneration, String> {
    let mut checkpoint = |_: &'static str| Ok::<(), String>(());
    prepare_with_checkpoint(output_directory, outputs, &mut checkpoint)
}

pub(crate) fn prepare_with_checkpoint(
    output_directory: &Path,
    outputs: &[OutputArtifact],
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<PreparedGeneration, String> {
    let manifest = build_manifest(outputs)?;
    ensure_generation_store(output_directory, checkpoint)?;
    ensure_asset_cache(output_directory, checkpoint)?;
    stage_generation(output_directory, outputs, &manifest, checkpoint)?;
    write_pending(output_directory, &manifest, checkpoint)?;
    Ok(PreparedGeneration { manifest })
}

pub(crate) fn commit(output_directory: &Path, prepared: &PreparedGeneration) -> Result<(), String> {
    let mut checkpoint = |_: &'static str| Ok::<(), String>(());
    commit_with_checkpoint(output_directory, prepared, &mut checkpoint)
}

pub(crate) fn commit_with_checkpoint(
    output_directory: &Path,
    prepared: &PreparedGeneration,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let pending = read_manifest(&output_directory.join(PENDING_GENERATION))?;
    if pending != prepared.manifest {
        return Err("pending generation changed before commit".to_owned());
    }
    commit_pending(output_directory, checkpoint)?;
    cleanup_orphan_generations(output_directory)?;
    cleanup_asset_cache(output_directory)
}

pub(crate) fn invalidate(output_directory: &Path) -> Vec<String> {
    let mut errors = [CURRENT_GENERATION, PENDING_GENERATION]
        .iter()
        .filter_map(|name| remove_regular_if_exists(&output_directory.join(name)).err())
        .collect::<Vec<_>>();
    if let Err(error) = cleanup_asset_cache(output_directory) {
        errors.push(error);
    }
    errors
}

pub(crate) fn asset_cache_key(
    content: &[u8],
    media_type: &str,
    export_settings: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"figma-rust-asset-cache-v1\0");
    update_length_prefixed(&mut hasher, media_type.as_bytes());
    for (key, value) in export_settings {
        update_length_prefixed(&mut hasher, key.as_bytes());
        update_length_prefixed(&mut hasher, value.as_bytes());
    }
    update_length_prefixed(&mut hasher, content);
    hex_digest(hasher.finalize().as_slice())
}

pub(crate) fn stage_projection_artifact(
    destination: &Path,
    output: &OutputArtifact,
) -> Result<(), String> {
    if let Some(cache_key) = &output.cache_key {
        let output_directory = destination
            .parent()
            .ok_or_else(|| format!("projection path {} has no parent", destination.display()))?;
        let blob = output_directory.join(ASSET_CACHE).join(cache_key);
        validate_cached_blob(&blob, cache_key, &output.content)?;
        // Flat projections are compatibility copies, not immutable cache members.
        // Keeping a distinct inode prevents a consumer edit from corrupting the
        // cache blob and every generation that reuses it through hard links.
    }
    write_synced_file(destination, &output.content)
}

fn build_manifest(outputs: &[OutputArtifact]) -> Result<GenerationManifest, String> {
    if outputs.len() > MAX_GENERATION_ARTIFACTS {
        return Err(format!(
            "generation has {} artifacts, exceeding {MAX_GENERATION_ARTIFACTS}",
            outputs.len()
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total_bytes = 0_u64;
    let mut artifacts = Vec::with_capacity(outputs.len());
    for output in outputs {
        validate_artifact_name(&output.name)?;
        if !seen.insert(output.name.as_str()) {
            return Err(format!("duplicate generation artifact {}", output.name));
        }
        let bytes = u64::try_from(output.content.len())
            .map_err(|_| format!("artifact {} is too large", output.name))?;
        total_bytes = total_bytes.saturating_add(bytes);
        artifacts.push(GenerationArtifact {
            name: output.name.clone(),
            bytes,
            sha256: sha256_hex(&output.content),
            cache_key: output.cache_key.clone(),
        });
    }
    if total_bytes > MAX_GENERATION_BYTES {
        return Err(format!(
            "generation has {total_bytes} bytes, exceeding {MAX_GENERATION_BYTES}"
        ));
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    let generation_id = generation_id(outputs)?;
    Ok(GenerationManifest {
        schema_version: 1,
        generation_id,
        artifacts,
    })
}

fn generation_id(outputs: &[OutputArtifact]) -> Result<String, String> {
    let mut ordered = outputs.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.name.cmp(&right.name));
    let mut hasher = Sha256::new();
    hasher.update(b"figma-rust-generation-v1\0");
    for output in ordered {
        let name_length = u64::try_from(output.name.len())
            .map_err(|_| format!("artifact name {} is too long", output.name))?;
        let content_length = u64::try_from(output.content.len())
            .map_err(|_| format!("artifact {} is too large", output.name))?;
        hasher.update(name_length.to_be_bytes());
        hasher.update(output.name.as_bytes());
        hasher.update(content_length.to_be_bytes());
        hasher.update(&output.content);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn ensure_generation_store(
    output_directory: &Path,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let store = output_directory.join(GENERATION_STORE);
    match fs::symlink_metadata(&store) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "generation store {} is not a real directory",
            store.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&store).map_err(|error| {
                format!("creating generation store {}: {error}", store.display())
            })?;
            sync_directory(output_directory)?;
            checkpoint("sync-output-after-store-create")
        }
        Err(error) => Err(format!(
            "inspecting generation store {}: {error}",
            store.display()
        )),
    }
}

fn ensure_asset_cache(
    output_directory: &Path,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let cache = output_directory.join(ASSET_CACHE);
    match fs::symlink_metadata(&cache) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "asset cache {} is not a real directory",
            cache.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&cache)
                .map_err(|error| format!("creating asset cache {}: {error}", cache.display()))?;
            sync_directory(output_directory)?;
            checkpoint("sync-output-after-cache-create")
        }
        Err(error) => Err(format!(
            "inspecting asset cache {}: {error}",
            cache.display()
        )),
    }
}

fn ensure_asset_blob(
    output_directory: &Path,
    output: &OutputArtifact,
    cache_key: &str,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let cache = output_directory.join(ASSET_CACHE);
    let blob = cache.join(cache_key);
    if blob.exists() {
        validate_cached_blob(&blob, cache_key, &output.content)?;
        return Ok(blob);
    }
    let temporary = cache.join(format!(
        ".tmp-{cache_key}-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    write_synced_file(&temporary, &output.content)?;
    checkpoint("sync-asset-cache-blob")?;
    fs::rename(&temporary, &blob).map_err(|error| {
        format!(
            "publishing asset cache blob {} to {}: {error}",
            temporary.display(),
            blob.display()
        )
    })?;
    checkpoint("rename-asset-cache-blob")?;
    sync_directory(&cache)?;
    checkpoint("sync-asset-cache")?;
    Ok(blob)
}

fn validate_cached_blob(path: &Path, cache_key: &str, expected: &[u8]) -> Result<(), String> {
    if cache_key.len() != 64 || !cache_key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid asset cache key {cache_key}"));
    }
    validate_regular_file(path, "asset cache blob")?;
    let actual = fs::read(path)
        .map_err(|error| format!("reading asset cache blob {}: {error}", path.display()))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "asset cache blob {} does not match the verified artifact bytes",
            path.display()
        ))
    }
}

fn stage_generation(
    output_directory: &Path,
    outputs: &[OutputArtifact],
    manifest: &GenerationManifest,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let store = output_directory.join(GENERATION_STORE);
    let final_path = store.join(&manifest.generation_id);
    if final_path.exists() {
        return validate_generation(output_directory, manifest);
    }
    let temporary = store.join(format!(
        ".tmp-{}-{}-{}",
        manifest.generation_id,
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&temporary)
        .map_err(|error| format!("creating generation {}: {error}", temporary.display()))?;
    for output in outputs {
        if let Some(cache_key) = &output.cache_key {
            let blob = ensure_asset_blob(output_directory, output, cache_key, checkpoint)?;
            fs::hard_link(&blob, temporary.join(&output.name)).map_err(|error| {
                format!(
                    "linking cached asset {} into generation: {error}",
                    blob.display()
                )
            })?;
        } else {
            write_synced_file(&temporary.join(&output.name), &output.content)?;
        }
        checkpoint("sync-generation-artifact")?;
    }
    let manifest_bytes = manifest_bytes(manifest)?;
    write_synced_file(&temporary.join(GENERATION_MANIFEST), &manifest_bytes)?;
    checkpoint("sync-generation-manifest")?;
    sync_directory(&temporary)?;
    checkpoint("sync-generation-directory")?;
    fs::rename(&temporary, &final_path).map_err(|error| {
        format!(
            "publishing generation {} to {}: {error}",
            temporary.display(),
            final_path.display()
        )
    })?;
    checkpoint("rename-generation-directory")?;
    sync_directory(&store)?;
    checkpoint("sync-generation-store")
}

fn write_pending(
    output_directory: &Path,
    manifest: &GenerationManifest,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let pending = output_directory.join(PENDING_GENERATION);
    if pending.exists() {
        return Err(format!(
            "pending generation {} already exists",
            pending.display()
        ));
    }
    let temporary = unique_sibling(output_directory, "pending");
    write_synced_file(&temporary, &manifest_bytes(manifest)?)?;
    checkpoint("sync-pending-pointer")?;
    fs::rename(&temporary, &pending).map_err(|error| {
        format!(
            "publishing pending generation {} to {}: {error}",
            temporary.display(),
            pending.display()
        )
    })?;
    checkpoint("rename-pending-pointer")?;
    sync_directory(output_directory)?;
    checkpoint("sync-output-after-pending")
}

fn commit_pending(
    output_directory: &Path,
    checkpoint: &mut dyn FnMut(&'static str) -> Result<(), String>,
) -> Result<(), String> {
    let pending = output_directory.join(PENDING_GENERATION);
    let current = output_directory.join(CURRENT_GENERATION);
    fs::rename(&pending, &current).map_err(|error| {
        format!(
            "committing generation pointer {}: {error}",
            current.display()
        )
    })?;
    checkpoint("rename-pending-pointer-to-current")?;
    sync_directory(output_directory)?;
    checkpoint("sync-output-after-current")?;
    Ok(())
}

fn read_generation(
    output_directory: &Path,
    manifest: &GenerationManifest,
) -> Result<Vec<OutputArtifact>, String> {
    validate_generation(output_directory, manifest)?;
    let generation = output_directory
        .join(GENERATION_STORE)
        .join(&manifest.generation_id);
    manifest
        .artifacts
        .iter()
        .map(|artifact| {
            fs::read(generation.join(&artifact.name))
                .map(|content| OutputArtifact {
                    name: artifact.name.clone(),
                    content,
                    cache_key: artifact.cache_key.clone(),
                })
                .map_err(|error| format!("reading generation artifact {}: {error}", artifact.name))
        })
        .collect()
}

fn validate_generation(
    output_directory: &Path,
    expected: &GenerationManifest,
) -> Result<(), String> {
    validate_manifest(expected)?;
    let generation = output_directory
        .join(GENERATION_STORE)
        .join(&expected.generation_id);
    validate_real_directory(&generation, "generation")?;
    let stored = read_manifest(&generation.join(GENERATION_MANIFEST))?;
    if stored != *expected {
        return Err(format!(
            "generation manifest mismatch in {}",
            generation.display()
        ));
    }
    let expected_names = expected
        .artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .chain(std::iter::once(GENERATION_MANIFEST))
        .collect::<BTreeSet<_>>();
    let actual_names = fs::read_dir(&generation)
        .map_err(|error| format!("reading generation {}: {error}", generation.display()))?
        .map(|entry| {
            entry
                .map_err(|error| format!("reading generation entry: {error}"))
                .and_then(|entry| {
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| "generation contains a non-UTF-8 file name".to_owned())
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_names
    {
        return Err(format!(
            "generation {} contains an unexpected file set",
            generation.display()
        ));
    }
    let mut validated_outputs = Vec::with_capacity(expected.artifacts.len());
    for artifact in &expected.artifacts {
        let path = generation.join(&artifact.name);
        validate_regular_file(&path, "generation artifact")?;
        let bytes = fs::read(&path)
            .map_err(|error| format!("reading generation artifact {}: {error}", path.display()))?;
        let length = u64::try_from(bytes.len())
            .map_err(|_| format!("generation artifact {} is too large", path.display()))?;
        if length != artifact.bytes || sha256_hex(&bytes) != artifact.sha256 {
            return Err(format!(
                "generation artifact {} failed hash/size validation",
                path.display()
            ));
        }
        if let Some(cache_key) = &artifact.cache_key {
            validate_cached_blob(
                &output_directory.join(ASSET_CACHE).join(cache_key),
                cache_key,
                &bytes,
            )?;
        }
        validated_outputs.push(OutputArtifact {
            name: artifact.name.clone(),
            content: bytes,
            cache_key: artifact.cache_key.clone(),
        });
    }
    if generation_id(&validated_outputs)? != expected.generation_id {
        return Err(format!(
            "generation {} is not addressed by its artifact content",
            generation.display()
        ));
    }
    Ok(())
}

fn validate_manifest(manifest: &GenerationManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported generation schema {}",
            manifest.schema_version
        ));
    }
    if manifest.generation_id.len() != 64
        || !manifest
            .generation_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("generation ID must be 64 hexadecimal characters".to_owned());
    }
    if manifest.artifacts.len() > MAX_GENERATION_ARTIFACTS {
        return Err("generation manifest exceeds the artifact limit".to_owned());
    }
    let mut previous = None;
    let mut total = 0_u64;
    for artifact in &manifest.artifacts {
        validate_artifact_name(&artifact.name)?;
        if previous.is_some_and(|name: &str| name >= artifact.name.as_str()) {
            return Err("generation artifacts must be unique and sorted".to_owned());
        }
        if artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("artifact {} has an invalid SHA-256", artifact.name));
        }
        if artifact.cache_key.as_ref().is_some_and(|cache_key| {
            cache_key.len() != 64 || !cache_key.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(format!(
                "artifact {} has an invalid cache key",
                artifact.name
            ));
        }
        total = total.saturating_add(artifact.bytes);
        previous = Some(&artifact.name);
    }
    if total > MAX_GENERATION_BYTES {
        return Err("generation manifest exceeds the byte limit".to_owned());
    }
    Ok(())
}

fn projection_matches(output_directory: &Path, outputs: &[OutputArtifact]) -> Result<bool, String> {
    for output in outputs {
        let path = output_directory.join(&output.name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let content = fs::read(&path).map_err(|error| {
                    format!("reading projected artifact {}: {error}", path.display())
                })?;
                if content != output.content {
                    return Ok(false);
                }
            }
            Ok(_) => {
                return Err(format!(
                    "projected artifact {} is not a regular file",
                    path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "inspecting projected artifact {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(true)
}

fn manifest_names(manifest: &GenerationManifest) -> BTreeSet<String> {
    manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.name.clone())
        .collect()
}

fn read_manifest(path: &Path) -> Result<GenerationManifest, String> {
    validate_regular_file(path, "generation manifest")?;
    let bytes = fs::read(path)
        .map_err(|error| format!("reading generation manifest {}: {error}", path.display()))?;
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parsing generation manifest {}: {error}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn manifest_bytes(manifest: &GenerationManifest) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("serializing generation manifest: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn cleanup_temporary_paths(output_directory: &Path) -> Result<(), String> {
    let store = output_directory.join(GENERATION_STORE);
    if store.exists() {
        let entries = fs::read_dir(&store)
            .map_err(|error| format!("reading generation store {}: {error}", store.display()))?;
        for entry in entries.take(MAX_CACHE_SCAN_ENTRIES) {
            let entry = entry.map_err(|error| format!("reading generation entry: {error}"))?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(".tmp-") {
                remove_generation_directory(&entry.path())?;
            }
        }
    }
    let cache = output_directory.join(ASSET_CACHE);
    if cache.exists() {
        let entries = fs::read_dir(&cache)
            .map_err(|error| format!("reading asset cache {}: {error}", cache.display()))?;
        for entry in entries.take(MAX_RECOVERY_ENTRIES) {
            let entry = entry.map_err(|error| format!("reading asset cache entry: {error}"))?;
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                remove_regular_if_exists(&entry.path())?;
            }
        }
    }
    for entry in fs::read_dir(output_directory)
        .map_err(|error| {
            format!(
                "reading output directory {}: {error}",
                output_directory.display()
            )
        })?
        .take(MAX_CACHE_SCAN_ENTRIES)
    {
        let entry = entry.map_err(|error| format!("reading output entry: {error}"))?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".figma-rust-pending.tmp-")
        {
            remove_regular_if_exists(&entry.path())?;
        }
    }
    Ok(())
}

fn cleanup_orphan_generations(output_directory: &Path) -> Result<(), String> {
    let mut retained = BTreeSet::new();
    for name in [CURRENT_GENERATION, PENDING_GENERATION] {
        let path = output_directory.join(name);
        if path.exists() {
            retained.insert(read_manifest(&path)?.generation_id);
        }
    }
    let store = output_directory.join(GENERATION_STORE);
    if !store.exists() {
        return Ok(());
    }
    let mut removed = 0;
    for entry in fs::read_dir(&store)
        .map_err(|error| format!("reading generation store {}: {error}", store.display()))?
        .take(MAX_RECOVERY_ENTRIES)
    {
        let entry = entry.map_err(|error| format!("reading generation entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 64
            && name.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !retained.contains(&name)
            && removed < MAX_RECOVERY_ENTRIES
        {
            remove_generation_directory(&entry.path())?;
            removed += 1;
        }
    }
    Ok(())
}

fn cleanup_asset_cache(output_directory: &Path) -> Result<(), String> {
    let cache = output_directory.join(ASSET_CACHE);
    if !cache.exists() {
        return Ok(());
    }
    validate_real_directory(&cache, "asset cache")?;
    let mut retained = BTreeSet::new();
    for name in [CURRENT_GENERATION, PENDING_GENERATION] {
        let path = output_directory.join(name);
        if path.exists() {
            retained.extend(
                read_manifest(&path)?
                    .artifacts
                    .into_iter()
                    .filter_map(|artifact| artifact.cache_key),
            );
        }
    }
    let mut removed = 0;
    for entry in fs::read_dir(&cache)
        .map_err(|error| format!("reading asset cache {}: {error}", cache.display()))?
        .take(MAX_RECOVERY_ENTRIES)
    {
        let entry = entry.map_err(|error| format!("reading asset cache entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 64
            && name.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !retained.contains(&name)
            && removed < MAX_RECOVERY_ENTRIES
        {
            remove_regular_if_exists(&entry.path())?;
            removed += 1;
        }
    }
    sync_directory(&cache)
}

fn remove_generation_directory(path: &Path) -> Result<(), String> {
    validate_real_directory(path, "generation cleanup target")?;
    for entry in fs::read_dir(path)
        .map_err(|error| {
            format!(
                "reading generation cleanup target {}: {error}",
                path.display()
            )
        })?
        .take(MAX_GENERATION_ARTIFACTS + 1)
    {
        let entry = entry.map_err(|error| format!("reading cleanup entry: {error}"))?;
        validate_regular_file(&entry.path(), "generation cleanup file")?;
        fs::remove_file(entry.path())
            .map_err(|error| format!("removing generation file: {error}"))?;
    }
    fs::remove_dir(path)
        .map_err(|error| format!("removing generation directory {}: {error}", path.display()))
}

fn write_synced_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("creating {}: {error}", path.display()))?;
    file.write_all(content)
        .map_err(|error| format!("writing {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("syncing {}: {error}", path.display()))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("syncing directory {}: {error}", path.display()))
}

fn validate_real_directory(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "{label} {} is not a real directory",
            path.display()
        )),
        Err(error) => Err(format!("inspecting {label} {}: {error}", path.display())),
    }
}

fn validate_regular_file(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!("{label} {} is not a regular file", path.display())),
        Err(error) => Err(format!("inspecting {label} {}: {error}", path.display())),
    }
}

fn validate_artifact_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    if path.components().count() == 1 && !name.starts_with('.') && path.file_name().is_some() {
        Ok(())
    } else {
        Err(format!("unsafe generation artifact name {name}"))
    }
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

fn unique_sibling(output_directory: &Path, label: &str) -> PathBuf {
    output_directory.join(format!(
        ".figma-rust-{label}.tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn update_length_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::Path,
    };

    use super::{
        ASSET_CACHE, CURRENT_GENERATION, GENERATION_STORE, asset_cache_key, commit,
        commit_with_checkpoint, prepare, prepare_with_checkpoint, recover,
    };
    use crate::compiler::OutputArtifact;

    #[test]
    fn every_persistence_checkpoint_recovers_one_complete_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let old = artifacts(b"old");
        let new = artifacts(b"new");
        let checkpoint_count = count_checkpoints(&old, &new)?;
        assert!(checkpoint_count >= 10);

        for fault in 0..checkpoint_count {
            let base = unique_test_path(&format!("fault-{fault}"));
            fs::create_dir_all(&base)?;
            seed_generation(&base, &old)?;
            let mut seen = 0;
            let mut inject = |_: &'static str| {
                let current = seen;
                seen += 1;
                if current == fault {
                    Err(format!("injected persistence fault {fault}"))
                } else {
                    Ok(())
                }
            };
            if let Ok(prepared) = prepare_with_checkpoint(&base, &new, &mut inject) {
                project(&base, &new, &artifact_names(&new))?;
                let _ = commit_with_checkpoint(&base, &prepared, &mut inject);
            }
            recover(&base, |outputs, names| project(&base, outputs, names))?;
            assert_complete_old_or_new(&base, &old, &new)?;
            assert_no_recovery_work_files(&base)?;
            fs::remove_dir_all(base)?;
        }
        Ok(())
    }

    #[test]
    fn asset_cache_key_includes_bytes_media_type_and_export_semantics() {
        let svg = b"<svg/>";
        let authored = BTreeMap::from([
            ("color_policy".to_owned(), "authored".to_owned()),
            ("format".to_owned(), "SVG".to_owned()),
        ]);
        let outlined = BTreeMap::from([
            ("color_policy".to_owned(), "outlined".to_owned()),
            ("format".to_owned(), "SVG".to_owned()),
        ]);

        let first = asset_cache_key(svg, "image/svg+xml", &authored);
        assert_eq!(first, asset_cache_key(svg, "image/svg+xml", &authored));
        assert_ne!(
            first,
            asset_cache_key(b"<svg>changed</svg>", "image/svg+xml", &authored)
        );
        assert_ne!(first, asset_cache_key(svg, "image/png", &authored));
        assert_ne!(first, asset_cache_key(svg, "image/svg+xml", &outlined));
    }

    #[test]
    fn exact_assets_share_one_blob_and_unreachable_blobs_are_collected()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("asset-cache");
        fs::create_dir_all(&base)?;
        let first = cached_artifacts(b"shared");
        seed_generation(&base, &first)?;
        let first_key = first[1]
            .cache_key
            .as_deref()
            .ok_or("missing first cache key")?;
        assert_eq!(fs::read_dir(base.join(ASSET_CACHE))?.count(), 1);
        assert_eq!(fs::read(base.join(ASSET_CACHE).join(first_key))?, b"shared");

        let changed = cached_artifacts(b"changed");
        let changed_key = changed[1]
            .cache_key
            .as_deref()
            .ok_or("missing changed cache key")?;
        let prepared = prepare(&base, &changed).map_err(std::io::Error::other)?;
        let mut owned = artifact_names(&first);
        owned.extend(artifact_names(&changed));
        project(&base, &changed, &owned).map_err(std::io::Error::other)?;
        commit(&base, &prepared).map_err(std::io::Error::other)?;

        assert!(!base.join(ASSET_CACHE).join(first_key).exists());
        assert_eq!(
            fs::read(base.join(ASSET_CACHE).join(changed_key))?,
            b"changed"
        );
        assert_eq!(fs::read_dir(base.join(ASSET_CACHE))?.count(), 1);
        fs::remove_dir_all(base)?;
        Ok(())
    }

    #[test]
    fn first_compile_failure_before_the_pointer_collects_orphan_cache_and_generation()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = unique_test_path("first-cache-failure");
        fs::create_dir_all(&base)?;
        let outputs = cached_artifacts(b"orphan");
        let mut inject = |checkpoint: &'static str| {
            if checkpoint == "sync-pending-pointer" {
                Err("injected failure before pending publication".to_owned())
            } else {
                Ok(())
            }
        };
        assert!(prepare_with_checkpoint(&base, &outputs, &mut inject).is_err());

        recover(&base, |outputs, names| project(&base, outputs, names))?;
        assert_eq!(fs::read_dir(base.join(ASSET_CACHE))?.count(), 0);
        assert_eq!(fs::read_dir(base.join(GENERATION_STORE))?.count(), 0);
        assert!(!base.join(CURRENT_GENERATION).exists());
        assert_no_recovery_work_files(&base)?;

        fs::remove_dir_all(base)?;
        Ok(())
    }

    fn count_checkpoints(
        old: &[OutputArtifact],
        new: &[OutputArtifact],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let base = unique_test_path("count-checkpoints");
        fs::create_dir_all(&base)?;
        seed_generation(&base, old)?;
        let mut count = 0;
        let mut checkpoint = |_: &'static str| {
            count += 1;
            Ok(())
        };
        let prepared = prepare_with_checkpoint(&base, new, &mut checkpoint)?;
        project(&base, new, &artifact_names(new))?;
        commit_with_checkpoint(&base, &prepared, &mut checkpoint)?;
        fs::remove_dir_all(base)?;
        Ok(count)
    }

    fn seed_generation(output_directory: &Path, outputs: &[OutputArtifact]) -> Result<(), String> {
        let prepared = prepare(output_directory, outputs)?;
        project(output_directory, outputs, &artifact_names(outputs))?;
        commit(output_directory, &prepared)
    }

    fn project(
        output_directory: &Path,
        outputs: &[OutputArtifact],
        owned_names: &BTreeSet<String>,
    ) -> Result<(), String> {
        for name in owned_names {
            let path = output_directory.join(name);
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|error| format!("removing projection {}: {error}", path.display()))?;
            }
        }
        for output in outputs {
            super::stage_projection_artifact(&output_directory.join(&output.name), output)?;
        }
        Ok(())
    }

    fn assert_complete_old_or_new(
        output_directory: &Path,
        old: &[OutputArtifact],
        new: &[OutputArtifact],
    ) -> Result<(), String> {
        let current = fs::read(output_directory.join(CURRENT_GENERATION))
            .map_err(|error| format!("reading current generation: {error}"))?;
        let old_projection = projection_bytes(output_directory, old)?;
        let new_projection = projection_bytes(output_directory, new)?;
        let expected_old = old
            .iter()
            .map(|output| output.content.clone())
            .collect::<Vec<_>>();
        let expected_new = new
            .iter()
            .map(|output| output.content.clone())
            .collect::<Vec<_>>();
        if old_projection == expected_old {
            assert_ne!(new_projection, expected_new);
        } else {
            assert_eq!(new_projection, expected_new);
        }
        if current.is_empty() {
            return Err("current generation pointer is empty".to_owned());
        }
        Ok(())
    }

    fn projection_bytes(
        output_directory: &Path,
        outputs: &[OutputArtifact],
    ) -> Result<Vec<Vec<u8>>, String> {
        outputs
            .iter()
            .map(|output| {
                fs::read(output_directory.join(&output.name))
                    .map_err(|error| format!("reading projection {}: {error}", output.name))
            })
            .collect()
    }

    fn assert_no_recovery_work_files(output_directory: &Path) -> Result<(), String> {
        for entry in fs::read_dir(output_directory)
            .map_err(|error| format!("reading output directory: {error}"))?
        {
            let entry = entry.map_err(|error| format!("reading output entry: {error}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".figma-rust-") && name != GENERATION_STORE && name != ASSET_CACHE {
                return Err(format!("recovery work file remains: {name}"));
            }
        }
        for entry in fs::read_dir(output_directory.join(GENERATION_STORE))
            .map_err(|error| format!("reading generation store: {error}"))?
        {
            let entry = entry.map_err(|error| format!("reading generation entry: {error}"))?;
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                return Err("temporary generation remains".to_owned());
            }
        }
        Ok(())
    }

    fn artifacts(value: &[u8]) -> Vec<OutputArtifact> {
        vec![
            OutputArtifact {
                name: "asset-manifest.json".to_owned(),
                content: [value, b"-manifest\n"].concat(),
                cache_key: None,
            },
            OutputArtifact {
                name: "generated.rs".to_owned(),
                content: [value, b"-generated\n"].concat(),
                cache_key: None,
            },
        ]
    }

    fn cached_artifacts(value: &[u8]) -> Vec<OutputArtifact> {
        let settings = BTreeMap::from([("format".to_owned(), "SVG".to_owned())]);
        let cache_key = asset_cache_key(value, "image/svg+xml", &settings);
        vec![
            OutputArtifact {
                name: "asset-manifest.json".to_owned(),
                content: b"manifest\n".to_vec(),
                cache_key: None,
            },
            OutputArtifact {
                name: "asset-a.svg".to_owned(),
                content: value.to_vec(),
                cache_key: Some(cache_key.clone()),
            },
            OutputArtifact {
                name: "asset-b.svg".to_owned(),
                content: value.to_vec(),
                cache_key: Some(cache_key),
            },
        ]
    }

    fn artifact_names(outputs: &[OutputArtifact]) -> BTreeSet<String> {
        outputs.iter().map(|output| output.name.clone()).collect()
    }

    fn unique_test_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "figma-rust-generation-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }
}
