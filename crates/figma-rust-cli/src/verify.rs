use std::{collections::BTreeMap, fs, io::Cursor, path::Path};

use image::{ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use thiserror::Error;

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
    pub image_metrics: Option<ImageMetrics>,
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
    let mut failures = Vec::new();
    if !geometry_differences.is_empty() {
        failures.push(format!(
            "{} geometry or hierarchy differences exceeded tolerance",
            geometry_differences.len()
        ));
    }

    let image_metrics = match (&reference_image, &actual_image) {
        (Some((reference_path, reference_bytes)), Some((actual_path, actual_bytes))) => {
            let reference = read_image(reference_path, reference_bytes)?;
            let actual = read_image(actual_path, actual_bytes)?;
            let metrics = compare_images(&reference, &actual);
            if reference.dimensions() == actual.dimensions() {
                if metrics.mean_absolute_error > manifest.thresholds.mean_absolute_pixel_error {
                    failures.push(format!(
                        "mean pixel error {:.5} exceeds {:.5}",
                        metrics.mean_absolute_error, manifest.thresholds.mean_absolute_pixel_error
                    ));
                }
                if metrics.changed_pixel_ratio > manifest.thresholds.changed_pixel_ratio {
                    failures.push(format!(
                        "changed pixel ratio {:.5} exceeds {:.5}",
                        metrics.changed_pixel_ratio, manifest.thresholds.changed_pixel_ratio
                    ));
                }
                if metrics.edge_error > manifest.thresholds.edge_error {
                    failures.push(format!(
                        "edge error {:.5} exceeds {:.5}",
                        metrics.edge_error, manifest.thresholds.edge_error
                    ));
                }
                if metrics.ssim < manifest.thresholds.minimum_ssim {
                    failures.push(format!(
                        "global SSIM {:.5} is below {:.5}",
                        metrics.ssim, manifest.thresholds.minimum_ssim
                    ));
                }
            } else {
                failures.push(format!(
                    "image dimensions differ: reference {:?}, actual {:?}",
                    reference.dimensions(),
                    actual.dimensions()
                ));
            }
            Some(metrics)
        }
        (None, None) => None,
        _ => {
            failures.push("both reference_image and actual_image are required together".to_owned());
            None
        }
    };

    Ok(VerificationReport {
        passed: failures.is_empty(),
        inputs,
        geometry_differences,
        image_metrics,
        failures,
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
    ImageReader::new(Cursor::new(bytes))
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
        let expected_alpha = f64::from(expected[3]) / 255.0;
        let actual_alpha = f64::from(actual[3]) / 255.0;
        let rgb_error = (0..3)
            .map(|channel| {
                let expected = f64::from(expected[channel]) / 255.0 * expected_alpha;
                let actual = f64::from(actual[channel]) / 255.0 * actual_alpha;
                (expected - actual).abs()
            })
            .sum::<f64>();
        let pixel_error = rgb_error + (expected_alpha - actual_alpha).abs();
        absolute_sum += pixel_error;
        if pixel_error / 4.0 > 0.02 {
            changed += 1.0;
        }
        reference_luma.push(luminance(expected.0));
        actual_luma.push(luminance(actual.0));
    }

    let pixel_count = f64::from(reference.width()) * f64::from(reference.height());
    ImageMetrics {
        mean_absolute_error: absolute_sum / (pixel_count * 4.0),
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
        Bounds, GeometryNode, GeometrySnapshot, Thresholds, compare_geometry, compare_images,
        verify_manifest,
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
}
