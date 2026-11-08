use serde_json::Value;
// The invalid-portrait fixture uses a permanently unknown key: brother4/angry3
// became a valid official expression in the refreshed game catalog.
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
            // Retain the Python capture. Reviewed v1.4.5 changes only the new
            // effect node's default coordinate mode and centered position.
            let mut expected = c.clone();
            // New dim nodes now expire at the next dialogue. Keep the captured
            // legacy fixture untouched; existing nodes without mode stay manual.
            if c["op"] == "add_node" && c["params"]["node_type"] == "dim" {
                assert!(c["result"].get("mode").is_none());
                expected["result"]["mode"] = serde_json::json!("until_dialogue");
                let generated = expected["result"].clone();
                let id = generated["id"].clone();
                *expected["after"]["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|node| node["id"] == id)
                    .unwrap() = generated;
            }
            if i == 62 {
                assert_eq!(c["op"], "add_node");
                assert_eq!(c["params"]["node_type"], "effect");
                assert_eq!(c["result"]["x"], 0);
                assert_eq!(c["result"]["y"], 0);
                expected["result"]["space"] = serde_json::json!("screen");
                expected["result"]["x"] = serde_json::json!(50);
                expected["result"]["y"] = serde_json::json!(50);
                let generated = expected["result"].clone();
                let generated_id = generated["id"].clone();
                *expected["after"]["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|node| node["id"] == generated_id)
                    .unwrap() = generated;
            }
            assert_eq!(r["result"], expected["result"], "case {i} result");
            assert_eq!(r["after"], expected["after"], "case {i} story");
        }
    }
}
