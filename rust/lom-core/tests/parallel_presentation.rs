use lom_core::{codegen, package, parallel, project::Project, release, validate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

fn presentation_actions(character: &str) -> Vec<Value> {
    vec![
        json!({"type":"move","character":character,"from":"L1","to":"R1","duration":0.5}),
        json!({"type":"face","character":character,"facing":"left"}),
        json!({"type":"rotate","character":character,"angle":450,"duration":1.75}),
        json!({"type":"mood","character":character,"enabled":true,"icon":"shock","wait_after":0.25}),
    ]
}

fn story(character: &str, actions: Vec<Value>) -> Value {
    json!({"story_schema":2,"id":"main","start":"enter","mood":false,"nodes":[
        {"id":"enter","type":"show","character":character,"position":"L1","portrait":"normal","fadeDuration":0,"moveDuration":0},
        {"id":"together","type":"parallel","actions":actions},
        {"id":"line","type":"say","character":character,"text":"转身、旋转、移动时保留气泡。"},
        {"id":"done","type":"end"}
    ]})
}

fn parallel_body(lua: &str) -> &str {
    lua.split("-- [together] parallel")
        .nth(1)
        .unwrap()
        .split("-- [line] say")
        .next()
        .unwrap()
}

#[test]
fn independent_character_channels_compile_in_every_action_order_and_keep_mood() {
    for motion_kind in ["move", "offset"] {
        for character in ["player", "user:demo.actor"] {
            let mut actions = presentation_actions(character);
            if motion_kind == "offset" {
                // The editor labels this default pixel-offset node 人物位移.
                actions[0] =
                    json!({"type":"offset","character":character,"x":20,"y":-10,"duration":0.5});
            }
            let motion_call = match (motion_kind, character) {
                ("move", _) => {
                    format!("mod_character_move_preserve(\"{character}\", \"L1\", \"R1\", 0.5)")
                }
                ("offset", "player") => {
                    "runwait(characters.MoveOffsetCoroutine(\"player\", 20, -10, 0.5))".into()
                }
                ("offset", _) => format!("mod_char_offset(\"{character}\", 20, -10, 0.5)"),
                _ => unreachable!(),
            };
            let mut permutations = 0;
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        for d in 0..4 {
                            if [a, b, c, d]
                                .iter()
                                .copied()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                                != 4
                            {
                                continue;
                            }
                            let source =
                                story(character, [a, b, c, d].map(|i| actions[i].clone()).into());
                            let before = source.clone();
                            validate::validate_story(&source).unwrap();
                            assert!(release::find_stage_issues(&source).is_empty());
                            let lua = codegen::compile_story(&source, None, None, None).unwrap();
                            let body = parallel_body(&lua);
                            for expected in [
                                motion_call.clone(),
                                format!("mod_character_face_preserve(\"{character}\", \"left\")"),
                                format!("mod_character_mood(\"{character}\", \"shock\")"),
                                format!(
                                    "{}(\"{character}\", 450, 1.75)",
                                    if character == "player" {
                                        "characters.Rotate"
                                    } else {
                                        "mod_char_rotate"
                                    }
                                ),
                            ] {
                                assert_eq!(
                                    body.matches(&expected).count(),
                                    1,
                                    "{motion_kind} order {a}{b}{c}{d}: {body}"
                                );
                            }
                            assert!(
                                !body.contains("stage.show"),
                                "Independent channels must preserve the holder: {body}"
                            );
                            assert!(!body.contains("mod_char_move("));
                            assert!(!body.contains("mod_char_face("));
                            assert!(!lua.contains("mod_hide_mood()"));
                            // Rotation is the longest branch. Its completion wait
                            // must occur once, including the official void API path.
                            assert_eq!(body.matches("wait(1.75)").count(), 1, "{body}");
                            // Official pixel offsets already await their native
                            // coroutine; move/custom-offset branches wait explicitly.
                            assert_eq!(
                                body.matches("wait(0.5)").count(),
                                usize::from(motion_kind == "move" || character != "player"),
                                "{body}"
                            );
                            assert_eq!(body.matches("wait(0.25)").count(), 1, "{body}");
                            assert_eq!(source, before);
                            permutations += 1;
                        }
                    }
                }
            }
            assert_eq!(permutations, 24);
        }
    }
}

#[test]
fn competing_channels_still_name_both_action_numbers_types_and_the_character_property() {
    let actions = presentation_actions("player");
    let move_action = actions[0].clone();
    let face = actions[1].clone();
    let rotate = actions[2].clone();
    let bubble = actions[3].clone();
    for (left, right, property, left_label, right_label) in [
        (
            move_action.clone(),
            json!({"type":"offset","character":"player","x":20,"y":0,"duration":0.5}),
            "位置",
            "人物位移",
            "人物偏移／大小",
        ),
        (
            rotate.clone(),
            rotate.clone(),
            "角度",
            "人物旋转",
            "人物旋转",
        ),
        (face.clone(), face.clone(), "朝向", "人物转向", "人物转向"),
        (
            json!({"type":"show","character":"player","position":"M"}),
            move_action,
            "位置",
            "人物登场",
            "人物位移",
        ),
        (
            bubble.clone(),
            bubble.clone(),
            "气泡",
            "气泡表情",
            "气泡表情",
        ),
        (
            json!({"type":"hide","character":"player"}),
            bubble,
            "气泡",
            "人物退场",
            "气泡表情",
        ),
    ] {
        for reversed in [false, true] {
            let (first, second, first_label, second_label) = if reversed {
                (right.clone(), left.clone(), right_label, left_label)
            } else {
                (left.clone(), right.clone(), left_label, right_label)
            };
            let children = vec![
                json!({"type":"wait","seconds":0}),
                first,
                json!({"type":"wait","seconds":0}),
                second,
            ];
            let error = parallel::validate_group(&json!({"type":"parallel","actions":children}))
                .unwrap_err()
                .to_string();
            for expected in [
                "第 2 和第 4",
                &format!("人物 player 的{property}"),
                &format!("（{first_label} / {second_label}）"),
            ] {
                assert!(error.contains(expected), "{error}");
            }
            let error = validate::validate_story(&story("player", children)).unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains("together"), "{message}");
            assert!(message.contains("第 2 和第 4"), "{message}");
        }
    }
}

#[test]
fn only_independent_transform_combinations_require_the_new_host_bridge() {
    let actions = presentation_actions("player");
    let directory = tempfile::tempdir().unwrap();
    for (label, children, required) in [
        (
            "move_face",
            vec![actions[0].clone(), actions[1].clone()],
            "1.5.15",
        ),
        (
            "move_rotate",
            vec![actions[0].clone(), actions[2].clone()],
            "1.5.15",
        ),
        (
            "face_rotate",
            vec![actions[1].clone(), actions[2].clone()],
            "1.5.15",
        ),
        (
            "offset_rotate",
            vec![
                json!({"type":"offset","character":"player","x":20,"y":0,"duration":0.5}),
                actions[2].clone(),
            ],
            "1.5.15",
        ),
        (
            "legacy_move_bubble",
            vec![actions[0].clone(), actions[3].clone()],
            "1.5.14",
        ),
        (
            "legacy_rotate_bubble",
            vec![actions[2].clone(), actions[3].clone()],
            "1.5.14",
        ),
    ] {
        let mut project = Project::new();
        project.manifest["min_host_version"] = json!("1.5.14");
        project
            .stories
            .insert("main".into(), story("player", children));
        let output = directory.path().join(format!("{label}.lommod"));
        project.export(&output).unwrap();
        let restored = Project::open(&output).unwrap();
        assert_eq!(restored.manifest["min_host_version"], required, "{label}");
        assert_eq!(restored.stories, project.stories);
        assert_eq!(project.manifest["min_host_version"], "1.5.14");
    }
}

fn assert_substituted_lua_is_rejected(path: &Path, output: &Path) {
    let mut entries = package::read_package(path).unwrap();
    entries.remove("package-content.sha256");
    entries
        .get_mut("lua/main.lua")
        .unwrap()
        .extend_from_slice(b"\nerror('unrelated Lua replacement')\n");
    let previous_record = std::str::from_utf8(&entries["story-lua.sha256"]).unwrap();
    let mut record = String::from("algorithm=lom-story-lua-sha256-v1\n");
    for line in previous_record
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
    {
        let columns: Vec<_> = line.split('\t').collect();
        record.push_str(&format!(
            "{}\t{:X}\t{}\t{:X}\n",
            columns[0],
            Sha256::digest(&entries[columns[0]]),
            columns[2],
            Sha256::digest(&entries[columns[2]])
        ));
    }
    entries.insert("story-lua.sha256".into(), record.into_bytes());
    // Recompute archive integrity, so the rejection specifically proves that
    // source ownership was not weakened to accept arbitrary regenerated Lua.
    package::write_package(entries, output).unwrap();
    let inspection = package::inspect_package(output).unwrap();
    assert_eq!(inspection["integrity_verified"], true);
    assert_eq!(inspection["source_lua_match"], false);
    assert!(Project::open(output).is_err());
}

fn released_package_edit_roundtrip(name: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let original_bytes = std::fs::read(&fixture).unwrap();
    let original_entries = package::read_package(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let old_story = project.stories["main"].clone();
    let old_assets = project.assets.clone();
    let old_identity = (
        project.manifest["id"].clone(),
        project.manifest["campaign_id"].clone(),
    );
    let directory = tempfile::tempdir().unwrap();
    let unchanged = directory.path().join("unchanged.lommod");
    project.export(&unchanged).unwrap();
    assert_eq!(
        package::read_package(&unchanged).unwrap()["lua/main.lua"],
        original_entries["lua/main.lua"]
    );
    assert_eq!(Project::open(&unchanged).unwrap().stories, project.stories);
    assert_substituted_lua_is_rejected(&fixture, &directory.path().join("old-substituted.lommod"));

    let chapter = project.stories.get_mut("main").unwrap();
    let nodes = chapter["nodes"].as_array_mut().unwrap();
    nodes.insert(0, json!({"id":"presentation_enter","type":"show","character":"player","position":"L1","portrait":"normal","fadeDuration":0,"moveDuration":0,"goto":"presentation_group"}));
    nodes.insert(1, json!({"id":"presentation_group","type":"parallel","actions":presentation_actions("player"),"goto":old_story["start"]}));
    chapter["start"] = json!("presentation_enter");
    let folder = directory.path().join("edited");
    project.save_to(&folder).unwrap();
    let saved = Project::open(&folder).unwrap();
    assert_eq!(saved.stories, project.stories);
    assert_eq!(saved.assets, old_assets);
    let exported = directory.path().join("edited.lommod");
    saved.export(&exported).unwrap();
    let restored = Project::open(&exported).unwrap();
    assert_eq!(restored.stories, saved.stories);
    assert_eq!(restored.assets, old_assets);
    assert_eq!(restored.manifest["min_host_version"], "1.5.15");
    assert_eq!(
        (
            restored.manifest["id"].clone(),
            restored.manifest["campaign_id"].clone()
        ),
        old_identity
    );
    assert_eq!(
        &restored.stories["main"]["nodes"].as_array().unwrap()[2..],
        old_story["nodes"].as_array().unwrap()
    );
    let entries = package::read_package(&exported).unwrap();
    let lua = std::str::from_utf8(&entries["lua/main.lua"]).unwrap();
    assert!(lua.contains("mod_character_move_preserve(\"player\", \"L1\", \"R1\", 0.5)"));
    assert!(lua.contains("mod_character_face_preserve(\"player\", \"left\")"));
    assert!(lua.contains("mod_character_mood(\"player\", \"shock\")"));
    assert_substituted_lua_is_rejected(&exported, &directory.path().join("new-substituted.lommod"));
    assert_eq!(std::fs::read(&fixture).unwrap(), original_bytes);
}

#[test]
fn released_1511_package_can_add_independent_channels_save_export_and_reopen() {
    released_package_edit_roundtrip("released-1.5.11-render-base.lommod");
}

#[test]
fn released_1514_parallel_package_retains_historical_lua_and_accepts_new_independent_channels() {
    // This fixture was actually packed with the published v1.5.14 lomc.exe,
    // extracted from that editor's immutable components/1.5.14-25fd4f184e25c8c0.
    // Its original group has one move plus a wait and uses legacy stage.show;
    // it was not produced by today's compiler or by an emulated old emitter.
    released_package_edit_roundtrip("released-1.5.14-parallel-base.lommod");
}

#[test]
fn released_1514_package_can_add_beautified_parallel_arrival_save_export_and_reopen() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.14-parallel-base.lommod");
    let original_bytes = std::fs::read(&fixture).unwrap();
    let original_entries = package::read_package(&fixture).unwrap();
    let mut project = Project::open(&fixture).unwrap();
    let old_story = project.stories["main"].clone();
    let old_assets = project.assets.clone();
    let directory = tempfile::tempdir().unwrap();
    let unchanged = directory.path().join("unchanged.lommod");
    project.export(&unchanged).unwrap();
    assert_eq!(
        package::read_package(&unchanged).unwrap()["lua/main.lua"],
        original_entries["lua/main.lua"]
    );
    assert_substituted_lua_is_rejected(&fixture, &directory.path().join("old-substituted.lommod"));

    let chapter = project.stories.get_mut("main").unwrap();
    chapter["nodes"].as_array_mut().unwrap().insert(0, json!({
        "id":"beautified_arrival","type":"parallel","goto":old_story["start"],"actions":[
            {"type":"show","character":"player","appearance":"beautified","portrait":"normal","position":"L1"},
            {"type":"show","character":"brother1","portrait":"normal","position":"R1"}
        ]
    }));
    chapter["start"] = json!("beautified_arrival");
    let folder = directory.path().join("edited");
    project.save_to(&folder).unwrap();
    let saved = Project::open(&folder).unwrap();
    assert_eq!(saved.stories, project.stories);
    assert_eq!(saved.assets, old_assets);
    let exported = directory.path().join("edited.lommod");
    saved.export(&exported).unwrap();
    let restored = Project::open(&exported).unwrap();
    assert_eq!(restored.stories, project.stories);
    assert_eq!(restored.assets, old_assets);
    assert_eq!(
        &restored.stories["main"]["nodes"].as_array().unwrap()[1..],
        old_story["nodes"].as_array().unwrap()
    );
    let entries = package::read_package(&exported).unwrap();
    let lua = std::str::from_utf8(&entries["lua/main.lua"]).unwrap();
    assert!(
        lua.find("mod_player_appearance(\"beautified\")").unwrap()
            < lua
                .find("runwait(characters.LoadCharacterAsset(\"player\"))")
                .unwrap()
    );
    assert_substituted_lua_is_rejected(&exported, &directory.path().join("new-substituted.lommod"));
    assert_eq!(std::fs::read(&fixture).unwrap(), original_bytes);
}
