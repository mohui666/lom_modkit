use super::*;

fn positions() -> Value {
    serde_json::from_str(include_str!("../../../../data/stage_positions.json")).unwrap()
}

#[test]
fn playtest_prelude_restores_native_container_before_the_logical_position() {
    let story = json!({"id":"main","start":"enter","nodes":[
        {"id":"enter","type":"show","character":"player","position":"M"},
        {"id":"move","type":"move","character":"player","from":"M","to":"BC2","duration":1},
        {"id":"target","type":"wait","seconds":1}
    ]});
    let source = story.clone();
    let prelude = build_playtest_prelude(&story, "target");
    assert!(prelude.iter().any(|node| node["type"] == "move"
        && node["from"] == "M"
        && node["to"] == "BC2"
        && node["duration"] == 0));
    let mut replay = story.clone();
    let mut nodes = prelude.clone();
    nodes.push(story["nodes"][2].clone());
    replay["start"] = prelude[0]["id"].clone();
    replay["nodes"] = nodes.into();
    let restored = simulate(&replay, "target");
    assert_eq!(restored["actors"]["player"]["position"], "BC2");
    assert_eq!(actor_slot(&restored["actors"]["player"]), "M");
    assert_eq!(story, source);
}

#[test]
fn native_stage_show_move_retains_the_from_container_after_reaching_the_to_slot() {
    let story = json!({"start":"enter","nodes":[
        {"id":"enter","type":"show","character":"player","position":"BC1"},
        {"id":"move","type":"move","character":"player","from":"M","to":"BC2","duration":1},
        {"id":"hold","type":"wait","seconds":1},
        {"id":"reenter","type":"show","character":"player","position":"BC2"}
    ]});
    let source = story.clone();
    let positions = positions();
    for elapsed in [0.0, 0.5, 1.0, 2.0] {
        let state = simulate_frame(&story, "move", elapsed, &positions);
        let actor = &state["actors"]["player"];
        assert_eq!(actor["position"], "BC2");
        assert_eq!(actor_slot(actor), "M");
        assert_eq!(
            slot_size(&positions, actor_slot(actor), Vec2::new(1920.0, 1080.0)),
            Vec2::new(1920.0, 1080.0)
        );
    }
    assert_eq!(
        actor_slot(&simulate(&story, "hold")["actors"]["player"]),
        "M"
    );
    assert_eq!(
        actor_slot(&simulate(&story, "reenter")["actors"]["player"]),
        "BC2"
    );
    assert_eq!(story, source);
}

#[test]
fn custom_move_keeps_the_current_holder_until_the_destination_fit_finishes() {
    let story = json!({"start":"enter","nodes":[
        {"id":"enter","type":"show","character":"user:actor","position":"BC1"},
        {"id":"move","type":"move","character":"user:actor","from":"M","to":"Talk","duration":2},
        {"id":"hold","type":"wait","seconds":1}
    ]});
    let positions = positions();
    for elapsed in [0.0, 0.5, 1.99] {
        let state = simulate_frame(&story, "move", elapsed, &positions);
        assert_eq!(actor_slot(&state["actors"]["user:actor"]), "BC1");
    }
    assert_eq!(
        actor_slot(&simulate_frame(&story, "move", 2.0, &positions)["actors"]["user:actor"]),
        "Talk"
    );
    assert_eq!(
        actor_slot(&simulate(&story, "hold")["actors"]["user:actor"]),
        "Talk"
    );
}

#[test]
fn zero_duration_and_legacy_actor_states_have_no_intermediate_container() {
    let story = json!({"start":"enter","nodes":[
        {"id":"enter","type":"show","character":"user:actor","position":"M"},
        {"id":"move","type":"move","character":"user:actor","from":"M","to":"BC2","duration":0}
    ]});
    assert_eq!(
        actor_slot(&simulate_frame(&story, "move", 0.0, &positions())["actors"]["user:actor"]),
        "BC2"
    );
    assert_eq!(actor_slot(&json!({"position":"BCB2"})), "BCB2");
    assert_eq!(actor_slot(&json!({"position":"L1"})), "L1");
}

#[test]
fn native_preserving_parallel_move_keeps_the_existing_holder_instead_of_refitting_from() {
    let story = json!({"start":"enter","nodes":[
        {"id":"enter","type":"show","character":"player","position":"BC2"},
        {"id":"move","type":"parallel","actions":[
            {"type":"move","character":"player","from":"M","to":"Talk","duration":2,"start_delay":0.5},
            {"type":"face","character":"player","facing":"left"}]},
        {"id":"hold","type":"wait","seconds":1}
    ]});
    let positions = positions();
    for elapsed in [0.0, 0.6, 1.5, 2.5] {
        assert_eq!(
            actor_slot(&simulate_frame(&story, "move", elapsed, &positions)["actors"]["player"]),
            "BC2"
        );
    }
    assert_eq!(
        actor_slot(&simulate(&story, "hold")["actors"]["player"]),
        "BC2"
    );
}

#[test]
fn a_later_native_face_or_hide_copies_the_logical_slot_again() {
    for node in [
        json!({"id":"next","type":"face","character":"player","facing":"left"}),
        json!({"id":"next","type":"hide","character":"player","fadeDuration":1}),
    ] {
        let story = json!({"start":"enter","nodes":[
            {"id":"enter","type":"show","character":"player","position":"M"},
            {"id":"move","type":"move","character":"player","from":"M","to":"BC2","duration":1},
            node]});
        let state = simulate_frame(&story, "next", 0.5, &positions());
        assert_eq!(actor_slot(&state["actors"]["player"]), "BC2");
    }
}
