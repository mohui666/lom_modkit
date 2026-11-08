use lom_core::{codegen, validate};
use serde_json::{json, Value};

#[test]
fn official_shock_finishes_before_following_actions_and_parallel_join() {
    let action = json!({"type":"shock","character":"player","duration":0.8,"wait_after":0.2});
    for node in [
        action.clone(),
        json!({"type":"parallel","actions":[action,{"type":"wait","seconds":0.3}]}),
    ] {
        let lua = codegen::compile_story(&story(node), None, None, None).unwrap();
        let shock = lua.find("runblock(flowcharts.common, \"shock\")").unwrap();
        let following = &lua[shock..];
        assert!(following.find("wait(0.8)").unwrap() < following.find("wait(0.2)").unwrap());
    }
}

fn story(action: Value) -> Value {
    let mut action = action;
    action["id"] = json!("action");
    json!({"id":"main","start":"action","nodes":[action,{"id":"done","type":"end"}]})
}

#[test]
fn legacy_launch_and_new_completion_waits_remain_distinct_and_groups_join_once() {
    let mut show = json!({"type":"show","character":"player","position":"M","fadeDuration":0.3,"moveDuration":0.7});
    let legacy = codegen::compile_story(&story(show.clone()), None, None, None).unwrap();
    assert!(!legacy.contains("wait(math.max"));
    show["wait_for_completion"] = json!(true);
    show["wait_after"] = json!(1.25);
    let lua = codegen::compile_story(&story(show.clone()), None, None, None).unwrap();
    assert!(lua.contains("wait(math.max(0.3, 0.7, stage.FadeDuration))\n\twait(1.25)"));
    let joined = story(json!({"type":"parallel","actions":[show,{"type":"wait","seconds":0.5}]}));
    let lua = codegen::compile_story(&joined, None, None, None).unwrap();
    assert_eq!(
        lua.matches("wait(math.max(0.3, 0.7, stage.FadeDuration))")
            .count(),
        1
    );
    assert_eq!(lua.matches("wait(1.25)").count(), 1);
    let moved = story(
        json!({"type":"move","character":"player","from":"L1","to":"M","duration":1,"wait_after":0.5}),
    );
    let lua = codegen::compile_story(&moved, None, None, None).unwrap();
    assert!(lua.contains("wait(1)\n\twait(0.5)"));
}

#[test]
fn rejects_invalid_waits_in_standalone_and_joined_actions() {
    for value in [json!(-1), json!("1"), json!(true), json!(null)] {
        let action = json!({"type":"hide","character":"player","wait_after":value});
        assert!(validate::validate_story(&story(action.clone())).is_err());
        assert!(validate::validate_story(&story(
            json!({"type":"parallel","actions":[action,{"type":"wait","seconds":0}]})
        ))
        .is_err());
    }
    assert!(validate::validate_story(&story(
        json!({"type":"show","character":"player","position":"M","wait_for_completion":1})
    ))
    .is_err());
}
