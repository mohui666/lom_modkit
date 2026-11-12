use lom_core::{project::Project, project_merge};
use serde_json::json;

#[test]
fn independent_projects_get_reachable_entries_and_rewritten_assets_without_editing_text() {
    let mut first = Project::new();
    let mut second = Project::new();
    first
        .assets
        .insert("assets/picture.png".into(), b"first".to_vec());
    second
        .assets
        .insert("assets/picture.png".into(), b"second".to_vec());
    second.stories.insert("main".into(), json!({"id":"main","story_schema":2,"start":"bg","nodes":[
        {"id":"bg","type":"intro","intro_source":"custom","name":"测试人物","image":"assets/picture.png","text":"测试插图"},
        {"id":"flag","type":"flag","flag":"private_record"},
        {"id":"text","type":"say","mode":"narrative","text":"assets/picture.png main private_record"},
        {"id":"go","type":"end","next_script":"next"}]}));
    second.stories.insert("next".into(), json!({"id":"next","story_schema":2,"start":"check","nodes":[
        {"id":"check","type":"flag_check","source":"mod","flag":"private_record","success":"done","failure":"done"},
        {"id":"official","type":"flag_check","source":"flag_value","flag":"must_remain_official","value":1,"op":"==","success":"done","failure":"done"},
        {"id":"done","type":"end"}]}));
    let original = second.clone();
    let report = project_merge::merge(&mut first, &second).unwrap();
    assert_eq!(report.chapters, 2);
    assert_eq!(first.assets["assets/picture.png"], b"first");
    assert_eq!(first.assets["assets/merge1/picture.png"], b"second");
    assert_eq!(
        first.stories["merge1_main"]["nodes"][0]["image"],
        "assets/merge1/picture.png"
    );
    assert_eq!(
        first.stories["merge1_main"]["nodes"][2]["text"],
        "assets/picture.png main private_record"
    );
    assert_eq!(
        first.stories["merge1_main"]["nodes"][3]["next_script"],
        "merge1_next"
    );
    assert_eq!(
        first.stories["merge1_next"]["nodes"][0]["flag"],
        "merge1_private_record"
    );
    assert_eq!(
        first.stories["merge1_next"]["nodes"][1]["flag"],
        "must_remain_official"
    );
    assert_eq!(
        first.stories["merge1_entries"]["nodes"][2]["next_script"],
        "merge1_main"
    );
    assert_eq!(second, original);
    assert!(first.source.is_none());
    let root = tempfile::tempdir().unwrap();
    first.save_to(&root.path().join("combined")).unwrap();
    first.export(&root.path().join("combined.lommod")).unwrap();
    assert_eq!(
        Project::open(&root.path().join("combined.lommod"))
            .unwrap()
            .stories,
        first.stories
    );
}

#[test]
fn addon_event_conditions_and_results_stay_with_their_original_module() {
    let mut first = Project::new_scenario("addon");
    let mut second = Project::new_scenario("addon");
    second.scenario.as_mut().unwrap()["tasks"] =
        json!([{"id":"quest","name":"任务","states":["locked","done"],"initial":"locked"}]);
    let event = &mut second.scenario.as_mut().unwrap()["events"][0];
    event["when"] = json!({"all":[
        {"source":"task","key":"quest","op":"eq","value":"locked"},
        {"source":"flag","key":"official_record","op":"eq","value":1},
        {"source":"game_flag","key":"official_numeric","op":"eq","value":2},
        {"source":"variable","key":"score","op":"eq","value":0}]});
    event["outcomes"]["completed"] =
        json!({"tasks":{"quest":"done"},"variables":{"score":{"op":"add","value":1}}});
    project_merge::merge(&mut first, &second).unwrap();
    let config = first.scenario.as_ref().unwrap();
    let event = &config["events"][1];
    assert_eq!(event["script"], "merge1_main");
    assert_eq!(event["when"]["all"][0]["key"], "merge1_quest");
    assert_eq!(event["when"]["all"][1]["key"], "official_record");
    assert_eq!(event["when"]["all"][2]["key"], "official_numeric");
    assert_eq!(event["when"]["all"][3]["key"], "merge1_score");
    assert_eq!(
        event["outcomes"]["completed"]["tasks"]["merge1_quest"],
        "done"
    );
    assert_eq!(config["tasks"][0]["id"], "merge1_quest");
    let dir = tempfile::tempdir().unwrap();
    first.export(&dir.path().join("combined.lommod")).unwrap();
}

#[test]
fn existing_private_names_reserve_their_namespace_even_without_a_matching_chapter() {
    let mut first = Project::new_scenario("addon");
    first.scenario.as_mut().unwrap()["events"][0]["outcomes"]["completed"]["variables"] =
        json!({"merge1_score":{"op":"set","value":2}});
    let mut second = Project::new_scenario("addon");
    second.scenario.as_mut().unwrap()["events"][0]["outcomes"]["completed"]["variables"] =
        json!({"score":{"op":"set","value":9}});
    project_merge::merge(&mut first, &second).unwrap();
    let events = &first.scenario.as_ref().unwrap()["events"];
    assert_eq!(
        events[0]["outcomes"]["completed"]["variables"]["merge1_score"]["value"],
        2
    );
    assert_eq!(
        events[1]["outcomes"]["completed"]["variables"]["merge2_score"]["value"],
        9
    );
    assert_eq!(events[1]["script"], "merge2_main");
}

#[test]
fn sandbox_merge_preserves_selected_openings_results_with_one_start_event() {
    let mut first = Project::new_scenario("sandbox");
    let mut second = Project::new_scenario("sandbox");
    second.scenario.as_mut().unwrap()["events"][0]["outcomes"]["completed"] =
        json!({"variables":{"score":{"op":"set","value":9}}});
    project_merge::merge(&mut first, &second).unwrap();
    let config = first.scenario.as_ref().unwrap();
    assert_eq!(
        config["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["trigger"] == "start")
            .count(),
        1
    );
    assert_eq!(
        first.stories["merge1_main"]["nodes"][1]["event_result"],
        "merge1_completed"
    );
    assert_eq!(
        config["events"][0]["outcomes"]["merge1_completed"]["variables"]["merge1_score"]["value"],
        9
    );
    let dir = tempfile::tempdir().unwrap();
    first.export(&dir.path().join("combined.lommod")).unwrap();
}

#[test]
fn an_existing_entries_chapter_does_not_collide_with_the_generated_work_menu() {
    let mut first = Project::new_scenario("sandbox");
    let mut second = Project::new_scenario("sandbox");
    let mut entry = second.stories.remove("main").unwrap();
    entry["id"] = json!("entries");
    second.stories.insert("entries".into(), entry);
    second.manifest["entry"] = json!("entries");
    second.scenario.as_mut().unwrap()["events"][0]["script"] = json!("entries");
    project_merge::merge(&mut first, &second).unwrap();
    assert_ne!(first.manifest["entry"], "merge1_entries");
    let menu = &first.stories[first.manifest["entry"].as_str().unwrap()];
    assert_eq!(menu["nodes"][2]["next_script"], "merge1_entries");
}

#[test]
fn failed_merge_preserves_current_project_and_reports_the_incompatible_input() {
    let mut first = Project::new();
    let before = first.clone();
    assert!(
        project_merge::merge(&mut first, &Project::new_scenario("addon"))
            .unwrap_err()
            .to_string()
            .contains("玩法模式")
    );
    assert_eq!(first, before);
    let mut second = Project::new();
    second.stories.get_mut("main").unwrap()["nodes"][0] =
        json!({"id":"say1","type":"raw","code":"return"});
    assert!(project_merge::merge(&mut first, &second)
        .unwrap_err()
        .to_string()
        .contains("原始 Lua"));
    assert_eq!(first, before);
}

#[test]
fn long_private_names_cannot_merge_two_different_variables_into_one() {
    let mut first = Project::new_scenario("addon");
    let mut second = Project::new_scenario("addon");
    let left = format!("{}a", "v".repeat(58));
    let right = format!("{}b", "v".repeat(58));
    second.scenario.as_mut().unwrap()["events"][0]["outcomes"]["completed"]["variables"] = json!({
        &left:{"op":"set","value":1}, &right:{"op":"set","value":2}
    });
    let before = first.clone();
    match project_merge::merge(&mut first, &second) {
        Ok(_) => assert_eq!(
            first.scenario.as_ref().unwrap()["events"][1]["outcomes"]["completed"]["variables"]
                .as_object()
                .unwrap()
                .len(),
            2
        ),
        Err(error) => {
            assert!(format!("{error:#}").contains("变量"), "{error:#}");
            assert_eq!(first, before);
        }
    }
}

#[test]
fn conflicting_user_content_keeps_one_dot_ids_and_retargets_martial_talent_dependencies() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-1.5.10-martial-base.lommod");
    let mut first = Project::open(&path).unwrap();
    let talent_key = "assets/user/talent/fixture.awakening/content.json";
    first.assets.insert(talent_key.into(), lom_core::stable_json(&json!({"schema":1,"id":"fixture.awakening","type":"talent","name":"测试天赋","files":{"main":""}})).unwrap());
    let key = "assets/user/martial/fixture.manual/content.json";
    let mut meta: serde_json::Value = serde_json::from_slice(&first.assets[key]).unwrap();
    meta["levels"][0]["talents"] = json!([{"talent":"user:fixture.awakening","level":1}]);
    first
        .assets
        .insert(key.into(), lom_core::stable_json(&meta).unwrap());
    let second = first.clone();
    project_merge::merge(&mut first, &second).unwrap();
    let key = "assets/user/martial/merge1_fixture.manual/content.json";
    let meta: serde_json::Value = serde_json::from_slice(&first.assets[key]).unwrap();
    assert_eq!(meta["id"], "merge1_fixture.manual");
    assert_eq!(
        meta["levels"][0]["talents"][0]["talent"],
        "user:merge1_fixture.awakening"
    );
    let dir = tempfile::tempdir().unwrap();
    first.export(&dir.path().join("merged.lommod")).unwrap();
    let loaded = Project::open(&dir.path().join("merged.lommod")).unwrap();
    assert!(loaded
        .assets
        .contains_key("assets/user/talent/merge1_fixture.awakening/content.json"));
}
