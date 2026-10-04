use lom_core::content;
use serde_json::Value;
#[test]
fn metadata_matches_legacy_normalization_and_payload() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/content_golden.json")).unwrap();
    for (i, case) in fixtures.as_array().unwrap().iter().enumerate() {
        let actual = content::normalize_content_metadata(&case["input"]);
        if case["valid"] == false {
            assert!(actual.is_err(), "case {i} should reject");
            continue;
        }
        let actual = actual.unwrap_or_else(|e| panic!("case {i}: {e}"));
        assert_eq!(actual, case["normalized"], "case {i} normalization");
        assert_eq!(
            content::content_metadata_payload(&actual),
            case["payload"],
            "case {i} payload"
        );
        assert_eq!(
            serde_json::to_value(content::listed_content_files(&actual)).unwrap(),
            case["files"],
            "case {i} files"
        );
    }
}
