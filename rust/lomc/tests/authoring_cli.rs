use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn cli(args: &[&str]) -> (i32, Value) {
    let result = Command::new(env!("CARGO_BIN_EXE_lomc"))
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    (
        result.status.code().unwrap(),
        serde_json::from_slice(&result.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&result.stderr))),
    )
}
fn recipe(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
#[test]
fn all_showcase_nodes_and_assets_roundtrip_through_declarative_cli() {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples/showcase3")
        .canonicalize()
        .unwrap();
    let (code, read) = cli(&["project-read", source.to_str().unwrap()]);
    assert_eq!(code, 0, "{read}");
    let mut request = read["project"].clone();
    request["manifest"]["name"] = json!("CLI 对象工程");
    request["stories"]["main"]["title"] = json!("CLI 第一章");
    let input = dir.path().join("request.json");
    recipe(&input, &request);
    let output = dir.path().join("project");
    let (code, result) = cli(&[
        "project",
        input.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{result}");
    let before = lom_core::project::Project::open(&source).unwrap();
    let after = lom_core::project::Project::open(&output).unwrap();
    assert_eq!(before.assets, after.assets);
    assert_eq!(before.paths, after.paths);
    assert_eq!(
        before.manifest["campaign_id"],
        after.manifest["campaign_id"]
    );
    assert_eq!(before.stories.len(), after.stories.len());
    let package = dir.path().join("story.lommod");
    let (code, result) = cli(&[
        "export",
        output.to_str().unwrap(),
        "-o",
        package.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{result}");
    let (code, result) = cli(&["inspect", package.to_str().unwrap()]);
    assert_eq!(code, 0, "{result}");
    let reopened = lom_core::project::Project::open(&package).unwrap();
    assert_eq!(after.stories, reopened.stories);
    assert_eq!(lom_core::project::Project::open(&source).unwrap(), before);
}

#[test]
fn bad_nodes_and_asset_paths_do_not_start_writing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("request.json");
    let output = dir.path().join("project");
    let cases = [
        json!({"unknown":true}),
        json!({"stories":{"main":{"id":"main","start":"s","nodes":[{"id":"s","type":"say","mode":"narrative","text":"hi","goto":"missing"}]}}}),
        json!({"assets":{"assets/../../escape":"input.png"}}),
        json!({"manifest":{"id":"INVALID ID"}}),
    ];
    for value in cases {
        recipe(&input, &value);
        let (code, result) = cli(&[
            "project",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ]);
        assert_eq!(code, 1, "{result}");
        assert_eq!(result["ok"], false);
        assert!(!output.exists(), "failed request wrote output: {result}");
    }
}

#[test]
fn deletions_require_save_as_and_unrelated_outputs_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("original");
    let mut project = lom_core::project::Project::new();
    let mut extra = project.stories["main"].clone();
    extra["id"] = json!("extra");
    project.stories.insert("extra".into(), extra);
    project.save_to(&root).unwrap();
    let input = dir.path().join("request.json");
    recipe(
        &input,
        &json!({"source":root,"stories":{"main":project.stories["main"]}}),
    );
    let (code, result) = cli(&[
        "project",
        input.to_str().unwrap(),
        "-o",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{result}");
    assert_eq!(
        lom_core::project::Project::open(&root)
            .unwrap()
            .stories
            .len(),
        2
    );
    let target = dir.path().join("copy");
    let (code, result) = cli(&[
        "project",
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(
        lom_core::project::Project::open(&target)
            .unwrap()
            .stories
            .len(),
        1
    );
    let original = fs::read(target.join("manifest.json")).unwrap();
    recipe(&input, &json!({"manifest":{"name":"should not overwrite"}}));
    assert_eq!(
        cli(&[
            "project",
            input.to_str().unwrap(),
            "-o",
            target.to_str().unwrap()
        ])
        .0,
        1
    );
    assert_eq!(fs::read(target.join("manifest.json")).unwrap(), original);
}
