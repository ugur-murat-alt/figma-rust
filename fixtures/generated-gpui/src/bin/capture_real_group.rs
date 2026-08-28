use std::{
    cell::Cell,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Cursor, Read as _, Write as _},
    path::{Component, Path, PathBuf},
    process::{self, ExitCode},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use figma_generated_gpui_fixture::generated_real_group;
use figma_gpui_runtime::{FallbackTokens, configure_figma_fidelity};
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, point, px, rgba, size,
};
use sha2::Digest as _;

const GPUI_REVISION: &str = "5631830c564afa89b3aba679f45d9c3345f9460f";
const EXTERNAL_LOADING_TITLE: &str = "figma-rust-real-group-loading";
const EXTERNAL_READY_TITLE: &str = "figma-rust-real-group-ready";
const EXTERNAL_WM_CLASS: &str = "figma-rust-real-group";
const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";
const MAX_DATA_URL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_ALLOC_BYTES: u64 = 1024 * 1024;
const WIDTH: u32 = 100;
const HEIGHT: u32 = 60;
const LOGICAL_WIDTH: f32 = 100.0;
const LOGICAL_HEIGHT: f32 = 60.0;

#[derive(Clone, Copy)]
enum Backdrop {
    Black,
    White,
}

impl Backdrop {
    fn parse(value: &OsString) -> Result<Self, String> {
        match value.to_str() {
            Some("black") => Ok(Self::Black),
            Some("white") => Ok(Self::White),
            Some(value) => Err(format!(
                "backdrop must be `black` or `white`, got {value:?}"
            )),
            None => Err("backdrop must be valid UTF-8".to_owned()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::White => "white",
        }
    }

    fn color(self) -> u32 {
        match self {
            Self::Black => 0x0000_00ff,
            Self::White => 0xffff_ffff,
        }
    }
}

enum CaptureCommand {
    Native {
        output: PathBuf,
    },
    Display {
        backdrop: Backdrop,
    },
    IngestDataUrl {
        output: PathBuf,
    },
    IngestGnomeDataUrl {
        output: PathBuf,
    },
    Reconstruct {
        black: PathBuf,
        white: PathBuf,
        output: PathBuf,
    },
    FinalizeProvenance {
        metadata: PathBuf,
        black: PathBuf,
        white: PathBuf,
        actual: PathBuf,
        manifest: PathBuf,
        report: PathBuf,
        source: String,
    },
}

struct CaptureView {
    output: Option<PathBuf>,
    succeeded: Rc<Cell<bool>>,
    backdrop: Option<Backdrop>,
    announce_ready: bool,
}

struct ManifestArtifacts {
    reference_geometry: PathBuf,
    actual_geometry: PathBuf,
    reference_image: PathBuf,
    actual_image: PathBuf,
}

struct CaptureLock {
    _directory: File,
}

impl CaptureLock {
    fn acquire(output: &Path) -> io::Result<Self> {
        let directory = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file = File::open(directory)?;
        file.try_lock()?;
        Ok(Self { _directory: file })
    }
}

impl Render for CaptureView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.announce_ready {
            self.announce_ready = false;
            let backdrop = self.backdrop.unwrap_or(Backdrop::Black);
            let ready = Rc::clone(&self.succeeded);
            window.on_next_frame(move |window, cx| {
                let viewport = window.viewport_size();
                let width = f32::from(viewport.width);
                let height = f32::from(viewport.height);
                if width != LOGICAL_WIDTH || height != LOGICAL_HEIGHT {
                    eprintln!(
                        "external GPUI capture viewport is {width}x{height}, expected {WIDTH}x{HEIGHT}"
                    );
                    cx.quit();
                    return;
                }

                window.activate_window();
                window.set_window_title(EXTERNAL_READY_TITLE);
                let event = serde_json::json!({
                    "schema_version": 1,
                    "event": "ready",
                    "pid": process::id(),
                    "window_title": EXTERNAL_READY_TITLE,
                    "wm_class": EXTERNAL_WM_CLASS,
                    "backdrop": backdrop.name(),
                    "logical_width": WIDTH,
                    "logical_height": HEIGHT,
                    "gpui_revision": GPUI_REVISION,
                    "text_rendering": "grayscale",
                });
                println!("{event}");
                if let Err(error) = io::stdout().flush() {
                    eprintln!("failed to publish capture readiness: {error}");
                    cx.quit();
                } else {
                    ready.set(true);
                }
            });
        }

        if let Some(output) = self.output.take() {
            let succeeded = Rc::clone(&self.succeeded);
            window.on_next_frame(move |window, cx| {
                capture_window(window, &output, &succeeded);
                cx.quit();
            });
        }

        let mut root = div()
            .relative()
            .w(px(LOGICAL_WIDTH))
            .h(px(LOGICAL_HEIGHT))
            .child(generated_real_group::generated_view(&FallbackTokens));
        if let Some(backdrop) = self.backdrop {
            root = root.bg(rgba(backdrop.color()));
        }
        root
    }
}

fn capture_window(window: &Window, output: &Path, succeeded: &Cell<bool>) {
    let viewport = window.viewport_size();
    let temporary = temporary_output_path(output);
    match window.render_to_image() {
        Ok(image) if image.width() == WIDTH && image.height() == HEIGHT => {
            match write_temporary_image(&image, &temporary) {
                Ok(()) => match fs::rename(&temporary, output) {
                    Ok(()) => {
                        eprintln!(
                            "captured GPUI {GPUI_REVISION} client image: {} ({}x{}, session={}, display={}, wayland={})",
                            output.display(),
                            image.width(),
                            image.height(),
                            environment("XDG_SESSION_TYPE"),
                            environment("DISPLAY"),
                            environment("WAYLAND_DISPLAY"),
                        );
                        succeeded.set(true);
                    }
                    Err(error) => {
                        eprintln!("failed to publish {}: {error}", output.display());
                        cleanup_temporary_capture(output);
                    }
                },
                Err(error) => {
                    eprintln!("failed to write {}: {error}", output.display());
                    cleanup_temporary_capture(output);
                }
            }
        }
        Ok(image) => {
            eprintln!(
                "GPUI capture returned {}x{} instead of {WIDTH}x{HEIGHT}",
                image.width(),
                image.height()
            );
            cleanup_temporary_capture(output);
        }
        Err(error) => {
            eprintln!(
                "GPUI {GPUI_REVISION} live-window capture is unavailable for a {}x{} client in session={} display={} wayland={}: {error}",
                f32::from(viewport.width),
                f32::from(viewport.height),
                environment("XDG_SESSION_TYPE"),
                environment("DISPLAY"),
                environment("WAYLAND_DISPLAY"),
            );
            cleanup_temporary_capture(output);
        }
    }
}

fn prepare_capture(output: &Path) -> io::Result<CaptureLock> {
    let lock = CaptureLock::acquire(output)?;
    clean_capture_paths(output)?;
    Ok(lock)
}

fn clean_capture_paths(output: &Path) -> io::Result<()> {
    let final_error = remove_file_if_present(output).err();
    let temporary_error = remove_stale_temporary_captures(output).err();
    match (final_error, temporary_error) {
        (None, None) => Ok(()),
        (Some(final_error), None) => Err(final_error),
        (None, Some(temporary_error)) => Err(temporary_error),
        (Some(final_error), Some(temporary_error)) => Err(io::Error::other(format!(
            "final cleanup failed: {final_error}; temporary cleanup failed: {temporary_error}"
        ))),
    }
}

fn remove_stale_temporary_captures(output: &Path) -> io::Result<()> {
    let directory = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let prefix = temporary_output_prefix(output);
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(prefix.as_encoded_bytes())
        {
            remove_file_if_present(&entry.path())?;
        }
    }
    Ok(())
}

fn cleanup_temporary_capture(output: &Path) {
    let temporary = temporary_output_path(output);
    if let Err(error) = remove_file_if_present(&temporary) {
        eprintln!(
            "failed to clean temporary capture {}: {error}",
            temporary.display()
        );
    }
}

fn write_temporary_image(image: &image::RgbaImage, temporary: &Path) -> Result<(), String> {
    let mut file = create_temporary_file(temporary).map_err(|error| error.to_string())?;
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut file),
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn create_temporary_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path)
        }
        Ok(_) => Err(io::Error::other(format!(
            "capture path is not a regular file or symlink: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn temporary_output_path(output: &Path) -> PathBuf {
    let mut name = temporary_output_prefix(output);
    name.push(process::id().to_string());
    output.with_file_name(name)
}

fn temporary_output_prefix(output: &Path) -> OsString {
    let mut name = OsString::from(".");
    name.push(output.file_name().unwrap_or_default());
    name.push(".tmp-");
    name
}

fn environment(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| "<unset>".to_owned())
}

fn parse_command(args: impl IntoIterator<Item = OsString>) -> Result<CaptureCommand, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(CaptureCommand::Native {
            output: PathBuf::from("fixtures/real-figma/actual.png"),
        });
    };

    let parsed = match command.to_str() {
        Some("--display") => CaptureCommand::Display {
            backdrop: Backdrop::parse(&required_arg(&mut args, "--display requires BACKDROP")?)?,
        },
        Some("--ingest-data-url") => CaptureCommand::IngestDataUrl {
            output: required_arg(&mut args, "--ingest-data-url requires OUTPUT")?.into(),
        },
        Some("--ingest-gnome-data-url") => CaptureCommand::IngestGnomeDataUrl {
            output: required_arg(&mut args, "--ingest-gnome-data-url requires OUTPUT")?.into(),
        },
        Some("--reconstruct") => CaptureCommand::Reconstruct {
            black: required_arg(&mut args, "--reconstruct requires BLACK WHITE OUTPUT")?.into(),
            white: required_arg(&mut args, "--reconstruct requires BLACK WHITE OUTPUT")?.into(),
            output: required_arg(&mut args, "--reconstruct requires BLACK WHITE OUTPUT")?.into(),
        },
        Some("--finalize-provenance") => CaptureCommand::FinalizeProvenance {
            metadata: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            black: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            white: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            actual: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            manifest: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            report: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into(),
            source: required_arg(
                &mut args,
                "--finalize-provenance requires METADATA BLACK WHITE ACTUAL MANIFEST REPORT SOURCE",
            )?
            .into_string()
            .map_err(|_| "capture source must be valid UTF-8".to_owned())?,
        },
        Some(command) if command.starts_with('-') => {
            return Err(format!("unknown capture command: {command}"));
        }
        _ => CaptureCommand::Native {
            output: command.into(),
        },
    };

    if args.next().is_some() {
        return Err("capture command received unexpected trailing arguments".to_owned());
    }
    Ok(parsed)
}

fn required_arg(
    args: &mut impl Iterator<Item = OsString>,
    message: &str,
) -> Result<OsString, String> {
    args.next().ok_or_else(|| message.to_owned())
}

fn ingest_data_url_from_stdin(output: &Path) -> Result<(), String> {
    let input = read_data_url_from_stdin()?;
    ingest_data_url(&input, output)
}

fn ingest_gnome_data_url_from_stdin(output: &Path) -> Result<(), String> {
    let input = read_data_url_from_stdin()?;
    ingest_gnome_data_url(&input, output)
}

fn read_data_url_from_stdin() -> Result<String, String> {
    let mut input = String::new();
    io::stdin()
        .take(MAX_DATA_URL_BYTES + 1)
        .read_to_string(&mut input)
        .map_err(|error| format!("failed to read PNG data URL from stdin: {error}"))?;
    if input.len() as u64 > MAX_DATA_URL_BYTES {
        return Err(format!(
            "PNG data URL exceeds the {MAX_DATA_URL_BYTES}-byte input limit"
        ));
    }
    Ok(input)
}

fn ingest_data_url(input: &str, output: &Path) -> Result<(), String> {
    let bytes = decode_data_url(input)?;
    let image = decode_capture_png(&bytes, "PNG data URL")?;
    validate_opaque_composite(&image, "PNG data URL")?;
    publish_ingested_image(output, &bytes, &image, None)
}

fn ingest_gnome_data_url(input: &str, output: &Path) -> Result<(), String> {
    let bytes = decode_data_url(input)?;
    let mut image = decode_capture_png(&bytes, "GNOME window PNG data URL")?;
    let (minimum_alpha, maximum_alpha) = image
        .pixels()
        .map(|pixel| pixel.0[3])
        .fold((u8::MAX, u8::MIN), |(minimum, maximum), alpha| {
            (minimum.min(alpha), maximum.max(alpha))
        });
    if minimum_alpha < 253 {
        return Err(format!(
            "GNOME opaque-window PNG alpha range must be within 253..=255, got {minimum_alpha}..={maximum_alpha}"
        ));
    }
    for pixel in image.pixels_mut() {
        pixel.0[3] = 255;
    }
    let normalized = encode_capture_png(&image)?;
    publish_ingested_image(
        output,
        &normalized,
        &image,
        Some((minimum_alpha, maximum_alpha)),
    )
}

fn decode_data_url(input: &str) -> Result<Vec<u8>, String> {
    let encoded = input
        .trim()
        .strip_prefix(PNG_DATA_URL_PREFIX)
        .ok_or_else(|| format!("capture input must start with {PNG_DATA_URL_PREFIX}"))?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("failed to decode PNG data URL: {error}"))
}

fn encode_capture_png(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut bytes),
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| format!("failed to encode normalized capture PNG: {error}"))?;
    Ok(bytes)
}

fn publish_ingested_image(
    output: &Path,
    bytes: &[u8],
    image: &image::RgbaImage,
    normalized_alpha_range: Option<(u8, u8)>,
) -> Result<(), String> {
    publish_bytes(output, bytes)?;

    let event = serde_json::json!({
        "schema_version": 1,
        "event": "ingested",
        "output": output,
        "bytes": bytes.len(),
        "width": image.width(),
        "height": image.height(),
        "sha256": sha256_hex(bytes),
        "normalized_alpha_range": normalized_alpha_range.map(|(minimum, maximum)| {
            serde_json::json!({ "minimum": minimum, "maximum": maximum })
        }),
    });
    println!("{event}");
    Ok(())
}

fn publish_bytes(output: &Path, bytes: &[u8]) -> Result<(), String> {
    let _lock = prepare_capture(output).map_err(|error| error.to_string())?;
    publish_bytes_locked(output, bytes)
}

fn publish_bytes_locked(output: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_output_path(output);
    let result = (|| {
        let mut file = create_temporary_file(&temporary).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, output).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        cleanup_temporary_capture(output);
    }
    result
}

fn reconstruct_external_capture(black: &Path, white: &Path, output: &Path) -> Result<(), String> {
    validate_reconstruction_paths(black, white, output)?;
    let _lock = prepare_capture(output).map_err(|error| error.to_string())?;
    let black = read_capture_image(black)?;
    let white = read_capture_image(white)?;
    let image = reconstruct_rgba(&black, &white)?;
    let temporary = temporary_output_path(output);
    if let Err(error) = write_temporary_image(&image, &temporary) {
        cleanup_temporary_capture(output);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, output) {
        cleanup_temporary_capture(output);
        return Err(error.to_string());
    }
    eprintln!(
        "reconstructed external GPUI capture: {} ({}x{})",
        output.display(),
        image.width(),
        image.height()
    );
    Ok(())
}

fn read_capture_image(path: &Path) -> Result<image::RgbaImage, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    decode_capture_png(&bytes, &path.display().to_string())
}

fn decode_capture_png(bytes: &[u8], label: &str) -> Result<image::RgbaImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("failed to inspect {label}: {error}"))?;
    if reader.format() != Some(image::ImageFormat::Png) {
        return Err(format!("external capture is not PNG: {label}"));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(WIDTH);
    limits.max_image_height = Some(HEIGHT);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC_BYTES);
    reader.limits(limits);
    reader
        .decode()
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| format!("failed to decode {label}: {error}"))
}

fn validate_opaque_composite(image: &image::RgbaImage, label: &str) -> Result<(), String> {
    if image.dimensions() != (WIDTH, HEIGHT) {
        return Err(format!(
            "{label} must be {WIDTH}x{HEIGHT}, got {:?}",
            image.dimensions()
        ));
    }
    if let Some((index, _)) = image
        .pixels()
        .enumerate()
        .find(|(_, pixel)| pixel[3] != u8::MAX)
    {
        return Err(format!(
            "{label} must be opaque at pixel ({}, {})",
            index % WIDTH as usize,
            index / WIDTH as usize
        ));
    }
    Ok(())
}

fn finalize_provenance(
    metadata: &Path,
    black: &Path,
    white: &Path,
    actual: &Path,
    manifest: &Path,
    report: &Path,
    source: &str,
) -> Result<(), String> {
    let engine = capture_engine(source)?;

    let black_name = artifact_name(metadata, black)?;
    let white_name = artifact_name(metadata, white)?;
    let actual_name = artifact_name(metadata, actual)?;
    let manifest_name = artifact_name(metadata, manifest)?;
    let report_name = artifact_name(metadata, report)?;
    let _lock = CaptureLock::acquire(metadata).map_err(|error| error.to_string())?;
    validate_metadata_path(metadata, black, white, actual, manifest, report)?;
    let manifest_bytes = read_artifact(manifest)?;
    let manifest_artifacts = manifest_artifacts(metadata, manifest, &manifest_bytes)?;
    validate_manifest_metadata_path(metadata, &manifest_artifacts)?;
    validate_reconstruction_paths(black, white, actual)?;
    let black_bytes = read_artifact(black)?;
    let white_bytes = read_artifact(white)?;
    let actual_bytes = read_artifact(actual)?;
    let black_image = decode_capture_png(&black_bytes, &black.display().to_string())?;
    let white_image = decode_capture_png(&white_bytes, &white.display().to_string())?;
    let actual_image = decode_capture_png(&actual_bytes, &actual.display().to_string())?;
    validate_opaque_composite(&black_image, &black.display().to_string())?;
    validate_opaque_composite(&white_image, &white.display().to_string())?;
    let reconstructed = reconstruct_rgba(&black_image, &white_image)?;
    if reconstructed.as_raw() != actual_image.as_raw() {
        return Err("actual PNG does not match the dual-backdrop reconstruction".to_owned());
    }

    validate_report_freshness(actual, report)?;
    let expected_inputs =
        verification_inputs(&manifest_bytes, &manifest_artifacts, actual, &actual_bytes)?;
    let report_bytes = read_artifact(report)?;
    let verification: serde_json::Value = serde_json::from_slice(&report_bytes)
        .map_err(|error| format!("failed to parse {}: {error}", report.display()))?;
    validate_verification_report(&verification, &expected_inputs)?;

    let metadata_value = serde_json::json!({
        "schema_version": 2,
        "captured_at_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis(),
        "engine": engine,
        "source": source,
        "capture_normalization": match source {
            "gnome-shell-screenshot" => "opaque-window-alpha-253-through-255-to-opaque-v1",
            _ => "none",
        },
        "session_type": environment("XDG_SESSION_TYPE"),
        "wayland_display": environment("WAYLAND_DISPLAY"),
        "gpui_revision": GPUI_REVISION,
        "text_rendering": "grayscale",
        "window": {
            "title": EXTERNAL_READY_TITLE,
            "wm_class": EXTERNAL_WM_CLASS,
            "coordinate_width": WIDTH,
            "coordinate_height": HEIGHT,
            "payload_width": WIDTH,
            "payload_height": HEIGHT,
            "scale": 1.0,
        },
        "backdrops": {
            "black": {
                "path": black_name,
                "sha256": sha256_hex(&black_bytes),
            },
            "white": {
                "path": white_name,
                "sha256": sha256_hex(&white_bytes),
            },
        },
        "reconstruction": "dual-backdrop-straight-alpha-v1",
        "actual": {
            "path": actual_name,
            "sha256": sha256_hex(&actual_bytes),
        },
        "verification": {
            "manifest": manifest_name,
            "report": report_name,
            "result": verification,
        },
    });
    let mut bytes = serde_json::to_vec_pretty(&metadata_value)
        .map_err(|error| format!("failed to serialize capture provenance: {error}"))?;
    bytes.push(b'\n');
    remove_stale_temporary_captures(metadata).map_err(|error| error.to_string())?;
    publish_bytes_locked(metadata, &bytes)?;

    let event = serde_json::json!({
        "schema_version": 1,
        "event": "finalized",
        "metadata": metadata,
        "actual_sha256": sha256_hex(&actual_bytes),
    });
    println!("{event}");
    Ok(())
}

fn capture_engine(source: &str) -> Result<&'static str, String> {
    match source {
        "xdg-desktop-portal" => Ok("computer-use-linux"),
        "gnome-shell-screenshot" => Ok("figma-rust-linux-capture"),
        _ => Err(format!(
            "capture source must be `xdg-desktop-portal` or `gnome-shell-screenshot`, got {source:?}"
        )),
    }
}

fn validate_metadata_path(
    metadata: &Path,
    black: &Path,
    white: &Path,
    actual: &Path,
    manifest: &Path,
    report: &Path,
) -> Result<(), String> {
    for (label, artifact) in [
        ("BLACK", black),
        ("WHITE", white),
        ("ACTUAL", actual),
        ("MANIFEST", manifest),
        ("REPORT", report),
    ] {
        if paths_refer_to_same_file(artifact, metadata)? {
            return Err(format!(
                "METADATA and {label} must refer to different files: {}",
                artifact.display()
            ));
        }
    }
    Ok(())
}

fn validate_manifest_metadata_path(
    metadata: &Path,
    artifacts: &ManifestArtifacts,
) -> Result<(), String> {
    for (label, artifact) in [
        ("REFERENCE_GEOMETRY", artifacts.reference_geometry.as_path()),
        ("ACTUAL_GEOMETRY", artifacts.actual_geometry.as_path()),
        ("REFERENCE_IMAGE", artifacts.reference_image.as_path()),
        ("ACTUAL_IMAGE", artifacts.actual_image.as_path()),
    ] {
        if paths_refer_to_same_file(artifact, metadata)? {
            return Err(format!(
                "METADATA and {label} must refer to different files: {}",
                artifact.display()
            ));
        }
    }
    Ok(())
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn validate_verification_report(
    report: &serde_json::Value,
    expected_inputs: &serde_json::Value,
) -> Result<(), String> {
    if report.get("passed").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err("verification report did not pass".to_owned());
    }
    if report.get("inputs") != Some(expected_inputs) {
        return Err("verification report inputs do not match the manifest artifacts".to_owned());
    }
    for field in ["geometry_differences", "failures"] {
        if !report
            .get(field)
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err(format!("verification report field {field:?} is not empty"));
        }
    }
    if !report
        .get("image_metrics")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err("verification report has no image_metrics object".to_owned());
    }
    Ok(())
}

fn verification_inputs(
    manifest_bytes: &[u8],
    artifacts: &ManifestArtifacts,
    actual: &Path,
    actual_bytes: &[u8],
) -> Result<serde_json::Value, String> {
    if !paths_refer_to_same_file(actual, &artifacts.actual_image)? {
        return Err(format!(
            "verification manifest actual_image does not refer to {}",
            actual.display()
        ));
    }
    let reference_geometry = read_artifact(&artifacts.reference_geometry)?;
    let actual_geometry = read_artifact(&artifacts.actual_geometry)?;
    let reference_image = read_artifact(&artifacts.reference_image)?;
    Ok(serde_json::json!({
        "manifest": sha256_hex(manifest_bytes),
        "reference_geometry": sha256_hex(&reference_geometry),
        "actual_geometry": sha256_hex(&actual_geometry),
        "reference_image": sha256_hex(&reference_image),
        "actual_image": sha256_hex(actual_bytes),
    }))
}

fn manifest_artifacts(
    metadata: &Path,
    manifest: &Path,
    bytes: &[u8],
) -> Result<ManifestArtifacts, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("failed to parse {}: {error}", manifest.display()))?;
    Ok(ManifestArtifacts {
        reference_geometry: manifest_artifact_path(
            metadata,
            manifest,
            &value,
            "reference_geometry",
        )?,
        actual_geometry: manifest_artifact_path(metadata, manifest, &value, "actual_geometry")?,
        reference_image: manifest_artifact_path(metadata, manifest, &value, "reference_image")?,
        actual_image: manifest_artifact_path(metadata, manifest, &value, "actual_image")?,
    })
}

fn manifest_artifact_path(
    metadata: &Path,
    manifest: &Path,
    value: &serde_json::Value,
    field: &str,
) -> Result<PathBuf, String> {
    let raw = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("verification manifest has no {field} path"))?;
    let path = Path::new(raw);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "verification manifest {field} must be one same-directory file name"
        ));
    }
    let path = manifest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(path);
    artifact_name(metadata, &path)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "failed to inspect manifest {field} {}: {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "verification manifest {field} must refer to a regular file, not a symlink or special path: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn validate_report_freshness(actual: &Path, report: &Path) -> Result<(), String> {
    let actual_modified = fs::metadata(actual)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("failed to read {} timestamp: {error}", actual.display()))?;
    let report_modified = fs::metadata(report)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("failed to read {} timestamp: {error}", report.display()))?;
    if report_modified < actual_modified {
        return Err("verification report is older than the actual PNG".to_owned());
    }
    Ok(())
}

fn artifact_name(metadata: &Path, artifact: &Path) -> Result<String, String> {
    let metadata_parent = metadata
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let artifact_parent = artifact
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let metadata_directory = fs::canonicalize(metadata_parent).map_err(|error| {
        format!(
            "failed to resolve metadata directory {}: {error}",
            metadata_parent.display()
        )
    })?;
    let artifact_directory = fs::canonicalize(artifact_parent).map_err(|error| {
        format!(
            "failed to resolve artifact directory {}: {error}",
            artifact_parent.display()
        )
    })?;
    if metadata_directory != artifact_directory {
        return Err(format!(
            "capture artifact must share the metadata directory: {}",
            artifact.display()
        ));
    }
    artifact
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "capture artifact has no UTF-8 file name: {}",
                artifact.display()
            )
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let digest = sha2::Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_reconstruction_paths(black: &Path, white: &Path, output: &Path) -> Result<(), String> {
    for (left_label, left, right_label, right) in [
        ("BLACK", black, "WHITE", white),
        ("BLACK", black, "OUTPUT", output),
        ("WHITE", white, "OUTPUT", output),
    ] {
        if paths_refer_to_same_file(left, right)? {
            return Err(format!(
                "{left_label} and {right_label} must refer to different files: {}",
                left.display()
            ));
        }
    }
    Ok(())
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> Result<bool, String> {
    let left_metadata = fs::metadata(left)
        .map_err(|error| format!("failed to inspect {}: {error}", left.display()))?;
    let right_metadata = match fs::metadata(right) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!("failed to inspect {}: {error}", right.display()));
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;

        Ok(left_metadata.dev() == right_metadata.dev()
            && left_metadata.ino() == right_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = (left_metadata, right_metadata);
        let left = fs::canonicalize(left)
            .map_err(|error| format!("failed to resolve {}: {error}", left.display()))?;
        let right = fs::canonicalize(right)
            .map_err(|error| format!("failed to resolve {}: {error}", right.display()))?;
        Ok(left == right)
    }
}

fn reconstruct_rgba(
    black: &image::RgbaImage,
    white: &image::RgbaImage,
) -> Result<image::RgbaImage, String> {
    if black.dimensions() != (WIDTH, HEIGHT) || white.dimensions() != (WIDTH, HEIGHT) {
        return Err(format!(
            "external captures must both be {WIDTH}x{HEIGHT}, got black={:?} white={:?}",
            black.dimensions(),
            white.dimensions()
        ));
    }

    let mut output = image::RgbaImage::new(WIDTH, HEIGHT);
    for (index, ((black, white), output)) in black
        .pixels()
        .zip(white.pixels())
        .zip(output.pixels_mut())
        .enumerate()
    {
        if black[3] != u8::MAX || white[3] != u8::MAX {
            return Err(format!(
                "external composites must be opaque at pixel ({}, {})",
                index % WIDTH as usize,
                index / WIDTH as usize
            ));
        }
        let mut backdrop = [0_u8; 3];
        for channel in 0..3 {
            backdrop[channel] = white[channel].checked_sub(black[channel]).ok_or_else(|| {
                format!(
                    "white composite is darker than black at pixel ({}, {}) channel {channel}",
                    index % WIDTH as usize,
                    index / WIDTH as usize
                )
            })?;
        }
        backdrop.sort_unstable();
        if backdrop[2] - backdrop[0] > 2 {
            return Err(format!(
                "inconsistent backdrop delta at pixel ({}, {}): {:?}",
                index % WIDTH as usize,
                index / WIDTH as usize,
                backdrop
            ));
        }

        let alpha = u8::MAX - backdrop[1];
        let mut straight = [0_u8; 4];
        straight[3] = alpha;
        if alpha != 0 {
            for channel in 0..3 {
                let value = ((u32::from(black[channel]) * u32::from(u8::MAX)
                    + u32::from(alpha) / 2)
                    / u32::from(alpha))
                .min(u32::from(u8::MAX));
                straight[channel] = u8::try_from(value).map_err(|error| {
                    format!("failed to reconstruct channel at pixel {index}: {error}")
                })?;
            }
        }
        *output = image::Rgba(straight);
    }
    Ok(output)
}

fn main() -> ExitCode {
    let command = match parse_command(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("failed to configure capture: {error}");
            return ExitCode::from(2);
        }
    };

    match command {
        CaptureCommand::Native { output } => run_window_capture(Some(output), None),
        CaptureCommand::Display { backdrop } => run_window_capture(None, Some(backdrop)),
        CaptureCommand::IngestDataUrl { output } => finish_command(
            ingest_data_url_from_stdin(&output),
            "failed to ingest external capture",
        ),
        CaptureCommand::IngestGnomeDataUrl { output } => finish_command(
            ingest_gnome_data_url_from_stdin(&output),
            "failed to ingest GNOME window capture",
        ),
        CaptureCommand::Reconstruct {
            black,
            white,
            output,
        } => finish_command(
            reconstruct_external_capture(&black, &white, &output),
            "failed to reconstruct external capture",
        ),
        CaptureCommand::FinalizeProvenance {
            metadata,
            black,
            white,
            actual,
            manifest,
            report,
            source,
        } => finish_command(
            finalize_provenance(
                &metadata, &black, &white, &actual, &manifest, &report, &source,
            ),
            "failed to finalize capture provenance",
        ),
    }
}

fn finish_command(result: Result<(), String>, context: &str) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{context}: {error}");
            ExitCode::from(2)
        }
    }
}

fn run_window_capture(output: Option<PathBuf>, backdrop: Option<Backdrop>) -> ExitCode {
    let _lock = if backdrop.is_some() {
        None
    } else {
        let output = output
            .as_deref()
            .unwrap_or_else(|| Path::new("fixtures/real-figma/actual.png"));
        match prepare_capture(output) {
            Ok(lock) => Some(lock),
            Err(error) => {
                eprintln!(
                    "failed to prepare capture output {}: {error}",
                    output.display()
                );
                return ExitCode::from(2);
            }
        }
    };
    let succeeded = Rc::new(Cell::new(false));
    let app_succeeded = Rc::clone(&succeeded);
    let external_capture = backdrop.is_some();

    gpui_platform::application().run(move |cx: &mut App| {
        configure_figma_fidelity(cx);
        let view_succeeded = Rc::clone(&app_succeeded);
        let open_result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(LOGICAL_WIDTH), px(LOGICAL_HEIGHT)),
                })),
                focus: external_capture,
                show: true,
                window_background: if external_capture {
                    WindowBackgroundAppearance::Opaque
                } else {
                    WindowBackgroundAppearance::Transparent
                },
                ..WindowOptions::default()
            },
            move |window, cx| {
                if external_capture {
                    window.set_app_id(EXTERNAL_WM_CLASS);
                    window.set_window_title(EXTERNAL_LOADING_TITLE);
                }
                cx.new(|_| CaptureView {
                    output,
                    succeeded: view_succeeded,
                    backdrop,
                    announce_ready: external_capture,
                })
            },
        );

        if let Err(error) = open_result {
            eprintln!("failed to open the real GPUI compositor window: {error}");
            cx.quit();
        }
    });

    window_capture_exit_code(succeeded.get())
}

fn window_capture_exit_code(succeeded: bool) -> ExitCode {
    if succeeded {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::ErrorKind, process};

    use base64::Engine as _;

    use super::{
        CaptureCommand, capture_engine, cleanup_temporary_capture, create_temporary_file,
        finalize_provenance, ingest_data_url, ingest_gnome_data_url, manifest_artifacts,
        parse_command, prepare_capture, read_capture_image, reconstruct_rgba,
        temporary_output_path, temporary_output_prefix, validate_metadata_path,
        validate_reconstruction_paths, verification_inputs, window_capture_exit_code,
    };

    fn test_output(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("figma-rust-capture-{name}-{}", process::id()));
        if let Err(error) = fs::create_dir_all(&directory) {
            panic!("capture test directory must be created: {error}");
        }
        directory.join("actual.png")
    }

    fn remove_test_directory(output: &std::path::Path) {
        if let Some(directory) = output.parent()
            && let Err(error) = fs::remove_dir_all(directory)
            && error.kind() != ErrorKind::NotFound
        {
            panic!("capture test directory must be removed: {error}");
        }
    }

    fn stale_temporary_path(output: &std::path::Path, suffix: &str) -> std::path::PathBuf {
        let mut name = temporary_output_prefix(output);
        name.push(suffix);
        output.with_file_name(name)
    }

    fn encode_png(image: &image::RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut bytes),
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap_or_else(|error| panic!("PNG fixture must encode: {error}"));
        bytes
    }

    struct ProvenanceFixture {
        metadata: std::path::PathBuf,
        black: std::path::PathBuf,
        white: std::path::PathBuf,
        actual: std::path::PathBuf,
        manifest: std::path::PathBuf,
        report: std::path::PathBuf,
        report_value: serde_json::Value,
    }

    impl ProvenanceFixture {
        fn new(name: &str) -> Self {
            let metadata = test_output(name).with_file_name("capture.compositor.json");
            let directory = metadata
                .parent()
                .unwrap_or_else(|| panic!("metadata fixture must have a parent"));
            let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../real-figma");
            let black = directory.join("capture.black.png");
            let white = directory.join("capture.white.png");
            let actual = directory.join("actual.png");
            for (source, target) in [
                (fixture.join("capture.black.png"), &black),
                (fixture.join("capture.white.png"), &white),
                (fixture.join("actual.png"), &actual),
            ] {
                if let Err(error) = fs::copy(source, target) {
                    panic!("capture fixture must be copied: {error}");
                }
            }

            let reference = directory.join("reference.png");
            if let Err(error) = fs::copy(&actual, reference) {
                panic!("reference fixture must be copied: {error}");
            }
            for (name, contents) in [
                ("reference.geometry.json", b"reference geometry".as_slice()),
                ("actual.geometry.json", b"actual geometry".as_slice()),
            ] {
                if let Err(error) = fs::write(directory.join(name), contents) {
                    panic!("geometry fixture must be written: {error}");
                }
            }

            let manifest = directory.join("verify.image.json");
            if let Err(error) = fs::write(
                &manifest,
                br#"{"reference_geometry":"reference.geometry.json","actual_geometry":"actual.geometry.json","reference_image":"reference.png","actual_image":"actual.png"}"#,
            ) {
                panic!("manifest fixture must be written: {error}");
            }
            let manifest_bytes = fs::read(&manifest)
                .unwrap_or_else(|error| panic!("manifest fixture must be read: {error}"));
            let actual_bytes = fs::read(&actual)
                .unwrap_or_else(|error| panic!("actual fixture must be read: {error}"));
            let artifacts = manifest_artifacts(&metadata, &manifest, &manifest_bytes)
                .unwrap_or_else(|error| panic!("manifest artifacts must resolve: {error}"));
            let inputs = verification_inputs(&manifest_bytes, &artifacts, &actual, &actual_bytes)
                .unwrap_or_else(|error| panic!("verification inputs must hash: {error}"));
            let report_value = serde_json::json!({
                "passed": true,
                "inputs": inputs,
                "geometry_differences": [],
                "image_metrics": {
                    "mean_absolute_error": 0.0,
                    "changed_pixel_ratio": 0.0,
                    "edge_error": 0.0,
                    "ssim": 1.0,
                },
                "failures": [],
            });
            let report = directory.join("capture.verify.json");
            if let Err(error) = fs::write(&report, report_value.to_string()) {
                panic!("verification report fixture must be written: {error}");
            }

            Self {
                metadata,
                black,
                white,
                actual,
                manifest,
                report,
                report_value,
            }
        }

        fn finalize(&self, metadata: &std::path::Path) -> Result<(), String> {
            finalize_provenance(
                metadata,
                &self.black,
                &self.white,
                &self.actual,
                &self.manifest,
                &self.report,
                "xdg-desktop-portal",
            )
        }
    }

    #[test]
    fn preparation_removes_stale_final_and_all_process_temporary_files() {
        let output = test_output("cleanup");
        let temporary = stale_temporary_path(&output, "previous-process");
        if let Err(error) = fs::write(&output, b"stale") {
            panic!("stale capture fixture must be written: {error}");
        }
        if let Err(error) = fs::write(&temporary, b"partial") {
            panic!("temporary capture fixture must be written: {error}");
        }

        let lock = prepare_capture(&output);
        if let Err(error) = &lock {
            panic!("capture paths must be cleaned: {error}");
        }

        assert!(!output.exists());
        assert!(!temporary.exists());
        drop(lock);
        remove_test_directory(&output);
    }

    #[test]
    fn temporary_creation_never_overwrites_an_existing_path() {
        let output = test_output("exclusive");
        let temporary = temporary_output_path(&output);
        let lock = prepare_capture(&output)
            .unwrap_or_else(|error| panic!("capture paths must start clean: {error}"));
        if let Err(error) = fs::write(&temporary, b"replacement") {
            panic!("replacement fixture must be written: {error}");
        }

        let error = create_temporary_file(&temporary).err();
        assert!(error.is_some_and(|error| error.kind() == ErrorKind::AlreadyExists));
        assert_eq!(
            fs::read(&temporary).ok().as_deref(),
            Some(b"replacement".as_slice())
        );

        cleanup_temporary_capture(&output);
        drop(lock);
        remove_test_directory(&output);
    }

    #[test]
    fn capture_lock_rejects_a_second_writer_without_deleting_the_final() {
        let output = test_output("lock");
        let lock = prepare_capture(&output)
            .unwrap_or_else(|error| panic!("first capture must acquire the lock: {error}"));
        if let Err(error) = fs::write(&output, b"published") {
            panic!("published fixture must be written: {error}");
        }

        let error = prepare_capture(&output).err();
        assert!(error.is_some_and(|error| error.kind() == ErrorKind::WouldBlock));
        assert_eq!(
            fs::read(&output).ok().as_deref(),
            Some(b"published".as_slice())
        );

        drop(lock);
        remove_test_directory(&output);
    }

    #[test]
    fn failure_cleanup_preserves_a_concurrently_published_final() {
        let output = test_output("concurrent");
        let temporary = temporary_output_path(&output);
        let lock = prepare_capture(&output)
            .unwrap_or_else(|error| panic!("capture paths must start clean: {error}"));
        if let Err(error) = fs::write(&output, b"published") {
            panic!("published fixture must be written: {error}");
        }
        if let Err(error) = fs::write(&temporary, b"partial") {
            panic!("partial fixture must be written: {error}");
        }

        cleanup_temporary_capture(&output);

        assert_eq!(
            fs::read(&output).ok().as_deref(),
            Some(b"published".as_slice())
        );
        assert!(!temporary.exists());
        drop(lock);
        remove_test_directory(&output);
    }

    #[cfg(unix)]
    #[test]
    fn preparation_removes_symlinks_without_touching_their_target() {
        use std::os::unix::fs::symlink;

        let output = test_output("symlink");
        let temporary = stale_temporary_path(&output, "old-pid");
        let target = output.with_file_name("target.png");
        if let Err(error) = fs::write(&target, b"target") {
            panic!("symlink target fixture must be written: {error}");
        }
        if let Err(error) = symlink(&target, &output) {
            panic!("final symlink fixture must be created: {error}");
        }
        if let Err(error) = symlink(&target, &temporary) {
            panic!("temporary symlink fixture must be created: {error}");
        }

        let lock = prepare_capture(&output)
            .unwrap_or_else(|error| panic!("capture symlinks must be cleaned: {error}"));

        assert!(fs::symlink_metadata(&output).is_err());
        assert!(fs::symlink_metadata(&temporary).is_err());
        assert_eq!(
            fs::read(&target).ok().as_deref(),
            Some(b"target".as_slice())
        );
        drop(lock);
        remove_test_directory(&output);
    }

    #[test]
    fn dual_backdrop_reconstruction_restores_transparent_and_opaque_pixels() {
        let black = image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 32, 64, 128, 255])
            .unwrap_or_else(|| panic!("black composite fixture must be valid"));
        let white = image::RgbaImage::from_raw(2, 1, vec![255, 255, 255, 255, 32, 64, 128, 255])
            .unwrap_or_else(|| panic!("white composite fixture must be valid"));

        let error = reconstruct_rgba(&black, &white).err();
        assert!(error.is_some_and(|error| error.contains("100x60")));

        let mut black =
            image::RgbaImage::from_pixel(super::WIDTH, super::HEIGHT, image::Rgba([0, 0, 0, 255]));
        let mut white = image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([255, 255, 255, 255]),
        );
        black.put_pixel(0, 0, image::Rgba([32, 64, 128, 255]));
        white.put_pixel(0, 0, image::Rgba([32, 64, 128, 255]));

        let image = reconstruct_rgba(&black, &white)
            .unwrap_or_else(|error| panic!("composites must reconstruct: {error}"));
        assert_eq!(image.get_pixel(0, 0).0, [32, 64, 128, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 0]);
    }

    #[test]
    fn reconstruction_rejects_input_output_aliases_without_deleting_inputs() {
        let output = test_output("reconstruction-alias");
        let black = output.with_file_name("black.png");
        let white = output.with_file_name("white.png");
        if let Err(error) = fs::write(&black, b"black") {
            panic!("black alias fixture must be written: {error}");
        }
        if let Err(error) = fs::write(&white, b"white") {
            panic!("white alias fixture must be written: {error}");
        }

        let error = validate_reconstruction_paths(&black, &white, &black).err();
        assert!(error.is_some_and(|error| error.contains("BLACK and OUTPUT")));
        let error = validate_reconstruction_paths(&black, &white, &white).err();
        assert!(error.is_some_and(|error| error.contains("WHITE and OUTPUT")));
        assert_eq!(fs::read(&black).ok().as_deref(), Some(b"black".as_slice()));
        assert_eq!(fs::read(&white).ok().as_deref(), Some(b"white".as_slice()));

        remove_test_directory(&output);
    }

    #[test]
    fn reconstruction_rejects_non_png_content() {
        let output = test_output("non-png");
        let jpeg = output.with_file_name("capture.png");
        if let Err(error) = fs::write(&jpeg, [0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0]) {
            panic!("non-PNG fixture must be written: {error}");
        }

        let error = read_capture_image(&jpeg).err();
        assert!(error.is_some_and(|error| error.contains("not PNG")));

        remove_test_directory(&output);
    }

    #[cfg(unix)]
    #[test]
    fn reconstruction_rejects_hardlink_and_symlink_output_aliases() {
        use std::os::unix::fs::symlink;

        let output = test_output("reconstruction-link-alias");
        let black = output.with_file_name("black.png");
        let white = output.with_file_name("white.png");
        if let Err(error) = fs::write(&black, b"black") {
            panic!("black link fixture must be written: {error}");
        }
        if let Err(error) = fs::write(&white, b"white") {
            panic!("white link fixture must be written: {error}");
        }
        if let Err(error) = fs::hard_link(&black, &output) {
            panic!("hardlink output fixture must be created: {error}");
        }
        let error = validate_reconstruction_paths(&black, &white, &output).err();
        assert!(error.is_some_and(|error| error.contains("BLACK and OUTPUT")));
        if let Err(error) = fs::remove_file(&output) {
            panic!("hardlink output fixture must be removed: {error}");
        }

        if let Err(error) = symlink(&white, &output) {
            panic!("symlink output fixture must be created: {error}");
        }
        let error = validate_reconstruction_paths(&black, &white, &output).err();
        assert!(error.is_some_and(|error| error.contains("WHITE and OUTPUT")));
        assert_eq!(fs::read(&black).ok().as_deref(), Some(b"black".as_slice()));
        assert_eq!(fs::read(&white).ok().as_deref(), Some(b"white".as_slice()));

        remove_test_directory(&output);
    }

    #[test]
    fn reconstruction_rejects_invalid_composites() {
        let mut black =
            image::RgbaImage::from_pixel(super::WIDTH, super::HEIGHT, image::Rgba([0, 0, 0, 255]));
        let mut white = image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([255, 255, 255, 255]),
        );

        black.put_pixel(0, 0, image::Rgba([0, 0, 0, 254]));
        let error = reconstruct_rgba(&black, &white).err();
        assert!(error.is_some_and(|error| error.contains("must be opaque")));

        black.put_pixel(0, 0, image::Rgba([1, 0, 0, 255]));
        white.put_pixel(0, 0, image::Rgba([0, 255, 255, 255]));
        let error = reconstruct_rgba(&black, &white).err();
        assert!(error.is_some_and(|error| error.contains("darker than black")));

        black.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        white.put_pixel(0, 0, image::Rgba([255, 250, 255, 255]));
        let error = reconstruct_rgba(&black, &white).err();
        assert!(error.is_some_and(|error| error.contains("inconsistent backdrop delta")));
    }

    #[test]
    fn checked_in_portal_composites_reconstruct_the_reference_pixels() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../real-figma");
        let black = read_capture_image(&fixture.join("capture.black.png"))
            .unwrap_or_else(|error| panic!("black portal fixture must decode: {error}"));
        let white = read_capture_image(&fixture.join("capture.white.png"))
            .unwrap_or_else(|error| panic!("white portal fixture must decode: {error}"));
        let reference = read_capture_image(&fixture.join("reference.png"))
            .unwrap_or_else(|error| panic!("reference fixture must decode: {error}"));

        let reconstructed = reconstruct_rgba(&black, &white)
            .unwrap_or_else(|error| panic!("portal fixtures must reconstruct: {error}"));
        assert_eq!(reconstructed.as_raw(), reference.as_raw());
    }

    #[test]
    fn command_parser_exposes_explicit_capture_modes() {
        let display = parse_command(["--display".into(), "black".into()])
            .unwrap_or_else(|error| panic!("display command must parse: {error}"));
        assert!(matches!(display, CaptureCommand::Display { .. }));

        let ingest = parse_command(["--ingest-data-url".into(), "capture.png".into()])
            .unwrap_or_else(|error| panic!("ingest command must parse: {error}"));
        assert!(matches!(ingest, CaptureCommand::IngestDataUrl { .. }));

        let gnome_ingest = parse_command(["--ingest-gnome-data-url".into(), "capture.png".into()])
            .unwrap_or_else(|error| panic!("GNOME ingest command must parse: {error}"));
        assert!(matches!(
            gnome_ingest,
            CaptureCommand::IngestGnomeDataUrl { .. }
        ));

        let reconstruct = parse_command([
            "--reconstruct".into(),
            "black.png".into(),
            "white.png".into(),
            "actual.png".into(),
        ])
        .unwrap_or_else(|error| panic!("reconstruct command must parse: {error}"));
        assert!(matches!(reconstruct, CaptureCommand::Reconstruct { .. }));

        let error = parse_command(["--display".into(), "white".into(), "extra".into()]).err();
        assert!(error.is_some_and(|error| error.contains("unexpected trailing")));
    }

    #[test]
    fn provenance_accepts_only_declared_capture_adapters() {
        assert_eq!(
            capture_engine("xdg-desktop-portal"),
            Ok("computer-use-linux")
        );
        assert_eq!(
            capture_engine("gnome-shell-screenshot"),
            Ok("figma-rust-linux-capture")
        );
        let error = capture_engine("desktop-crop").err();
        assert!(error.is_some_and(|error| error.contains("must be")));
    }

    #[test]
    fn data_url_ingest_validates_then_publishes_exact_portal_bytes() {
        let output = test_output("data-url-ingest");
        let image = image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([20, 40, 80, 255]),
        );
        let bytes = encode_png(&image);
        let data_url = format!(
            "{}{}",
            super::PNG_DATA_URL_PREFIX,
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );

        ingest_data_url(&data_url, &output)
            .unwrap_or_else(|error| panic!("valid data URL must publish: {error}"));

        assert_eq!(fs::read(&output).ok().as_deref(), Some(bytes.as_slice()));
        remove_test_directory(&output);
    }

    #[test]
    fn gnome_data_url_ingest_normalizes_only_near_opaque_window_alpha() {
        let output = test_output("gnome-data-url-ingest");
        let image = image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([20, 40, 80, 254]),
        );
        let bytes = encode_png(&image);
        let data_url = format!(
            "{}{}",
            super::PNG_DATA_URL_PREFIX,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );

        ingest_gnome_data_url(&data_url, &output)
            .unwrap_or_else(|error| panic!("uniform GNOME alpha must normalize: {error}"));
        let normalized = read_capture_image(&output)
            .unwrap_or_else(|error| panic!("normalized GNOME PNG must decode: {error}"));
        assert!(
            normalized
                .pixels()
                .all(|pixel| pixel.0 == [20, 40, 80, 255])
        );

        let mut invalid = image;
        invalid.put_pixel(0, 0, image::Rgba([20, 40, 80, 252]));
        let invalid_url = format!(
            "{}{}",
            super::PNG_DATA_URL_PREFIX,
            base64::engine::general_purpose::STANDARD.encode(encode_png(&invalid))
        );
        let before = fs::read(&output).unwrap_or_else(|error| panic!("output must exist: {error}"));
        let error = ingest_gnome_data_url(&invalid_url, &output).err();
        assert!(error.is_some_and(|error| error.contains("alpha range")));
        assert_eq!(fs::read(&output).ok().as_deref(), Some(before.as_slice()));
        remove_test_directory(&output);
    }

    #[test]
    fn invalid_data_urls_preserve_the_previous_final() {
        let output = test_output("invalid-data-url");
        if let Err(error) = fs::write(&output, b"published") {
            panic!("previous final must be written: {error}");
        }

        let valid = encode_png(&image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([0, 0, 0, 255]),
        ));
        let truncated = &valid[..valid.len() / 2];
        let wrong_size = encode_png(&image::RgbaImage::from_pixel(
            super::WIDTH + 1,
            super::HEIGHT,
            image::Rgba([0, 0, 0, 255]),
        ));
        let non_opaque = encode_png(&image::RgbaImage::from_pixel(
            super::WIDTH,
            super::HEIGHT,
            image::Rgba([0, 0, 0, 254]),
        ));
        let invalid = [
            "data:image/jpeg;base64,AAAA".to_owned(),
            format!(
                "{}{}",
                super::PNG_DATA_URL_PREFIX,
                base64::engine::general_purpose::STANDARD
                    .encode([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0])
            ),
            format!(
                "{}{}",
                super::PNG_DATA_URL_PREFIX,
                base64::engine::general_purpose::STANDARD.encode(truncated)
            ),
            format!(
                "{}{}",
                super::PNG_DATA_URL_PREFIX,
                base64::engine::general_purpose::STANDARD.encode(&wrong_size)
            ),
            format!(
                "{}{}",
                super::PNG_DATA_URL_PREFIX,
                base64::engine::general_purpose::STANDARD.encode(&non_opaque)
            ),
        ];
        for data_url in invalid {
            assert!(ingest_data_url(&data_url, &output).is_err());
            assert_eq!(
                fs::read(&output).ok().as_deref(),
                Some(b"published".as_slice())
            );
            assert!(!temporary_output_path(&output).exists());
        }

        remove_test_directory(&output);
    }

    #[test]
    fn provenance_requires_reconstruction_and_passing_verification() {
        let fixture = ProvenanceFixture::new("provenance");
        fixture
            .finalize(&fixture.metadata)
            .unwrap_or_else(|error| panic!("valid provenance must publish: {error}"));

        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&fixture.metadata)
                .unwrap_or_else(|error| panic!("metadata fixture must be read: {error}")),
        )
        .unwrap_or_else(|error| panic!("metadata fixture must parse: {error}"));
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["verification"]["result"]["passed"], true);
        assert_eq!(value["backdrops"]["black"]["path"], "capture.black.png");

        remove_test_directory(&fixture.metadata);
    }

    #[test]
    fn provenance_rejects_unrelated_and_failed_reports_without_replacing_metadata() {
        let fixture = ProvenanceFixture::new("provenance-report");
        fixture
            .finalize(&fixture.metadata)
            .unwrap_or_else(|error| panic!("initial provenance must publish: {error}"));

        let published_metadata = fs::read(&fixture.metadata)
            .unwrap_or_else(|error| panic!("published metadata must be read: {error}"));
        let mut unrelated = fixture.report_value.clone();
        unrelated["inputs"]["actual_image"] = "wrong".into();
        if let Err(error) = fs::write(&fixture.report, unrelated.to_string()) {
            panic!("unrelated verification report must be written: {error}");
        }
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("inputs do not match")));
        assert_eq!(
            fs::read(&fixture.metadata).ok().as_deref(),
            Some(published_metadata.as_slice())
        );

        let mut failed = fixture.report_value.clone();
        failed["passed"] = false.into();
        if let Err(error) = fs::write(&fixture.report, failed.to_string()) {
            panic!("failed verification report must be written: {error}");
        }
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("did not pass")));
        assert_eq!(
            fs::read(&fixture.metadata).ok().as_deref(),
            Some(published_metadata.as_slice())
        );

        remove_test_directory(&fixture.metadata);
    }

    #[test]
    fn provenance_rejects_metadata_actual_alias_without_replacing_actual() {
        let fixture = ProvenanceFixture::new("provenance-alias");
        let actual_bytes = fs::read(&fixture.actual)
            .unwrap_or_else(|error| panic!("actual fixture must be read: {error}"));
        let error = fixture.finalize(&fixture.actual).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and ACTUAL")));
        assert_eq!(
            fs::read(&fixture.actual).ok().as_deref(),
            Some(actual_bytes.as_slice())
        );

        remove_test_directory(&fixture.metadata);
    }

    #[cfg(unix)]
    #[test]
    fn provenance_rejects_indirect_manifest_artifact_aliases() {
        use std::os::unix::fs::symlink;

        let fixture = ProvenanceFixture::new("provenance-indirect-alias");
        let directory = fixture
            .metadata
            .parent()
            .unwrap_or_else(|| panic!("indirect alias fixture must have a parent"));
        let reference_geometry = directory.join("reference.geometry.json");
        let actual_geometry = directory.join("actual.geometry.json");
        let reference = directory.join("reference.png");

        let reference_geometry_bytes = fs::read(&reference_geometry)
            .unwrap_or_else(|error| panic!("reference geometry must be read: {error}"));
        let error = fixture.finalize(&reference_geometry).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and REFERENCE_GEOMETRY")));
        assert_eq!(
            fs::read(&reference_geometry).ok().as_deref(),
            Some(reference_geometry_bytes.as_slice())
        );

        if let Err(error) = fs::hard_link(&actual_geometry, &fixture.metadata) {
            panic!("indirect metadata hardlink must be created: {error}");
        }
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and ACTUAL_GEOMETRY")));
        if let Err(error) = fs::remove_file(&fixture.metadata) {
            panic!("indirect metadata hardlink must be removed: {error}");
        }

        if let Err(error) = symlink(&reference, &fixture.metadata) {
            panic!("indirect metadata symlink must be created: {error}");
        }
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and REFERENCE_IMAGE")));

        remove_test_directory(&fixture.metadata);
    }

    #[test]
    fn provenance_rejects_manifest_paths_outside_the_locked_directory() {
        let fixture = ProvenanceFixture::new("provenance-path-escape");
        if let Err(error) = fs::write(&fixture.metadata, b"published") {
            panic!("previous metadata must be written: {error}");
        }
        let published = fs::read(&fixture.metadata)
            .unwrap_or_else(|error| panic!("previous metadata must be read: {error}"));

        for escaped in ["../reference.geometry.json", "/tmp/reference.geometry.json"] {
            let mut manifest: serde_json::Value = serde_json::from_slice(
                &fs::read(&fixture.manifest)
                    .unwrap_or_else(|error| panic!("manifest fixture must be read: {error}")),
            )
            .unwrap_or_else(|error| panic!("manifest fixture must parse: {error}"));
            manifest["reference_geometry"] = escaped.into();
            if let Err(error) = fs::write(&fixture.manifest, manifest.to_string()) {
                panic!("escaped manifest fixture must be written: {error}");
            }

            let error = fixture.finalize(&fixture.metadata).err();
            assert!(error.is_some_and(|error| error.contains("same-directory file name")));
            assert_eq!(
                fs::read(&fixture.metadata).ok().as_deref(),
                Some(published.as_slice())
            );
        }

        remove_test_directory(&fixture.metadata);
    }

    #[cfg(unix)]
    #[test]
    fn provenance_rejects_symlinked_manifest_artifacts() {
        use std::os::unix::fs::symlink;

        for (field, file_name) in [
            ("reference_geometry", "reference.geometry.json"),
            ("actual_geometry", "actual.geometry.json"),
            ("reference_image", "reference.png"),
            ("actual_image", "actual.png"),
        ] {
            let fixture = ProvenanceFixture::new(&format!("provenance-symlink-{field}"));
            fixture
                .finalize(&fixture.metadata)
                .unwrap_or_else(|error| panic!("initial provenance must publish: {error}"));
            let published = fs::read(&fixture.metadata)
                .unwrap_or_else(|error| panic!("published metadata must be read: {error}"));
            let artifact = fixture
                .metadata
                .parent()
                .unwrap_or_else(|| panic!("symlink fixture must have a parent"))
                .join(file_name);
            if let Err(error) = fs::remove_file(&artifact) {
                panic!("manifest artifact must be removed: {error}");
            }
            let outside = test_output(&format!("provenance-symlink-target-{field}"));
            if let Err(error) = fs::write(&outside, b"outside the locked directory") {
                panic!("outside artifact must be written: {error}");
            }
            if let Err(error) = symlink(&outside, &artifact) {
                panic!("manifest artifact symlink must be created: {error}");
            }

            let error = fixture.finalize(&fixture.metadata).err();
            assert!(
                error.is_some_and(|error| {
                    error.contains(field) && error.contains("regular file")
                })
            );
            assert_eq!(
                fs::read(&fixture.metadata).ok().as_deref(),
                Some(published.as_slice())
            );

            remove_test_directory(&fixture.metadata);
            remove_test_directory(&outside);
        }
    }

    #[test]
    fn provenance_cleanup_failure_preserves_previous_metadata() {
        let fixture = ProvenanceFixture::new("provenance-cleanup-failure");
        fixture
            .finalize(&fixture.metadata)
            .unwrap_or_else(|error| panic!("initial provenance must publish: {error}"));
        let published = fs::read(&fixture.metadata)
            .unwrap_or_else(|error| panic!("published metadata must be read: {error}"));
        let blocked_temporary = stale_temporary_path(&fixture.metadata, "blocked-directory");
        if let Err(error) = fs::create_dir(&blocked_temporary) {
            panic!("blocked temporary directory must be created: {error}");
        }

        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("not a regular file or symlink")));
        assert_eq!(
            fs::read(&fixture.metadata).ok().as_deref(),
            Some(published.as_slice())
        );

        remove_test_directory(&fixture.metadata);
    }

    #[test]
    fn provenance_rejects_stale_report_and_concurrent_writer() {
        let fixture = ProvenanceFixture::new("provenance-concurrent");
        if let Err(error) = fs::write(&fixture.metadata, b"published") {
            panic!("previous metadata must be written: {error}");
        }
        let published = fs::read(&fixture.metadata)
            .unwrap_or_else(|error| panic!("previous metadata must be read: {error}"));
        let report = fs::OpenOptions::new()
            .write(true)
            .open(&fixture.report)
            .unwrap_or_else(|error| panic!("report fixture must open: {error}"));
        let times = fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH);
        if let Err(error) = report.set_times(times) {
            panic!("report fixture timestamp must be set: {error}");
        }
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some_and(|error| error.contains("older than")));
        assert_eq!(
            fs::read(&fixture.metadata).ok().as_deref(),
            Some(published.as_slice())
        );

        let lock = super::CaptureLock::acquire(&fixture.metadata)
            .unwrap_or_else(|error| panic!("concurrent writer lock must be acquired: {error}"));
        let error = fixture.finalize(&fixture.metadata).err();
        assert!(error.is_some());
        assert_eq!(
            fs::read(&fixture.metadata).ok().as_deref(),
            Some(published.as_slice())
        );
        drop(lock);

        remove_test_directory(&fixture.metadata);
    }

    #[test]
    fn window_capture_exit_requires_published_readiness_or_image() {
        assert_eq!(
            window_capture_exit_code(true),
            std::process::ExitCode::SUCCESS
        );
        assert_ne!(
            window_capture_exit_code(false),
            std::process::ExitCode::SUCCESS
        );
    }

    #[cfg(unix)]
    #[test]
    fn metadata_rejects_hardlink_and_symlink_artifact_aliases() {
        use std::os::unix::fs::symlink;

        let metadata = test_output("metadata-link-alias").with_file_name("metadata.json");
        let directory = metadata
            .parent()
            .unwrap_or_else(|| panic!("metadata alias fixture must have a parent"));
        let black = directory.join("black.png");
        let white = directory.join("white.png");
        let actual = directory.join("actual.png");
        let manifest = directory.join("manifest.json");
        let report = directory.join("report.json");
        for path in [&black, &white, &actual, &manifest, &report] {
            if let Err(error) = fs::write(path, path.as_os_str().as_encoded_bytes()) {
                panic!("metadata alias fixture must be written: {error}");
            }
        }

        if let Err(error) = fs::hard_link(&black, &metadata) {
            panic!("metadata hardlink fixture must be created: {error}");
        }
        let error =
            validate_metadata_path(&metadata, &black, &white, &actual, &manifest, &report).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and BLACK")));
        if let Err(error) = fs::remove_file(&metadata) {
            panic!("metadata hardlink fixture must be removed: {error}");
        }

        if let Err(error) = symlink(&report, &metadata) {
            panic!("metadata symlink fixture must be created: {error}");
        }
        let error =
            validate_metadata_path(&metadata, &black, &white, &actual, &manifest, &report).err();
        assert!(error.is_some_and(|error| error.contains("METADATA and REPORT")));

        remove_test_directory(&metadata);
    }
}
