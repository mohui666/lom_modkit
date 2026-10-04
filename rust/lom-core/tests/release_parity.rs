//! Captured from 25 original Python stage/release/statistics/coverage regressions.
use lom_core::release::*;
use serde_json::{json, Value};

#[test]
fn original_authoring_release_semantics() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/release_golden.json")).unwrap();
    assert_eq!(fixtures.len(), 97);
    for (index, fixture) in fixtures.iter().enumerate() {
        let args = fixture["args"].as_array().unwrap();
        let expected = &fixture["result"];
        let actual = match fixture["function"].as_str().unwrap() {
            "required_character" => json!(required_character(&args[0])),
            "missing_stage_linear" => json!(missing_stage_linear(
                args[0].as_array().unwrap(),
                args[1].as_u64().unwrap() as usize
            )),
            "find_stage_issues" => json!(find_stage_issues(&args[0])),
            "ensure_stage" => {
                let mut story = args[0].clone();
                let result = json!(ensure_stage(&mut story, args[1].as_str().unwrap()));
                assert_eq!(story, fixture["after"], "ensure_stage after {index}");
                result
            }
            "apply_safe_fixes" => {
                let stories = serde_json::from_value(args[0].clone()).unwrap();
                let proposal = propose_safe_fixes(&stories);
                assert_eq!(
                    json!(proposal.after),
                    fixture["after"],
                    "safe fixes after {index}"
                );
                assert_eq!(proposal.changes.len(), expected.as_array().unwrap().len());
                continue;
            }
            "validate_release_version" => {
                json!(validate_release_version(args[0].as_str().unwrap_or("")))
            }
            "apply_release_profile" => {
                let editing = serde_json::from_value(args[0].clone()).unwrap();
                let stories = serde_json::from_value(args[1].clone()).unwrap();
                let assets: Option<Vec<String>> = serde_json::from_value(
                    fixture["kwargs"]
                        .get("bundled_assets")
                        .cloned()
                        .unwrap_or(Value::Null),
                )
                .unwrap();
                let report = apply_release_profile(
                    editing,
                    &stories,
                    &args[2],
                    args[3].as_str().unwrap(),
                    assets.as_deref(),
                );
                json!(report.into_iter().map(|i|json!({"severity":i.severity,"code":i.code,"story_id":i.story_id,"node_id":i.node_id,"fixable":i.fixable})).collect::<Vec<_>>())
            }
            "calculate_project_statistics" => {
                let stories = serde_json::from_value(args[0].clone()).unwrap();
                let assets: Option<Vec<String>> = args
                    .get(1)
                    .map(|a| serde_json::from_value(a.clone()).unwrap());
                let mut result = calculate_project_statistics(&stories, assets.as_deref());
                result.as_object_mut().unwrap().remove("node_types");
                result
            }
            "calculate_voice_coverage" => {
                let stories = serde_json::from_value(args[0].clone()).unwrap();
                let mut result = calculate_voice_coverage(&stories);
                for key in ["total", "stories", "characters"] {
                    if key == "total" {
                        let row = result[key].as_object_mut().unwrap();
                        row.remove("total");
                        row.remove("percent");
                    } else {
                        for row in result[key].as_array_mut().unwrap() {
                            let o = row.as_object_mut().unwrap();
                            o.remove("total");
                            o.remove("percent");
                        }
                    }
                }
                result
            }
            other => panic!("unknown fixture {other}"),
        };
        assert_eq!(
            actual, *expected,
            "fixture {index}: {}",
            fixture["function"]
        );
    }
}
