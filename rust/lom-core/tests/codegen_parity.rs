//! Golden outputs captured from the previous Python compiler and its regression suite.
//! These fixtures are data only; running the native compiler/tests does not invoke Python.
use lom_core::{codegen, content};
use serde_json::Value;
#[test]
fn all_63_node_types_match_legacy_lua_byte_for_byte() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/codegen_golden.json")).unwrap();
    let mut types = std::collections::BTreeSet::new();
    for (i, fixture) in fixtures.as_array().unwrap().iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        if let Some(files) = fixture["content"].as_object() {
            for (relative, metadata) in files {
                let path = directory.path().join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, serde_json::to_vec(metadata).unwrap()).unwrap();
                for file in content::listed_content_files(
                    &content::normalize_content_metadata(metadata).unwrap(),
                ) {
                    std::fs::write(path.parent().unwrap().join(file), b"fixture").unwrap();
                }
            }
        }
        let actual = codegen::story_to_lua(
            &fixture["story"],
            fixture.get("mod_info").filter(|v| !v.is_null()),
            fixture["source"].as_str(),
            Some(directory.path()),
        )
        .unwrap_or_else(|e| panic!("fixture {i}: {e:#}"));
        let expected = fixture["lua"].as_str().unwrap();
        if actual != expected {
            let line = actual
                .lines()
                .zip(expected.lines())
                .position(|(a, b)| a != b)
                .unwrap_or(actual.lines().count().min(expected.lines().count()));
            panic!(
                "fixture {i} differs at line {}: actual {:?}, expected {:?}",
                line + 1,
                actual.lines().nth(line),
                expected.lines().nth(line)
            );
        }
        for node in fixture["story"]["nodes"].as_array().unwrap() {
            types.insert(node["type"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(types.len(), 63);
}
#[test]
fn numeric_literals_match_python_boundaries() {
    for (raw, expected) in [
        ("-0.0", "0"),
        ("0.1200", "0.12"),
        (
            "123456789012345678901234567890123456789",
            "123456789012345678901234567890123456789",
        ),
        ("0.000001", "1e-06"),
        ("0.0001", "0.0001"),
        ("1000000000000000.0", "1000000000000000.0"),
        ("1e16", "1e+16"),
        ("1e-20", "1e-20"),
        ("-1e20", "-1e+20"),
    ] {
        let v: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(codegen::lua_num(&v).unwrap(), expected, "{raw}");
    }
    assert!(codegen::lua_num(&Value::Bool(true)).is_err());
}
