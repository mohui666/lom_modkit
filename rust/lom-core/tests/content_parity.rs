use lom_core::content;
use serde_json::{json, Value};

// Preserve the Python capture. v1.4.3 deliberately replaces clamping/truncation
// with positive, finite float percentages; these are the reviewed differences.
fn reviewed_scale_expectation(index: usize, legacy: &Value) -> Value {
    let mut expected = legacy.clone();
    let scale = match index {
        7 => Some((json!(200), json!(130), json!(200))),
        44 => Some((json!("88"), json!(88), json!(88.0))),
        45 => Some((json!("88.2"), json!(100), json!(88.2))),
        47 => Some((json!(88.9), json!(88), json!(88.9))),
        48 => Some((json!(999), json!(130), json!(999))),
        _ => None,
    };
    if let Some((input, previous, current)) = scale {
        assert_eq!(legacy["input"]["type"], "character");
        assert_eq!(legacy["input"]["scale"], input);
        assert_eq!(legacy["normalized"]["scale"], previous);
        expected["normalized"]["scale"] = current.clone();
        expected["payload"]["scale"] = current;
    }
    if let Some(input) = match index {
        42 => Some(json!(true)),
        43 => Some(json!(false)),
        46 => Some(json!(0)),
        49 => Some(json!(-1)),
        _ => None,
    } {
        assert_eq!(legacy["input"]["type"], "character");
        assert_eq!(legacy["input"]["scale"], input);
        assert_eq!(legacy["valid"], true);
        expected["valid"] = json!(false);
    }
    if index == 64 {
        assert_eq!(legacy["input"]["intro"]["image_scale"], 999);
        assert_eq!(legacy["normalized"]["intro"]["image_scale"], 160);
        expected["normalized"]["intro"]["image_scale"] = json!(999);
        expected["payload"]["intro"]["image_scale"] = json!(999);
    }
    expected
}

#[test]
fn metadata_matches_legacy_and_reviewed_scale_contract() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/content_golden.json")).unwrap();
    for (i, legacy) in fixtures.as_array().unwrap().iter().enumerate() {
        let case = reviewed_scale_expectation(i, legacy);
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
