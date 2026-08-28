use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::{ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use thiserror::Error;

const PIXEL_CHANGE_THRESHOLD: f64 = 0.02;
const MAX_PIXEL_ATTRIBUTION_WORK: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse verification JSON from {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to decode image {path}: {message}")]
    Image { path: String, message: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeometryNode {
    pub parent: Option<String>,
    pub sibling_index: u32,
    pub bounds: Bounds,
    #[serde(default)]
    pub clipped: bool,
    pub text_bounds: Option<Bounds>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeometrySnapshot {
    pub viewport: [f32; 2],
    pub nodes: BTreeMap<String, GeometryNode>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Thresholds {
    pub geometry_absolute_px: f32,
    pub geometry_relative: f32,
    pub mean_absolute_pixel_error: f64,
    pub changed_pixel_ratio: f64,
    pub edge_error: f64,
    pub minimum_ssim: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            geometry_absolute_px: 0.75,
            geometry_relative: 0.005,
            mean_absolute_pixel_error: 0.02,
            changed_pixel_ratio: 0.04,
            edge_error: 0.03,
            minimum_ssim: 0.98,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerificationManifest {
    pub reference_geometry: String,
    pub actual_geometry: String,
    pub reference_image: Option<String>,
    pub actual_image: Option<String>,
    #[serde(default)]
    pub thresholds: Thresholds,
}

#[derive(Clone, Debug, Serialize)]
pub struct GeometryDifference {
    pub node_id: String,
    pub property: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DerivedGeometryDifference {
    pub node_id: String,
    pub related_node_id: String,
    pub property: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PixelBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct NodePixelDifference {
    pub node_id: String,
    pub changed_pixel_count: u64,
    pub changed_pixel_ratio: f64,
    pub mean_absolute_error: f64,
    pub maximum_pixel_error: f64,
    pub changed_bounds: PixelBounds,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnalysisDiagnostic {
    pub code: String,
    pub node_id: String,
    pub property: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ImageMetrics {
    pub mean_absolute_error: f64,
    pub changed_pixel_ratio: f64,
    pub edge_error: f64,
    pub ssim: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerificationReport {
    pub passed: bool,
    pub inputs: VerificationInputs,
    pub geometry_differences: Vec<GeometryDifference>,
    pub derived_geometry_differences: Vec<DerivedGeometryDifference>,
    pub image_metrics: Option<ImageMetrics>,
    pub node_pixel_differences: Vec<NodePixelDifference>,
    pub analysis_diagnostics: Vec<AnalysisDiagnostic>,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerificationInputs {
    pub manifest: String,
    pub reference_geometry: String,
    pub actual_geometry: String,
    pub reference_image: Option<String>,
    pub actual_image: Option<String>,
}

struct ImageAnalysis {
    metrics: Option<ImageMetrics>,
    node_differences: Vec<NodePixelDifference>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

pub fn verify_manifest(path: &Path) -> Result<VerificationReport, VerifyError> {
    let manifest_bytes = read_bytes(path)?;
    let manifest: VerificationManifest = parse_json(path, &manifest_bytes)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let reference_geometry_path = base.join(&manifest.reference_geometry);
    let actual_geometry_path = base.join(&manifest.actual_geometry);
    let reference_geometry_bytes = read_bytes(&reference_geometry_path)?;
    let actual_geometry_bytes = read_bytes(&actual_geometry_path)?;
    let reference_image = manifest
        .reference_image
        .as_ref()
        .map(|image| {
            let path = base.join(image);
            read_bytes(&path).map(|bytes| (path, bytes))
        })
        .transpose()?;
    let actual_image = manifest
        .actual_image
        .as_ref()
        .map(|image| {
            let path = base.join(image);
            read_bytes(&path).map(|bytes| (path, bytes))
        })
        .transpose()?;
    let inputs = VerificationInputs {
        manifest: sha256_hex(&manifest_bytes),
        reference_geometry: sha256_hex(&reference_geometry_bytes),
        actual_geometry: sha256_hex(&actual_geometry_bytes),
        reference_image: reference_image.as_ref().map(|(_, bytes)| sha256_hex(bytes)),
        actual_image: actual_image.as_ref().map(|(_, bytes)| sha256_hex(bytes)),
    };
    let reference_geometry: GeometrySnapshot =
        parse_json(&reference_geometry_path, &reference_geometry_bytes)?;
    let actual_geometry: GeometrySnapshot =
        parse_json(&actual_geometry_path, &actual_geometry_bytes)?;

    let geometry_differences =
        compare_geometry(&reference_geometry, &actual_geometry, manifest.thresholds);
    let (derived_geometry_differences, mut analysis_diagnostics) =
        compare_derived_geometry(&reference_geometry, &actual_geometry, manifest.thresholds);
    let mut failures = Vec::new();
    if !geometry_differences.is_empty() {
        failures.push(format!(
            "{} geometry or hierarchy differences exceeded tolerance",
            geometry_differences.len()
        ));
    }
    if !derived_geometry_differences.is_empty() {
        failures.push(format!(
            "{} derived spacing or alignment differences exceeded tolerance",
            derived_geometry_differences.len()
        ));
    }

    let image_analysis = compare_manifest_images(
        reference_image.as_ref(),
        actual_image.as_ref(),
        &reference_geometry,
        &actual_geometry,
        manifest.thresholds,
        &mut failures,
    )?;
    analysis_diagnostics.extend(image_analysis.diagnostics);

    Ok(VerificationReport {
        passed: failures.is_empty(),
        inputs,
        geometry_differences,
        derived_geometry_differences,
        image_metrics: image_analysis.metrics,
        node_pixel_differences: image_analysis.node_differences,
        analysis_diagnostics,
        failures,
    })
}

fn compare_manifest_images(
    reference_input: Option<&(PathBuf, Vec<u8>)>,
    actual_input: Option<&(PathBuf, Vec<u8>)>,
    reference_geometry: &GeometrySnapshot,
    actual_geometry: &GeometrySnapshot,
    thresholds: Thresholds,
    failures: &mut Vec<String>,
) -> Result<ImageAnalysis, VerifyError> {
    let (Some((reference_path, reference_bytes)), Some((actual_path, actual_bytes))) =
        (reference_input, actual_input)
    else {
        if reference_input.is_some() || actual_input.is_some() {
            failures.push("both reference_image and actual_image are required together".to_owned());
        }
        return Ok(ImageAnalysis {
            metrics: None,
            node_differences: Vec::new(),
            diagnostics: Vec::new(),
        });
    };
    let reference = read_image(reference_path, reference_bytes)?;
    let actual = read_image(actual_path, actual_bytes)?;
    let metrics = compare_images(&reference, &actual);
    if reference.dimensions() != actual.dimensions() {
        failures.push(format!(
            "image dimensions differ: reference {:?}, actual {:?}",
            reference.dimensions(),
            actual.dimensions()
        ));
        return Ok(ImageAnalysis {
            metrics: Some(metrics),
            node_differences: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    push_image_metric_failures(metrics, thresholds, failures);
    let (node_differences, diagnostics) =
        attribute_pixel_differences(&reference, &actual, reference_geometry, actual_geometry);
    Ok(ImageAnalysis {
        metrics: Some(metrics),
        node_differences,
        diagnostics,
    })
}

fn push_image_metric_failures(
    metrics: ImageMetrics,
    thresholds: Thresholds,
    failures: &mut Vec<String>,
) {
    if metrics.mean_absolute_error > thresholds.mean_absolute_pixel_error {
        failures.push(format!(
            "mean pixel error {:.5} exceeds {:.5}",
            metrics.mean_absolute_error, thresholds.mean_absolute_pixel_error
        ));
    }
    if metrics.changed_pixel_ratio > thresholds.changed_pixel_ratio {
        failures.push(format!(
            "changed pixel ratio {:.5} exceeds {:.5}",
            metrics.changed_pixel_ratio, thresholds.changed_pixel_ratio
        ));
    }
    if metrics.edge_error > thresholds.edge_error {
        failures.push(format!(
            "edge error {:.5} exceeds {:.5}",
            metrics.edge_error, thresholds.edge_error
        ));
    }
    if metrics.ssim < thresholds.minimum_ssim {
        failures.push(format!(
            "global SSIM {:.5} is below {:.5}",
            metrics.ssim, thresholds.minimum_ssim
        ));
    }
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

fn read_bytes(path: &Path) -> Result<Vec<u8>, VerifyError> {
    let path_text = path.display().to_string();
    fs::read(path).map_err(|source| VerifyError::Read {
        path: path_text,
        source,
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(path: &Path, bytes: &[u8]) -> Result<T, VerifyError> {
    serde_json::from_slice(bytes).map_err(|source| VerifyError::Json {
        path: path.display().to_string(),
        source,
    })
}

fn read_image(path: &Path, bytes: &[u8]) -> Result<RgbaImage, VerifyError> {
    let decoded;
    let image_bytes = if path
        .extension()
        .is_some_and(|extension| extension == "base64")
    {
        let encoded = std::str::from_utf8(bytes)
            .map_err(|error| VerifyError::Image {
                path: path.display().to_string(),
                message: format!("base64 image wrapper is not UTF-8: {error}"),
            })?
            .split_whitespace()
            .collect::<String>();
        decoded = BASE64.decode(encoded).map_err(|error| VerifyError::Image {
            path: path.display().to_string(),
            message: format!("failed to decode base64 image wrapper: {error}"),
        })?;
        decoded.as_slice()
    } else {
        bytes
    };
    ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|error| VerifyError::Image {
            path: path.display().to_string(),
            message: error.to_string(),
        })?
        .decode()
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| VerifyError::Image {
            path: path.display().to_string(),
            message: error.to_string(),
        })
}

fn compare_geometry(
    reference: &GeometrySnapshot,
    actual: &GeometrySnapshot,
    thresholds: Thresholds,
) -> Vec<GeometryDifference> {
    let mut differences = Vec::new();
    compare_scalar(
        "<viewport>",
        "width",
        reference.viewport[0],
        actual.viewport[0],
        thresholds,
        &mut differences,
    );
    compare_scalar(
        "<viewport>",
        "height",
        reference.viewport[1],
        actual.viewport[1],
        thresholds,
        &mut differences,
    );

    for (node_id, expected) in &reference.nodes {
        let Some(actual_node) = actual.nodes.get(node_id) else {
            differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "presence".to_owned(),
                expected: "present".to_owned(),
                actual: "missing".to_owned(),
            });
            continue;
        };

        if expected.parent != actual_node.parent {
            differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "parent".to_owned(),
                expected: format!("{:?}", expected.parent),
                actual: format!("{:?}", actual_node.parent),
            });
        }
        if expected.sibling_index != actual_node.sibling_index {
            differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "sibling_index".to_owned(),
                expected: expected.sibling_index.to_string(),
                actual: actual_node.sibling_index.to_string(),
            });
        }
        compare_bounds(
            node_id,
            "bounds",
            expected.bounds,
            actual_node.bounds,
            thresholds,
            &mut differences,
        );
        if expected.clipped != actual_node.clipped {
            differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "clipped".to_owned(),
                expected: expected.clipped.to_string(),
                actual: actual_node.clipped.to_string(),
            });
        }
        match (expected.text_bounds, actual_node.text_bounds) {
            (Some(expected), Some(actual)) => compare_bounds(
                node_id,
                "text_bounds",
                expected,
                actual,
                thresholds,
                &mut differences,
            ),
            (None, None) => {}
            (expected, actual) => differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "text_bounds".to_owned(),
                expected: format!("{expected:?}"),
                actual: format!("{actual:?}"),
            }),
        }
    }

    for node_id in actual.nodes.keys() {
        if !reference.nodes.contains_key(node_id) {
            differences.push(GeometryDifference {
                node_id: node_id.clone(),
                property: "presence".to_owned(),
                expected: "missing".to_owned(),
                actual: "present".to_owned(),
            });
        }
    }

    differences
}

#[derive(Clone, Copy)]
enum LayoutAxis {
    Horizontal,
    Vertical,
}

fn compare_derived_geometry(
    reference: &GeometrySnapshot,
    actual: &GeometrySnapshot,
    thresholds: Thresholds,
) -> (Vec<DerivedGeometryDifference>, Vec<AnalysisDiagnostic>) {
    let mut differences = Vec::new();
    let mut diagnostics = Vec::new();
    let mut children_by_parent = BTreeMap::<String, Vec<&str>>::new();

    for (node_id, expected) in &reference.nodes {
        let Some(actual_node) = actual.nodes.get(node_id) else {
            continue;
        };
        let Some(parent_id) = expected.parent.as_deref() else {
            continue;
        };
        if actual_node.parent.as_deref() == Some(parent_id)
            && reference.nodes.contains_key(parent_id)
            && actual.nodes.contains_key(parent_id)
        {
            children_by_parent
                .entry(parent_id.to_owned())
                .or_default()
                .push(node_id);
        }
    }

    for (parent_id, mut child_ids) in children_by_parent {
        if child_ids.len() < 2 {
            continue;
        }
        child_ids.sort_by_key(|node_id| {
            let node = &reference.nodes[*node_id];
            (node.sibling_index, *node_id)
        });

        let Some(axis) = infer_layout_axis(&child_ids, reference, thresholds.geometry_absolute_px)
        else {
            diagnostics.push(AnalysisDiagnostic {
                code: "FR-VERIFY-DERIVED-001".to_owned(),
                node_id: parent_id,
                property: "children.layout_axis".to_owned(),
                message: "spacing and cross-axis alignment were not derived because sibling geometry has no unambiguous primary axis".to_owned(),
            });
            continue;
        };

        for pair in child_ids.windows(2) {
            let previous_id = pair[0];
            let node_id = pair[1];
            let expected = axis_gap(
                axis,
                reference.nodes[previous_id].bounds,
                reference.nodes[node_id].bounds,
            );
            let measured = axis_gap(
                axis,
                actual.nodes[previous_id].bounds,
                actual.nodes[node_id].bounds,
            );
            push_derived_difference(
                node_id,
                previous_id,
                match axis {
                    LayoutAxis::Horizontal => "spacing.horizontal",
                    LayoutAxis::Vertical => "spacing.vertical",
                },
                expected,
                measured,
                thresholds.geometry_absolute_px,
                &mut differences,
            );
        }

        let expected_parent = reference.nodes[&parent_id].bounds;
        let actual_parent = actual.nodes[&parent_id].bounds;
        for node_id in child_ids {
            let expected =
                cross_axis_center_offset(axis, expected_parent, reference.nodes[node_id].bounds);
            let measured =
                cross_axis_center_offset(axis, actual_parent, actual.nodes[node_id].bounds);
            push_derived_difference(
                node_id,
                &parent_id,
                match axis {
                    LayoutAxis::Horizontal => "alignment.cross_axis_center_y",
                    LayoutAxis::Vertical => "alignment.cross_axis_center_x",
                },
                expected,
                measured,
                thresholds.geometry_absolute_px,
                &mut differences,
            );
        }
    }

    (differences, diagnostics)
}

fn infer_layout_axis(
    child_ids: &[&str],
    geometry: &GeometrySnapshot,
    tolerance: f32,
) -> Option<LayoutAxis> {
    let mut minimum_x = f32::INFINITY;
    let mut maximum_x = f32::NEG_INFINITY;
    let mut minimum_y = f32::INFINITY;
    let mut maximum_y = f32::NEG_INFINITY;
    for node_id in child_ids {
        let bounds = geometry.nodes[*node_id].bounds;
        let center_x = bounds.x + bounds.width / 2.0;
        let center_y = bounds.y + bounds.height / 2.0;
        minimum_x = minimum_x.min(center_x);
        maximum_x = maximum_x.max(center_x);
        minimum_y = minimum_y.min(center_y);
        maximum_y = maximum_y.max(center_y);
    }
    let horizontal_span = maximum_x - minimum_x;
    let vertical_span = maximum_y - minimum_y;
    if horizontal_span > vertical_span + tolerance {
        Some(LayoutAxis::Horizontal)
    } else if vertical_span > horizontal_span + tolerance {
        Some(LayoutAxis::Vertical)
    } else {
        None
    }
}

fn axis_gap(axis: LayoutAxis, previous: Bounds, current: Bounds) -> f32 {
    match axis {
        LayoutAxis::Horizontal => current.x - (previous.x + previous.width),
        LayoutAxis::Vertical => current.y - (previous.y + previous.height),
    }
}

fn cross_axis_center_offset(axis: LayoutAxis, parent: Bounds, child: Bounds) -> f32 {
    match axis {
        LayoutAxis::Horizontal => child.y + child.height / 2.0 - parent.y,
        LayoutAxis::Vertical => child.x + child.width / 2.0 - parent.x,
    }
}

fn push_derived_difference(
    node_id: &str,
    related_node_id: &str,
    property: &str,
    expected: f32,
    actual: f32,
    tolerance: f32,
    differences: &mut Vec<DerivedGeometryDifference>,
) {
    if (expected - actual).abs() > tolerance {
        differences.push(DerivedGeometryDifference {
            node_id: node_id.to_owned(),
            related_node_id: related_node_id.to_owned(),
            property: property.to_owned(),
            expected: format!("{expected:.4}"),
            actual: format!("{actual:.4}"),
        });
    }
}

fn compare_bounds(
    node_id: &str,
    prefix: &str,
    expected: Bounds,
    actual: Bounds,
    thresholds: Thresholds,
    differences: &mut Vec<GeometryDifference>,
) {
    compare_absolute_scalar(
        node_id,
        &format!("{prefix}.x"),
        expected.x,
        actual.x,
        thresholds.geometry_absolute_px,
        differences,
    );
    compare_absolute_scalar(
        node_id,
        &format!("{prefix}.y"),
        expected.y,
        actual.y,
        thresholds.geometry_absolute_px,
        differences,
    );
    compare_scalar(
        node_id,
        &format!("{prefix}.width"),
        expected.width,
        actual.width,
        thresholds,
        differences,
    );
    compare_scalar(
        node_id,
        &format!("{prefix}.height"),
        expected.height,
        actual.height,
        thresholds,
        differences,
    );
}

fn compare_absolute_scalar(
    node_id: &str,
    property: &str,
    expected: f32,
    actual: f32,
    tolerance: f32,
    differences: &mut Vec<GeometryDifference>,
) {
    if (expected - actual).abs() > tolerance {
        differences.push(GeometryDifference {
            node_id: node_id.to_owned(),
            property: property.to_owned(),
            expected: format!("{expected:.4}"),
            actual: format!("{actual:.4}"),
        });
    }
}

fn compare_scalar(
    node_id: &str,
    property: &str,
    expected: f32,
    actual: f32,
    thresholds: Thresholds,
    differences: &mut Vec<GeometryDifference>,
) {
    let tolerance = thresholds
        .geometry_absolute_px
        .max(expected.abs() * thresholds.geometry_relative);
    if (expected - actual).abs() > tolerance {
        differences.push(GeometryDifference {
            node_id: node_id.to_owned(),
            property: property.to_owned(),
            expected: format!("{expected:.4}"),
            actual: format!("{actual:.4}"),
        });
    }
}

#[derive(Clone, Copy)]
struct PixelCandidate<'a> {
    node_id: &'a str,
    reference: Bounds,
    actual: Bounds,
    depth: usize,
    area: f64,
}

#[derive(Default)]
struct PixelAccumulator {
    count: u32,
    error_sum: f64,
    maximum_error: f64,
    minimum_x: u32,
    minimum_y: u32,
    maximum_x: u32,
    maximum_y: u32,
}

fn attribute_pixel_differences(
    reference_image: &RgbaImage,
    actual_image: &RgbaImage,
    reference_geometry: &GeometrySnapshot,
    actual_geometry: &GeometrySnapshot,
) -> (Vec<NodePixelDifference>, Vec<AnalysisDiagnostic>) {
    let mut diagnostics = Vec::new();
    if !viewport_matches_image(reference_geometry.viewport, reference_image)
        || !viewport_matches_image(actual_geometry.viewport, actual_image)
    {
        diagnostics.push(AnalysisDiagnostic {
            code: "FR-VERIFY-PIXEL-001".to_owned(),
            node_id: "<viewport>".to_owned(),
            property: "pixel_attribution.viewport_scale".to_owned(),
            message: format!(
                "node-scoped pixel attribution requires 1:1 geometry/image dimensions; reference viewport {:?}, actual viewport {:?}, image {:?}",
                reference_geometry.viewport,
                actual_geometry.viewport,
                reference_image.dimensions()
            ),
        });
        return (Vec::new(), diagnostics);
    }

    let common_nodes = common_geometry_nodes(reference_geometry, actual_geometry);
    let candidates = pixel_candidates(reference_geometry, actual_geometry, &common_nodes);
    let pixel_count = u64::from(reference_image.width()) * u64::from(reference_image.height());
    let candidate_count = u64::try_from(candidates.len()).unwrap_or(u64::MAX);
    let work = pixel_count.saturating_mul(candidate_count);
    if work > MAX_PIXEL_ATTRIBUTION_WORK {
        diagnostics.push(AnalysisDiagnostic {
            code: "FR-VERIFY-PIXEL-002".to_owned(),
            node_id: "<viewport>".to_owned(),
            property: "pixel_attribution.work_limit".to_owned(),
            message: format!(
                "node-scoped pixel attribution requires {work} pixel-node checks, exceeding the bounded limit {MAX_PIXEL_ATTRIBUTION_WORK}"
            ),
        });
        return (Vec::new(), diagnostics);
    }

    let (accumulators, owned_counts, unmapped) =
        collect_pixel_attribution(reference_image, actual_image, &candidates);

    if unmapped > 0 {
        diagnostics.push(AnalysisDiagnostic {
            code: "FR-VERIFY-PIXEL-003".to_owned(),
            node_id: "<viewport>".to_owned(),
            property: "pixel_attribution.unmapped".to_owned(),
            message: format!(
                "{unmapped} changed pixels were outside every common source-node bound"
            ),
        });
    }

    let differences = finish_pixel_attribution(accumulators, &owned_counts);
    (differences, diagnostics)
}

fn common_geometry_nodes(
    reference: &GeometrySnapshot,
    actual: &GeometrySnapshot,
) -> BTreeSet<String> {
    reference
        .nodes
        .keys()
        .filter(|node_id| actual.nodes.contains_key(*node_id))
        .cloned()
        .collect()
}

fn pixel_candidates<'a>(
    reference_geometry: &'a GeometrySnapshot,
    actual_geometry: &'a GeometrySnapshot,
    common_nodes: &BTreeSet<String>,
) -> Vec<PixelCandidate<'a>> {
    common_nodes
        .iter()
        .filter_map(|node_id| {
            let (reference_id, reference) = reference_geometry.nodes.get_key_value(node_id)?;
            let actual = &actual_geometry.nodes[node_id];
            if reference.parent != actual.parent
                || !valid_attribution_bounds(reference.bounds)
                || !valid_attribution_bounds(actual.bounds)
            {
                return None;
            }
            let reference_area =
                f64::from(reference.bounds.width) * f64::from(reference.bounds.height);
            let actual_area = f64::from(actual.bounds.width) * f64::from(actual.bounds.height);
            Some(PixelCandidate {
                node_id: reference_id,
                reference: reference.bounds,
                actual: actual.bounds,
                depth: node_depth(node_id, reference_geometry, common_nodes),
                area: reference_area.min(actual_area),
            })
        })
        .collect()
}

fn collect_pixel_attribution<'a>(
    reference: &RgbaImage,
    actual: &RgbaImage,
    candidates: &'a [PixelCandidate<'a>],
) -> (
    BTreeMap<&'a str, PixelAccumulator>,
    BTreeMap<&'a str, u32>,
    u64,
) {
    let mut accumulators = BTreeMap::<&str, PixelAccumulator>::new();
    let mut owned_counts = BTreeMap::<&str, u32>::new();
    let mut unmapped = 0_u64;
    for (x, y, expected) in reference.enumerate_pixels() {
        let center_x = f64::from(x) + 0.5;
        let center_y = f64::from(y) + 0.5;
        let owner = pixel_owner(candidates, center_x, center_y);
        if let Some(owner) = owner {
            *owned_counts.entry(owner.node_id).or_default() += 1;
        }
        let error = normalized_pixel_error(expected.0, actual.get_pixel(x, y).0);
        if error <= PIXEL_CHANGE_THRESHOLD {
            continue;
        }
        let Some(owner) = owner else {
            unmapped += 1;
            continue;
        };
        update_pixel_accumulator(&mut accumulators, owner.node_id, x, y, error);
    }
    (accumulators, owned_counts, unmapped)
}

fn pixel_owner<'a>(
    candidates: &'a [PixelCandidate<'a>],
    x: f64,
    y: f64,
) -> Option<&'a PixelCandidate<'a>> {
    candidates
        .iter()
        .filter(|candidate| {
            bounds_contains(candidate.reference, x, y) || bounds_contains(candidate.actual, x, y)
        })
        .max_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| right.area.total_cmp(&left.area))
                .then_with(|| right.node_id.cmp(left.node_id))
        })
}

fn update_pixel_accumulator<'a>(
    accumulators: &mut BTreeMap<&'a str, PixelAccumulator>,
    node_id: &'a str,
    x: u32,
    y: u32,
    error: f64,
) {
    let accumulator = accumulators
        .entry(node_id)
        .or_insert_with(|| PixelAccumulator {
            minimum_x: x,
            minimum_y: y,
            maximum_x: x,
            maximum_y: y,
            ..PixelAccumulator::default()
        });
    accumulator.count += 1;
    accumulator.error_sum += error;
    accumulator.maximum_error = accumulator.maximum_error.max(error);
    accumulator.minimum_x = accumulator.minimum_x.min(x);
    accumulator.minimum_y = accumulator.minimum_y.min(y);
    accumulator.maximum_x = accumulator.maximum_x.max(x);
    accumulator.maximum_y = accumulator.maximum_y.max(y);
}

fn finish_pixel_attribution(
    accumulators: BTreeMap<&str, PixelAccumulator>,
    owned_counts: &BTreeMap<&str, u32>,
) -> Vec<NodePixelDifference> {
    accumulators
        .into_iter()
        .map(|(node_id, accumulator)| {
            let owned_count = owned_counts.get(node_id).copied().unwrap_or(1);
            NodePixelDifference {
                node_id: node_id.to_owned(),
                changed_pixel_count: u64::from(accumulator.count),
                changed_pixel_ratio: f64::from(accumulator.count) / f64::from(owned_count),
                mean_absolute_error: accumulator.error_sum / f64::from(accumulator.count),
                maximum_pixel_error: accumulator.maximum_error,
                changed_bounds: PixelBounds {
                    x: accumulator.minimum_x,
                    y: accumulator.minimum_y,
                    width: accumulator.maximum_x - accumulator.minimum_x + 1,
                    height: accumulator.maximum_y - accumulator.minimum_y + 1,
                },
            }
        })
        .collect()
}

fn viewport_matches_image(viewport: [f32; 2], image: &RgbaImage) -> bool {
    viewport[0].is_finite()
        && viewport[1].is_finite()
        && (f64::from(viewport[0]) - f64::from(image.width())).abs() < f64::EPSILON
        && (f64::from(viewport[1]) - f64::from(image.height())).abs() < f64::EPSILON
}

fn valid_attribution_bounds(bounds: Bounds) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.width > 0.0
        && bounds.height > 0.0
}

fn node_depth(
    node_id: &str,
    geometry: &GeometrySnapshot,
    common_nodes: &BTreeSet<String>,
) -> usize {
    let mut depth = 0;
    let mut current = geometry.nodes[node_id].parent.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(parent_id) = current {
        if !common_nodes.contains(parent_id) || !visited.insert(parent_id) {
            break;
        }
        depth += 1;
        current = geometry
            .nodes
            .get(parent_id)
            .and_then(|node| node.parent.as_deref());
    }
    depth
}

fn bounds_contains(bounds: Bounds, x: f64, y: f64) -> bool {
    x >= f64::from(bounds.x)
        && y >= f64::from(bounds.y)
        && x < f64::from(bounds.x + bounds.width)
        && y < f64::from(bounds.y + bounds.height)
}

fn normalized_pixel_error(expected: [u8; 4], actual: [u8; 4]) -> f64 {
    let expected_alpha = f64::from(expected[3]) / 255.0;
    let actual_alpha = f64::from(actual[3]) / 255.0;
    let rgb_error = (0..3)
        .map(|channel| {
            let expected = f64::from(expected[channel]) / 255.0 * expected_alpha;
            let actual = f64::from(actual[channel]) / 255.0 * actual_alpha;
            (expected - actual).abs()
        })
        .sum::<f64>();
    (rgb_error + (expected_alpha - actual_alpha).abs()) / 4.0
}

fn compare_images(reference: &RgbaImage, actual: &RgbaImage) -> ImageMetrics {
    if reference.dimensions() != actual.dimensions() || reference.is_empty() {
        return ImageMetrics {
            mean_absolute_error: 1.0,
            changed_pixel_ratio: 1.0,
            edge_error: 1.0,
            ssim: 0.0,
        };
    }

    let mut absolute_sum = 0.0_f64;
    let mut changed = 0.0_f64;
    let mut reference_luma = Vec::with_capacity(reference.len() / 4);
    let mut actual_luma = Vec::with_capacity(actual.len() / 4);
    for (expected, actual) in reference.pixels().zip(actual.pixels()) {
        let pixel_error = normalized_pixel_error(expected.0, actual.0);
        absolute_sum += pixel_error;
        if pixel_error > PIXEL_CHANGE_THRESHOLD {
            changed += 1.0;
        }
        reference_luma.push(luminance(expected.0));
        actual_luma.push(luminance(actual.0));
    }

    let pixel_count = f64::from(reference.width()) * f64::from(reference.height());
    ImageMetrics {
        mean_absolute_error: absolute_sum / pixel_count,
        changed_pixel_ratio: changed / pixel_count,
        edge_error: edge_error(
            &reference_luma,
            &actual_luma,
            reference.width() as usize,
            reference.height() as usize,
        ),
        ssim: global_ssim(&reference_luma, &actual_luma),
    }
}

fn luminance(pixel: [u8; 4]) -> f64 {
    (0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2]))
        * (f64::from(pixel[3]) / 255.0)
        / 255.0
}

fn edge_error(reference: &[f64], actual: &[f64], width: usize, height: usize) -> f64 {
    if width < 2 || height < 2 {
        return 0.0;
    }
    let mut difference = 0.0;
    let mut samples = 0.0_f64;
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            let index = y * width + x;
            let reference_edge = (reference[index + 1] - reference[index]).abs()
                + (reference[index + width] - reference[index]).abs();
            let actual_edge = (actual[index + 1] - actual[index]).abs()
                + (actual[index + width] - actual[index]).abs();
            difference += (reference_edge - actual_edge).abs() / 2.0;
            samples += 1.0;
        }
    }
    difference / samples
}

fn global_ssim(reference: &[f64], actual: &[f64]) -> f64 {
    let (reference_sum, count) = reference
        .iter()
        .fold((0.0, 0.0), |(sum, count), value| (sum + value, count + 1.0));
    let reference_mean = reference_sum / count;
    let actual_mean = actual.iter().sum::<f64>() / count;
    let mut reference_variance = 0.0;
    let mut actual_variance = 0.0;
    let mut covariance = 0.0;
    for (&reference, &actual) in reference.iter().zip(actual) {
        let reference_delta = reference - reference_mean;
        let actual_delta = actual - actual_mean;
        reference_variance += reference_delta * reference_delta;
        actual_variance += actual_delta * actual_delta;
        covariance += reference_delta * actual_delta;
    }
    let denominator = (count - 1.0).max(1.0);
    reference_variance /= denominator;
    actual_variance /= denominator;
    covariance /= denominator;

    let c1 = 0.01_f64.powi(2);
    let c2 = 0.03_f64.powi(2);
    ((2.0 * reference_mean * actual_mean + c1) * (2.0 * covariance + c2))
        / ((reference_mean.powi(2) + actual_mean.powi(2) + c1)
            * (reference_variance + actual_variance + c2))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use image::{Rgba, RgbaImage};

    use super::{
        Bounds, GeometryNode, GeometrySnapshot, Thresholds, attribute_pixel_differences,
        compare_derived_geometry, compare_geometry, compare_images, verify_manifest,
    };

    #[test]
    fn identical_geometry_and_pixels_pass() {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "1:2".to_owned(),
            GeometryNode {
                parent: None,
                sibling_index: 0,
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 50.0,
                },
                clipped: false,
                text_bounds: None,
            },
        );
        let geometry = GeometrySnapshot {
            viewport: [100.0, 50.0],
            nodes,
        };
        assert!(compare_geometry(&geometry, &geometry, Thresholds::default()).is_empty());

        let image = RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 255]));
        let metrics = compare_images(&image, &image);
        assert!(metrics.mean_absolute_error.abs() < f64::EPSILON);
        assert!(metrics.changed_pixel_ratio.abs() < f64::EPSILON);
        assert!(metrics.edge_error.abs() < f64::EPSILON);
        assert!((metrics.ssim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn verification_report_binds_every_input_hash() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/real-figma/verify.image.json");
        let report = verify_manifest(&manifest)
            .unwrap_or_else(|error| panic!("real fixture verification must run: {error}"));

        assert!(report.passed);
        assert_eq!(report.inputs.manifest.len(), 64);
        assert_eq!(report.inputs.reference_geometry.len(), 64);
        assert_eq!(report.inputs.actual_geometry.len(), 64);
        assert_eq!(
            report.inputs.actual_image.as_deref(),
            Some("3cdadaa6e0a29756eaf467632e9dab30e771491aa46af89ba2fab054ca99ac83")
        );
        assert_eq!(
            report.inputs.reference_image.as_deref(),
            Some("00b870f8e967362deb405959a68758d31f10615aa949eb44e7cfa6cdad84db89")
        );
    }

    #[test]
    fn attribution_fixture_reports_spacing_alignment_and_color_owner() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/verification-attribution/verify.json");
        let report = verify_manifest(&manifest)
            .unwrap_or_else(|error| panic!("attribution fixture verification must run: {error}"));

        assert!(!report.passed);
        assert!(report.analysis_diagnostics.is_empty());
        assert!(
            report
                .derived_geometry_differences
                .iter()
                .any(|difference| {
                    difference.node_id == "1:3"
                        && difference.related_node_id == "1:2"
                        && difference.property == "spacing.horizontal"
                })
        );
        assert!(
            report
                .derived_geometry_differences
                .iter()
                .any(|difference| {
                    difference.node_id == "1:3"
                        && difference.related_node_id == "1:1"
                        && difference.property == "alignment.cross_axis_center_y"
                })
        );
        assert_eq!(report.node_pixel_differences.len(), 1);
        assert_eq!(report.node_pixel_differences[0].node_id, "1:2");
        assert_eq!(report.node_pixel_differences[0].changed_pixel_count, 1);
    }

    #[test]
    fn hierarchy_and_bounds_fail_with_node_identity() {
        let expected = GeometrySnapshot {
            viewport: [100.0, 50.0],
            nodes: BTreeMap::from([(
                "1:2".to_owned(),
                GeometryNode {
                    parent: Some("1:1".to_owned()),
                    sibling_index: 0,
                    bounds: Bounds {
                        x: 10.0,
                        y: 5.0,
                        width: 20.0,
                        height: 10.0,
                    },
                    clipped: false,
                    text_bounds: None,
                },
            )]),
        };
        let actual = GeometrySnapshot {
            viewport: [100.0, 50.0],
            nodes: BTreeMap::from([(
                "1:2".to_owned(),
                GeometryNode {
                    parent: None,
                    sibling_index: 0,
                    bounds: Bounds {
                        x: 12.0,
                        y: 5.0,
                        width: 20.0,
                        height: 10.0,
                    },
                    clipped: false,
                    text_bounds: None,
                },
            )]),
        };

        let differences = compare_geometry(&expected, &actual, Thresholds::default());
        assert!(
            differences
                .iter()
                .all(|difference| difference.node_id == "1:2")
        );
        assert!(
            differences
                .iter()
                .any(|difference| difference.property == "parent")
        );
        assert!(
            differences
                .iter()
                .any(|difference| difference.property == "bounds.x")
        );
    }

    #[test]
    fn far_canvas_origin_uses_only_absolute_tolerance() {
        let expected = geometry_node(100_000.0, 0);
        let actual = geometry_node(100_004.0, 0);
        let differences = compare_geometry(&expected, &actual, Thresholds::default());
        assert!(
            differences
                .iter()
                .any(|difference| difference.property == "bounds.x")
        );
    }

    #[test]
    fn reversed_siblings_are_reported() {
        let expected = geometry_node(0.0, 0);
        let actual = geometry_node(0.0, 1);
        let differences = compare_geometry(&expected, &actual, Thresholds::default());
        assert!(
            differences
                .iter()
                .any(|difference| difference.property == "sibling_index")
        );
    }

    #[test]
    fn transparent_hidden_rgb_is_ignored() {
        let reference = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 0]));
        let actual = RgbaImage::from_pixel(2, 2, Rgba([0, 255, 255, 0]));
        let metrics = compare_images(&reference, &actual);
        assert!(metrics.mean_absolute_error.abs() < f64::EPSILON);
        assert!(metrics.changed_pixel_ratio.abs() < f64::EPSILON);
    }

    #[test]
    fn spacing_and_alignment_differences_name_the_affected_nodes() {
        let reference = nested_layout_geometry(10.0, 5.0);
        let actual = nested_layout_geometry(12.0, 7.0);

        let (differences, diagnostics) =
            compare_derived_geometry(&reference, &actual, zero_thresholds());

        assert!(diagnostics.is_empty());
        assert!(differences.iter().any(|difference| {
            difference.node_id == "1:3"
                && difference.related_node_id == "1:2"
                && difference.property == "spacing.horizontal"
                && difference.expected == "10.0000"
                && difference.actual == "12.0000"
        }));
        assert!(differences.iter().any(|difference| {
            difference.node_id == "1:3"
                && difference.related_node_id == "1:1"
                && difference.property == "alignment.cross_axis_center_y"
                && difference.expected == "10.0000"
                && difference.actual == "12.0000"
        }));
    }

    #[test]
    fn changed_pixels_are_attributed_to_the_smallest_common_node() {
        let geometry = nested_layout_geometry(10.0, 5.0);
        let reference = RgbaImage::from_pixel(40, 20, Rgba([255, 255, 255, 255]));
        let mut actual = reference.clone();
        actual.put_pixel(2, 6, Rgba([255, 0, 0, 255]));

        let (differences, diagnostics) =
            attribute_pixel_differences(&reference, &actual, &geometry, &geometry);

        assert!(diagnostics.is_empty());
        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].node_id, "1:2");
        assert_eq!(differences[0].changed_pixel_count, 1);
        assert_eq!(differences[0].changed_bounds.x, 2);
        assert_eq!(differences[0].changed_bounds.y, 6);
    }

    #[test]
    fn pixel_attribution_fails_closed_when_viewport_scale_is_unknown() {
        let geometry = nested_layout_geometry(10.0, 5.0);
        let reference = RgbaImage::from_pixel(80, 40, Rgba([255, 255, 255, 255]));
        let mut actual = reference.clone();
        actual.put_pixel(4, 12, Rgba([255, 0, 0, 255]));

        let (differences, diagnostics) =
            attribute_pixel_differences(&reference, &actual, &geometry, &geometry);

        assert!(differences.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "FR-VERIFY-PIXEL-001");
        assert_eq!(diagnostics[0].property, "pixel_attribution.viewport_scale");
    }

    fn geometry_node(x: f32, sibling_index: u32) -> GeometrySnapshot {
        GeometrySnapshot {
            viewport: [100.0, 50.0],
            nodes: BTreeMap::from([(
                "1:2".to_owned(),
                GeometryNode {
                    parent: Some("1:1".to_owned()),
                    sibling_index,
                    bounds: Bounds {
                        x,
                        y: 5.0,
                        width: 20.0,
                        height: 10.0,
                    },
                    clipped: false,
                    text_bounds: None,
                },
            )]),
        }
    }

    fn nested_layout_geometry(second_gap: f32, second_y: f32) -> GeometrySnapshot {
        GeometrySnapshot {
            viewport: [40.0, 20.0],
            nodes: BTreeMap::from([
                (
                    "1:1".to_owned(),
                    GeometryNode {
                        parent: None,
                        sibling_index: 0,
                        bounds: Bounds {
                            x: 0.0,
                            y: 0.0,
                            width: 40.0,
                            height: 20.0,
                        },
                        clipped: false,
                        text_bounds: None,
                    },
                ),
                (
                    "1:2".to_owned(),
                    GeometryNode {
                        parent: Some("1:1".to_owned()),
                        sibling_index: 0,
                        bounds: Bounds {
                            x: 0.0,
                            y: 5.0,
                            width: 10.0,
                            height: 10.0,
                        },
                        clipped: false,
                        text_bounds: None,
                    },
                ),
                (
                    "1:3".to_owned(),
                    GeometryNode {
                        parent: Some("1:1".to_owned()),
                        sibling_index: 1,
                        bounds: Bounds {
                            x: 10.0 + second_gap,
                            y: second_y,
                            width: 10.0,
                            height: 10.0,
                        },
                        clipped: false,
                        text_bounds: None,
                    },
                ),
            ]),
        }
    }

    fn zero_thresholds() -> Thresholds {
        Thresholds {
            geometry_absolute_px: 0.0,
            geometry_relative: 0.0,
            mean_absolute_pixel_error: 0.0,
            changed_pixel_ratio: 0.0,
            edge_error: 0.0,
            minimum_ssim: 1.0,
        }
    }
}
