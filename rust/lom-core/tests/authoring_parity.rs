use serde_json::Value;
#[test]
fn authoring_api_matches_legacy_success_and_rejection_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/authoring_golden.json")).unwrap();
    assert_eq!(fixture["failures"], 0);
    assert_eq!(fixture["errors"], 0);
    for (i, c) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let r = lom_core::story_api::execute(c["op"].as_str().unwrap(), &c["params"]);
        assert_eq!(
            r.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i} {}: {r:?}",
            c["op"]
        );
        if let Ok(r) = r {
            assert_eq!(r["result"], c["result"], "case {i} result");
            assert_eq!(r["after"], c["after"], "case {i} story");
        }
    }
}
