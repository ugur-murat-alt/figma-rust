use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Diagnostic, ExtractionFingerprint, Severity, diagnostic::codes, extraction_fingerprint,
    raw::ExtractionBundle,
};

pub const CAPABILITY_PROFILE_REPORT_VERSION: u32 = 1;
pub const ORBITLINE_MINIMAL_PROFILE_V1: &str = "orbitline-minimal-v1";

const REQUIRED_CAPABILITIES: [&str; 6] = [
    "bound-component-properties",
    "bounded-traversal",
    "child-counter-alignment",
    "modeled-bound-dimensions",
    "schema-v2",
    "typed-text-layout",
];
const OPTIONAL_CAPABILITIES: [&str; 1] = ["asset-payload-export"];
const QUARANTINED_CAPABILITIES: [&str; 9] = [
    "action-contracts",
    "code-connect",
    "custom-effects",
    "media-paint",
    "mixed-grid",
    "pattern-paint",
    "rest-import",
    "rest-snapshot",
    "shader-paint",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityUsage {
    Used,
    Unused,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityPolicy {
    Required,
    Optional,
    Quarantined,
    Undeclared,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityStatus {
    Available,
    Deferred,
    Missing,
    Violation,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityInventoryEntry {
    pub capability: String,
    pub usage: CapabilityUsage,
    pub policy: CapabilityPolicy,
    pub advertised: bool,
    pub status: CapabilityStatus,
}

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityProfileSummary {
    pub used: usize,
    pub unused: usize,
    pub violations: usize,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityProfileReport {
    pub schema_version: u32,
    pub profile_id: String,
    pub input_fingerprint: ExtractionFingerprint,
    pub passed: bool,
    pub summary: CapabilityProfileSummary,
    pub entries: Vec<CapabilityInventoryEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

struct CapabilityProfile {
    required: &'static [&'static str],
    optional: &'static [&'static str],
    quarantined: &'static [&'static str],
}

const ORBITLINE_MINIMAL: CapabilityProfile = CapabilityProfile {
    required: &REQUIRED_CAPABILITIES,
    optional: &OPTIONAL_CAPABILITIES,
    quarantined: &QUARANTINED_CAPABILITIES,
};

/// Builds a deterministic capability-use inventory and rejects profile drift.
///
/// The extraction manifest advertises capabilities available in this bundle; it
/// does not prove that every node exercises them. The report therefore calls the
/// source fact `advertised` and treats quarantined routes as unused unless their
/// presence creates a violation.
///
/// # Errors
///
/// Returns an error when the input bundle cannot be fingerprinted as canonical
/// JSON, for example after constructing it with a non-finite number in memory.
pub fn evaluate_capability_profile(
    bundle: &ExtractionBundle,
    profile_id: &str,
) -> Result<CapabilityProfileReport, serde_json::Error> {
    let input_fingerprint = extraction_fingerprint(bundle)?;
    let Some(profile) = profile(profile_id) else {
        let diagnostic = profile_diagnostic(
            codes::PROFILE_UNKNOWN,
            format!("unknown capability profile {profile_id:?}"),
            "profile_id",
            "Use a versioned profile supported by this compiler.",
        );
        return Ok(CapabilityProfileReport {
            schema_version: CAPABILITY_PROFILE_REPORT_VERSION,
            profile_id: profile_id.to_owned(),
            input_fingerprint,
            passed: false,
            summary: CapabilityProfileSummary {
                used: 0,
                unused: 0,
                violations: 1,
            },
            entries: Vec::new(),
            diagnostics: vec![diagnostic],
        });
    };

    let observed = observed_capabilities(bundle);
    let (entries, diagnostics) = build_inventory(profile_id, profile, &observed);
    let summary = CapabilityProfileSummary {
        used: entries
            .iter()
            .filter(|entry| entry.usage == CapabilityUsage::Used)
            .count(),
        unused: entries
            .iter()
            .filter(|entry| entry.usage == CapabilityUsage::Unused)
            .count(),
        violations: diagnostics.len(),
    };
    Ok(CapabilityProfileReport {
        schema_version: CAPABILITY_PROFILE_REPORT_VERSION,
        profile_id: profile_id.to_owned(),
        input_fingerprint,
        passed: diagnostics.is_empty(),
        summary,
        entries,
        diagnostics,
    })
}

fn observed_capabilities(bundle: &ExtractionBundle) -> BTreeMap<String, String> {
    let mut observed = BTreeMap::new();
    if let Some(manifest) = &bundle.extraction_manifest {
        for capability in &manifest.capabilities {
            observed.insert(
                capability.to_owned(),
                format!("extraction_manifest.capabilities[{capability}]"),
            );
        }
    }
    if bundle.rest_snapshot.is_some() {
        observed.insert("rest-snapshot".to_owned(), "rest_snapshot".to_owned());
    }
    for extension in bundle.extensions.keys() {
        observed.insert(format!("extension:{extension}"), extension.to_owned());
    }
    observed
}

fn build_inventory(
    profile_id: &str,
    profile: &CapabilityProfile,
    observed: &BTreeMap<String, String>,
) -> (Vec<CapabilityInventoryEntry>, Vec<Diagnostic>) {
    let mut capabilities = observed.keys().cloned().collect::<BTreeSet<_>>();
    capabilities.extend(profile.required.iter().map(|value| (*value).to_owned()));
    capabilities.extend(profile.optional.iter().map(|value| (*value).to_owned()));
    capabilities.extend(profile.quarantined.iter().map(|value| (*value).to_owned()));

    let mut diagnostics = Vec::new();
    let mut entries = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        let advertised = observed.contains_key(&capability);
        let (policy, status) = classify_capability(
            &capability,
            advertised,
            profile_id,
            profile,
            observed,
            &mut diagnostics,
        );
        entries.push(CapabilityInventoryEntry {
            capability,
            usage: if advertised {
                CapabilityUsage::Used
            } else {
                CapabilityUsage::Unused
            },
            policy,
            advertised,
            status,
        });
    }
    (entries, diagnostics)
}

fn classify_capability(
    capability: &str,
    advertised: bool,
    profile_id: &str,
    profile: &CapabilityProfile,
    observed: &BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (CapabilityPolicy, CapabilityStatus) {
    if profile.required.contains(&capability) {
        return classify_required(capability, advertised, diagnostics);
    }
    if profile.optional.contains(&capability) {
        return (
            CapabilityPolicy::Optional,
            if advertised {
                CapabilityStatus::Available
            } else {
                CapabilityStatus::Deferred
            },
        );
    }
    let property_path = observed.get(capability).map_or(
        "extraction_manifest.capabilities",
        std::string::String::as_str,
    );
    if profile.quarantined.contains(&capability) {
        if advertised {
            diagnostics.push(profile_diagnostic(
                codes::PROFILE_QUARANTINED,
                format!("capability {capability:?} is quarantined by profile {profile_id:?}"),
                property_path,
                "Remove the unused route or select a future profile that explicitly permits it.",
            ));
            return (CapabilityPolicy::Quarantined, CapabilityStatus::Violation);
        }
        return (CapabilityPolicy::Quarantined, CapabilityStatus::Deferred);
    }
    diagnostics.push(profile_diagnostic(
        codes::PROFILE_UNDECLARED,
        format!("capability {capability:?} is not declared by profile {profile_id:?}"),
        property_path,
        "Classify the capability in a new versioned profile before using it.",
    ));
    (CapabilityPolicy::Undeclared, CapabilityStatus::Violation)
}

fn classify_required(
    capability: &str,
    advertised: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (CapabilityPolicy, CapabilityStatus) {
    if advertised {
        return (CapabilityPolicy::Required, CapabilityStatus::Available);
    }
    diagnostics.push(profile_diagnostic(
        codes::PROFILE_REQUIRED_MISSING,
        format!("required capability {capability:?} is absent from the extraction bundle"),
        &format!("extraction_manifest.capabilities[{capability}]"),
        "Re-extract with the matching plugin/profile; do not infer the missing capability.",
    ));
    (CapabilityPolicy::Required, CapabilityStatus::Missing)
}

fn profile(profile_id: &str) -> Option<&'static CapabilityProfile> {
    match profile_id {
        ORBITLINE_MINIMAL_PROFILE_V1 => Some(&ORBITLINE_MINIMAL),
        _ => None,
    }
}

fn profile_diagnostic(code: &str, message: String, property_path: &str, help: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message,
        node_id: None,
        property_path: Some(property_path.to_owned()),
        help: Some(help.to_owned()),
    }
}
