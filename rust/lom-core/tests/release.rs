use lom_core::{project::Project, release::*};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
};

fn story(nodes: Value) -> Value {
    json!({"story_schema":2,"id":"main","title":"发布剧情","start":nodes[0]["id"],"nodes":nodes})
}
fn project() -> Project {
    let mut p = Project::new();
    p.manifest["id"] = json!("release_demo");
    p.manifest["version"] = json!("1.2.3");
    p.stories.insert("main".into(),story(json!([{"id":"say1","type":"say","mode":"narrative","text":"完成对白"},{"id":"end1","type":"end"}])));
    p
}

#[test]
fn stage_guard_branch_join_and_reversible_repair() {
    let source = story(json!([
        {"id":"n1","type":"choice","options":[{"text":"a","goto":"n2"},{"text":"b","goto":"n3"}]},
        {"id":"n2","type":"show","character":"player","position":"M","goto":"n4"},
        {"id":"n3","type":"say","mode":"narrative","text":"旁白","goto":"n4"},
        {"id":"n4","type":"rotate","character":"player","angle":180,"duration":1},
        {"id":"n5","type":"end"}
    ]));
    assert_eq!(
        find_stage_issues(&source),
        vec![("n4".into(), "player".into())]
    );
    let original = BTreeMap::from([("main".into(), source.clone())]);
    let proposal = propose_safe_fixes(&original);
    assert_eq!(original["main"], source);
    assert_eq!(proposal.changes.len(), 1);
    let mut current = original.clone();
    proposal.apply(&mut current).unwrap();
    assert!(find_stage_issues(&current["main"]).is_empty());
    let show_id = current["main"]["nodes"][3]["id"].as_str().unwrap();
    assert_eq!(current["main"]["nodes"][1]["goto"], show_id);
    assert_eq!(current["main"]["nodes"][2]["goto"], show_id);
    proposal.undo(&mut current).unwrap();
    assert_eq!(current, original);
    current.get_mut("main").unwrap()["title"] = json!("new edit");
    assert!(proposal.apply(&mut current).is_err());
}

#[test]
fn guard_preserves_self_loop_and_retargets_all_branch_forms() {
    let mut s = story(json!([
        {"id":"check","type":"stat_check","success":"act","failure":"act"},
        {"id":"battle","type":"battle_result","win":"act","lose":"act"},
        {"id":"die","type":"dice","options":[{"goto_成功":"act","goto_大成功":"act","goto_失败":"act"}],"bands":[{"goto":"act"}]},
        {"id":"branch","type":"branch","cases":[{"goto":"act"}]},
        {"id":"act","type":"rotate","character":"player","goto":"act"}
    ]));
    s["start"] = json!("act");
    let show = ensure_stage(&mut s, "act").unwrap();
    assert_eq!(s["start"], show["id"]);
    assert_eq!(s["nodes"][5]["goto"], "act");
    for (i, key) in [(0, "success"), (0, "failure"), (1, "win"), (1, "lose")] {
        assert_eq!(s["nodes"][i][key], show["id"]);
    }
    assert_eq!(s["nodes"][2]["options"][0]["goto_失败"], show["id"]);
    assert_eq!(s["nodes"][3]["cases"][0]["goto"], show["id"]);
    assert!(find_stage_issues(&s).is_empty());
}

#[test]
fn preflight_uses_editor_terminal_and_cross_chapter_cycle_rules() {
    let mut p = project();
    p.stories.insert(
        "main".into(),
        story(json!([{"id":"loop","type":"end","next_script":"side"}])),
    );
    let mut side = story(json!([{"id":"return","type":"end","next_script":"main"}]));
    side["id"] = json!("side");
    p.stories.insert("side".into(), side);
    let cyclic = run_preflight(&p, Profile::Editing, "1.1.2");
    assert_eq!(cyclic.iter().filter(|i| i.code == "no_exit_scc").count(), 2);
    p.stories.get_mut("side").unwrap()["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"id":"raw","type":"raw","lua":"-- author-controlled flow"}),
        );
    p.stories.get_mut("side").unwrap()["start"] = json!("raw");
    assert!(!run_preflight(&p, Profile::Editing, "1.1.2")
        .iter()
        .any(|i| i.code == "no_exit_scc"));
    p.stories.remove("side");
    p.stories.insert("main".into(),story(json!([
        {"id":"loop","type":"choice","options":[{"text":"重试","goto":"loop"},{"text":"战斗","goto":"battle"}]},
        {"id":"battle","type":"battle_result","win":"loop","lose":"loop"}
    ])));
    assert!(!run_preflight(&p, Profile::Editing, "1.1.2")
        .iter()
        .any(|i| i.code == "no_exit_scc"));
    p.stories.insert(
        "main".into(),
        story(json!([{"id":"say","type":"say","mode":"narrative","text":"未写完"}])),
    );
    let draft = run_preflight(&p, Profile::Editing, "1.1.2");
    assert!(draft
        .iter()
        .any(|i| i.code == "broken_flow" && i.severity == "warning"));
    assert!(!draft.iter().any(|i| i.code == "missing_ending"));
    p.assets.insert(
        "assets/user/image/demo.orphan/photo.png".into(),
        vec![1, 2, 3],
    );
    assert!(run_preflight(&p, Profile::Editing, "1.1.2")
        .iter()
        .any(|i| i.code == "stale_content_metadata"));
}

#[test]
fn release_profile_promotes_only_placeholder_and_validates_semver() {
    let mut p = project();
    p.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("在这里填写对白");
    p.stories.get_mut("main").unwrap()["localization"] = json!({"default_locale":"chs","fallback_locale":"chs","translations":{"ja":{"story.title":"物語"}}});
    p.manifest["min_host_version"] = json!("9.0.0");
    p.manifest["tested_host_version"] = json!("9.0.0");
    p.assets.insert("assets/unused.wav".into(), vec![1, 2]);
    let editing = run_preflight(&p, Profile::Editing, "1.1.2");
    assert!(editing
        .iter()
        .any(|i| i.code == "placeholder_text" && i.severity == "warning"));
    let release = run_preflight(&p, Profile::Release, "1.1.2");
    for code in ["placeholder_text", "incompatible_runtime_requirement"] {
        assert!(
            release
                .iter()
                .any(|i| i.code == code && i.severity == "error"),
            "{code}"
        );
    }
    assert_eq!(
        release
            .iter()
            .filter(|i| i.code == "missing_locale")
            .count(),
        2
    );
    assert!(release
        .iter()
        .any(|i| i.code == "unused_critical_asset" && i.severity == "warning"));
    for good in ["0.0.0", "1.2.3", "2.0.0-beta.1", "1.0.0+build.001"] {
        assert_eq!(validate_release_version(good), None, "{good}");
    }
    for bad in [
        "1.2",
        "01.2.3",
        "1.2.3-01",
        "2147483648.0.0",
        "1.2.3-alpha.2147483648",
    ] {
        assert!(validate_release_version(bad).is_some(), "{bad}");
    }
}

#[test]
fn build_checksums_and_blocked_build_preserves_existing_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let p = project();
    let result = build_release_directory(&p, root.path(), "1.1.2").unwrap();
    assert_eq!(result.story_count, 1);
    assert_eq!(result.node_count, 2);
    assert_eq!(result.compile_report.len(), 1);
    let bytes = fs::read(&result.package_path).unwrap();
    let hash = format!("{:X}", Sha256::digest(&bytes));
    assert_eq!(result.package_sha256, hash);
    assert_eq!(result.package_size, bytes.len() as u64);
    assert_eq!(
        fs::read_to_string(&result.checksum_path).unwrap(),
        format!("{hash}  release_demo-1.2.3.lommod\n")
    );
    assert!(fs::read_to_string(result.readme_path.unwrap())
        .unwrap()
        .contains("未执行 Windows 或游戏实机测试"));
    lom_core::package::read_package(&result.package_path).unwrap();
    let mut broken = p;
    broken.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("在这里填写对白");
    let err = build_release(&broken, &result.package_path, "1.1.2").unwrap_err();
    let blocked = err.downcast_ref::<ReleaseBuildBlocked>().unwrap();
    assert!(blocked.issues.iter().any(|i| i.code == "placeholder_text"));
    assert_eq!(fs::read(&result.package_path).unwrap(), bytes);
    let missing = root.path().join("not_created.lommod");
    assert!(build_release(&broken, &missing, "1.1.2").is_err());
    assert!(!missing.exists());
}

#[test]
fn content_preflight_distinguishes_type_portrait_and_hidden_images() {
    let mut p = project();
    p.stories.get_mut("main").unwrap()["nodes"] = json!([
        {"id":"say1","type":"show","character":"user:demo.hero","position":"M","portrait":"angry"},
        {"id":"music1","type":"music","name":"user:demo.hero"},
        {"id":"overlay1","type":"overlay","action":"hide","image":"user:missing"},
        {"id":"end1","type":"end"}
    ]);
    p.assets.insert("assets/user/character/demo.hero/content.json".into(),serde_json::to_vec(&json!({"content_schema":1,"id":"demo.hero","name":"Hero","type":"character","files":{"main":"normal.png"},"portraits":{"normal":"normal.png"}})).unwrap());
    p.assets.insert(
        "assets/user/character/demo.hero/normal.png".into(),
        b"test image bytes".to_vec(),
    );
    let issues = run_preflight(&p, Profile::Editing, "1.1.2");
    assert!(issues
        .iter()
        .any(|i| i.code == "wrong_user_content_type" && i.node_id == "music1"));
    assert!(
        issues
            .iter()
            .any(|i| i.code == "missing_portrait" && i.node_id == "say1"),
        "{issues:?}"
    );
    assert!(!issues
        .iter()
        .any(|i| i.code == "missing_image" && i.node_id == "overlay1"));
}

#[test]
fn statistics_and_missing_voice_report_use_dialogue_roles() {
    let stories = BTreeMap::from([
        (
            "main".into(),
            json!({"id":"main","title":"主线","start":"a","nodes":[{"id":"a","type":"say","character":"player","text":"有声","voice":"user:demo.a"},{"id":"b","type":"say","character":"player","text":"无声"},{"id":"c","type":"say","mode":"narrative","text":"旁白"}]}),
        ),
        (
            "side".into(),
            json!({"id":"side","title":"支线","start":"d","nodes":[{"id":"d","type":"say","mode":"center","text":"居中旁白","voice":"user:demo.d"},{"id":"e","type":"say","character":"user:demo.hero","text":"自定义人物无声"},{"id":"end","type":"end"}]}),
        ),
    ]);
    let report = calculate_voice_coverage(&stories);
    assert_eq!(report["total"]["voiced"], 2);
    assert_eq!(report["total"]["unvoiced"], 3);
    assert_eq!(report["total"]["percent"], 40.0);
    assert_eq!(report["characters"][0]["key"], "__narrator__");
    assert_eq!(
        report["unvoiced_dialogues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["node_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["b", "c", "e"]
    );
    let assets = vec![
        "assets/user/audio/demo.a/content.json".into(),
        "assets/user/audio/demo.a/voice.wav".into(),
        "assets/unused.png".into(),
    ];
    assert_eq!(
        unused_asset_paths(&stories, &assets),
        vec!["assets/unused.png"]
    );
    let stats = calculate_project_statistics(&stories, Some(&assets));
    assert_eq!(stats["nodes"], 6);
    assert_eq!(stats["dialogue_count"], 5);
    assert_eq!(stats["unused_assets"], 1);
    assert_eq!(stats["node_types"]["say"], 5);
    assert!(calculate_project_statistics(&stories, None)["unused_assets"].is_null());
}

#[test]
fn diagnostic_fixed_allowlist_limits_and_privacy() {
    let root = tempfile::tempdir().unwrap();
    let game = root.path().join("steamapps/common/LegendOfMortal");
    fs::create_dir_all(game.join("BepInEx/plugins/MortalModHost")).unwrap();
    let installed = game.join("BepInEx/plugins/MortalModHost/MortalModHost.dll");
    fs::write(&installed, b"own runtime").unwrap();
    let bundled = root.path().join("bundled.dll");
    fs::write(&bundled, b"own runtime").unwrap();
    fs::write(
        root.path().join("steamapps/appmanifest_1859910.acf"),
        "\"buildid\" \"20337760\"",
    )
    .unwrap();
    fs::write(game.join("BepInEx/LogOutput.log"),format!("MortalModHost 9.8.7 启动 {}\nUNRELATED_PRIVATE_PAYLOAD\nMortalModHost C:\\Users\\Alice\\private.txt\nMortalModHost {}",game.display(),"x".repeat(MAX_LOG_OUTPUT_CHARS+1000))).unwrap();
    let crash = root.path().join("editor.log");
    fs::write(
        &crash,
        format!(
            "crash at {}",
            root.path().join("private/source.rs").display()
        ),
    )
    .unwrap();
    let opts = DiagnosticOptions {
        game_root: Some(game.clone()),
        bundled_runtime: Some(bundled),
        crash_log: Some(crash),
        ..Default::default()
    };
    let mut p = project();
    p.stories.get_mut("main").unwrap()["title"] = json!("PRIVATE_TITLE");
    p.stories.get_mut("main").unwrap()["nodes"][0]["text"] = json!("DIALOGUE_SECRET");
    p.stories.get_mut("main").unwrap()["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"raw1","type":"raw","lua":"RAW_LUA_SECRET"}));
    p.manifest["description"] = json!(format!("asset at {}/private/plot.txt", game.display()));
    let issue = PreflightIssue {
        severity: "error".into(),
        code: "missing_image".into(),
        story_id: "main".into(),
        node_id: "say1".into(),
        message: format!("missing at {}/assets/secret.png", game.display()),
        fixable: false,
    };
    let issues = vec![issue; MAX_COLLECTION_ITEMS + 2];
    let output =
        export_diagnostic_bundle(&root.path().join("result.diagnostics"), &p, &issues, &opts)
            .unwrap();
    let mut archive = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
    let expected = BTreeSet::from([
        "diagnostic.json",
        "validation.json",
        "logs/editor-crash.log",
        "logs/runtime.log",
        "README.txt",
    ]);
    assert_eq!(archive.file_names().collect::<BTreeSet<_>>(), expected);
    let mut entries = BTreeMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut data = String::new();
        file.read_to_string(&mut data).unwrap();
        entries.insert(file.name().to_owned(), data);
    }
    let diagnostic: Value = serde_json::from_str(&entries["diagnostic.json"]).unwrap();
    let validation: Value = serde_json::from_str(&entries["validation.json"]).unwrap();
    assert_eq!(diagnostic["runtime_version"], "1.1.2");
    assert_eq!(diagnostic["detected_game_version"], "Steam build 20337760");
    assert_eq!(diagnostic["project_metadata"]["raw_node_count"], 1);
    assert_eq!(validation["errors"], 1002);
    assert_eq!(validation["truncated"], 2);
    assert_eq!(validation["issues"].as_array().unwrap().len(), 1000);
    assert!(entries["logs/runtime.log"].chars().count() <= MAX_LOG_OUTPUT_CHARS);
    let all = entries.into_values().collect::<Vec<_>>().join("\n");
    for secret in [
        "UNRELATED_PRIVATE_PAYLOAD",
        "DIALOGUE_SECRET",
        "RAW_LUA_SECRET",
        "PRIVATE_TITLE",
        "C:\\Users\\Alice",
        root.path().to_str().unwrap(),
    ] {
        assert!(!all.contains(secret), "leaked {secret}");
    }
    assert!(all.contains("<game-dir>"));
    fs::write(installed, b"different").unwrap();
    assert_eq!(
        detect_runtime_version(&opts, "MortalModHost 9.8.7 启动"),
        "9.8.7"
    );
    assert_eq!(detect_game_version(None), "not configured");
}
