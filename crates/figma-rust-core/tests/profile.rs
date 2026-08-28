use figma_rust_core::{
    CapabilityPolicy, CapabilityUsage, ORBITLINE_MINIMAL_PROFILE_V1, evaluate_capability_profile,
    parse_bundle,
};

const FIXTURE: &str = include_str!("../../../fixtures/capability-profile/extraction.json");

#[test]
fn orbitline_minimal_profile_produces_a_deterministic_usage_matrix()
-> Result<(), Box<dyn std::error::Error>> {
    let bundle = parse_bundle(FIXTURE)?;
    let first = evaluate_capability_profile(&bundle, ORBITLINE_MINIMAL_PROFILE_V1)?;
    let second = evaluate_capability_profile(&bundle, ORBITLINE_MINIMAL_PROFILE_V1)?;

    assert_eq!(
        serde_json::to_vec_pretty(&first)?,
        serde_json::to_vec_pretty(&second)?
    );
    assert!(first.passed);
    assert_eq!(first.schema_version, 1);
    assert_eq!(first.profile_id, ORBITLINE_MINIMAL_PROFILE_V1);
    assert_eq!(first.summary.used, 7);
    assert_eq!(first.summary.unused, 9);
    assert_eq!(first.summary.violations, 0);
    assert!(
        first
            .entries
            .windows(2)
            .all(|entries| { entries[0].capability < entries[1].capability })
    );
    assert!(first.entries.iter().any(|entry| {
        entry.capability == "schema-v2"
            && entry.usage == CapabilityUsage::Used
            && entry.policy == CapabilityPolicy::Required
            && entry.advertised
    }));
    assert!(first.entries.iter().any(|entry| {
        entry.capability == "rest-import"
            && entry.usage == CapabilityUsage::Unused
            && entry.policy == CapabilityPolicy::Quarantined
            && !entry.advertised
    }));
    Ok(())
}

#[test]
fn profile_fails_closed_for_missing_quarantined_and_undeclared_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let mut missing = parse_bundle(FIXTURE)?;
    missing
        .extraction_manifest
        .as_mut()
        .expect("fixture manifest")
        .capabilities
        .retain(|capability| capability != "schema-v2");
    let missing_report = evaluate_capability_profile(&missing, ORBITLINE_MINIMAL_PROFILE_V1)?;
    assert!(!missing_report.passed);
    assert!(missing_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "FR-PROFILE-001"
            && diagnostic.property_path.as_deref()
                == Some("extraction_manifest.capabilities[schema-v2]")
    }));

    let mut quarantined = parse_bundle(FIXTURE)?;
    quarantined.rest_snapshot = Some(serde_json::json!({"document": "redacted"}));
    let quarantined_report =
        evaluate_capability_profile(&quarantined, ORBITLINE_MINIMAL_PROFILE_V1)?;
    assert!(!quarantined_report.passed);
    assert!(quarantined_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "FR-PROFILE-002"
            && diagnostic.property_path.as_deref() == Some("rest_snapshot")
    }));

    let mut undeclared = parse_bundle(FIXTURE)?;
    undeclared
        .extraction_manifest
        .as_mut()
        .expect("fixture manifest")
        .capabilities
        .push("future-half-route".to_owned());
    undeclared.extensions.insert(
        "experimental_route".to_owned(),
        serde_json::json!({"enabled": true}),
    );
    let undeclared_report = evaluate_capability_profile(&undeclared, ORBITLINE_MINIMAL_PROFILE_V1)?;
    assert!(!undeclared_report.passed);
    assert_eq!(
        undeclared_report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "FR-PROFILE-003")
            .count(),
        2
    );
    assert!(
        undeclared_report.diagnostics.iter().any(|diagnostic| {
            diagnostic.property_path.as_deref() == Some("experimental_route")
        })
    );
    Ok(())
}
