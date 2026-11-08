#[test]
fn native_routes_match_reference_choices_scopes_and_return_contexts() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/vanilla_routes.json")).unwrap();
    for case in cases {
        let actual = lom_core::vanilla_routes::analyze(
            &serde_json::from_value(case["sources"].clone()).unwrap(),
            &serde_json::from_value(case["texts"].clone()).unwrap(),
            &serde_json::from_value(case["positions"].clone()).unwrap(),
        );
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["expected"],
            "sources: {}",
            case["sources"]
        );
    }
}
