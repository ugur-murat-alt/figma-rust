use figma_rust_core::parse_and_normalize;

#[test]
fn basic_raw_to_ir_matches_golden_fixture() {
    let raw = include_str!("fixtures/basic.raw.json");
    let expected: serde_json::Value = serde_json::from_str(include_str!("fixtures/basic.ir.json"))
        .expect("golden IR JSON must parse");
    let actual = serde_json::to_value(parse_and_normalize(raw).expect("raw fixture must parse"))
        .expect("normalized IR must serialize");
    assert_eq!(actual, expected);
}
