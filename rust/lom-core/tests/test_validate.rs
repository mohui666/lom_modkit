use lom_core::validate::{
    editor_data, node_schema, validate_manifest, validate_story, validate_story_with_catalog,
};
use serde_json::{json, Value};

#[test]
fn legacy_validation_acceptance_and_warning_parity() {
    let fixtures: Value =
        serde_json::from_str(include_str!("test_validate_fixtures.json")).unwrap();
    assert_eq!(fixtures["test_failures"], 0);
    assert_eq!(fixtures["test_errors"], 0);
    let mut mismatches = Vec::new();
    let mut valid = 0;
    let mut invalid = 0;
    for (index, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let expected = case["valid"].as_bool().unwrap();
        if expected {
            valid += 1;
        } else {
            invalid += 1;
        }
        let parsed = serde_json::from_str::<Value>(case["input_json"].as_str().unwrap());
        let input = match parsed {
            Ok(value) => value,
            Err(error) => {
                if expected {
                    mismatches.push(format!(
                        "case {index} {}: valid Python input rejected by JSON parser: {error}",
                        case["test"]
                    ));
                }
                continue;
            }
        };
        let result = if case["kind"] == "manifest" {
            validate_manifest(&input).map(|()| vec![])
        } else {
            let mut catalog = editor_data().clone();
            if let Some(overrides) = case["catalog_overrides"].as_object() {
                for (key, value) in overrides {
                    catalog[key] = value.clone();
                }
            }
            validate_story_with_catalog(&input, &catalog)
        };
        if result.is_ok() != expected {
            mismatches.push(format!(
                "case {index} {}: expected valid={expected}; result={result:?}; input={}",
                case["test"], input
            ));
        } else if let Ok(warnings) = result {
            if json!(warnings) != case["warnings"] {
                mismatches.push(format!(
                    "case {index} {}: warning mismatch\nRust: {}\nPython: {}",
                    case["test"],
                    json!(warnings),
                    case["warnings"]
                ));
            }
        }
    }
    assert!(
        valid >= 263 && invalid >= 266,
        "fixture corpus must retain both valid and invalid inputs"
    );
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

fn story(nodes: Value) -> Value {
    json!({"id":"test","start":"n1","nodes":nodes})
}
#[test]
fn schema_exposes_all_63_nodes() {
    assert_eq!(node_schema()["nodes"].as_object().unwrap().len(), 63);
}
#[test]
fn partial_direct_dice_is_rejected_without_panicking() {
    for node in [
        json!({"id":"n1","type":"dice","max":6}),
        json!({"id":"n1","type":"dice","header":"标题"}),
        json!({"id":"n1","type":"dice","bands":[]}),
    ] {
        assert!(validate_story(&story(json!([node]))).is_err());
    }
}
#[test]
fn branch_join_requires_cg_show_on_every_reachable_path() {
    let input = story(json!([
        {"id":"n1","type":"choice","options":[{"text":"展示","goto":"show"},{"text":"跳过","goto":"hide"}]},
        {"id":"show","type":"cg","action":"show","kind":"picture","key":"picture","goto":"hide"},
        {"id":"hide","type":"cg","action":"hide","kind":"picture"},
        {"id":"end","type":"end"}
    ]));
    assert!(validate_story(&input)
        .unwrap_err()
        .to_string()
        .contains("每条可达路径"));
}
#[test]
fn json_nonfinite_numbers_never_enter_native_validator() {
    for number in ["NaN", "Infinity", "-Infinity"] {
        assert!(serde_json::from_str::<Value>(&format!("{{\"value\":{number}}}")).is_err());
    }
}
