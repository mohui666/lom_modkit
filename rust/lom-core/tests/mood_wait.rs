use lom_core::{codegen, validate};
use serde_json::{json, Value};

fn story(actions: &[Value]) -> Value {
    let mut nodes = vec![json!({"id":"enter","type":"show","character":"player","position":"M"})];
    for (index, action) in actions.iter().enumerate() {
        let mut action = action.clone();
        action["id"] = json!(format!("mood{index}"));
        nodes.push(action);
    }
    nodes.push(json!({"id":"done","type":"end"}));
    json!({"id":"main","start":"enter","nodes":nodes})
}

#[test]
fn mood_show_switch_and_hide_wait_before_advancing_to_the_next_step() {
    let actions = [
        json!({"type":"mood","enabled":true,"wait_after":0.25}),
        json!({"type":"mood","character":"player","enabled":true,"icon":"shock","wait_after":0.5}),
        json!({"type":"mood","character":"player","enabled":true,"icon":"laugh1","wait_after":1.25}),
        json!({"type":"mood","character":"player","enabled":false,"wait_after":0.75}),
        json!({"type":"mood","enabled":false,"wait_after":1.5}),
    ];
    let lua = codegen::compile_story(&story(&actions), None, None, None).unwrap();
    let calls = [
        "mod_set_mood(true)\n\tmod_refresh_mood()",
        "mod_character_mood(\"player\", \"shock\")",
        "mod_character_mood(\"player\", \"laugh1\")",
        "mod_character_mood(\"player\", \"hide\")",
        "mod_set_mood(false)\n\tmod_hide_mood()",
    ];
    for (index, (action, call)) in actions.iter().zip(calls).enumerate() {
        let next = if index + 1 == actions.len() {
            "done".into()
        } else {
            format!("mood{}", index + 1)
        };
        let expected = format!(
            "{call}\n\twait({})\n\treturn node_{next}()",
            codegen::lua_num(&action["wait_after"]).unwrap()
        );
        assert!(lua.contains(&expected), "step {index}: {lua}");
    }
    assert_eq!(lua.matches("\twait(").count(), actions.len());
}

#[test]
fn omitted_or_zero_mood_wait_preserves_the_existing_lua() {
    for action in [
        json!({"type":"mood","enabled":true}),
        json!({"type":"mood","enabled":false}),
        json!({"type":"mood","character":"player","enabled":true,"icon":"shock"}),
        json!({"type":"mood","character":"player","enabled":false}),
    ] {
        let previous = codegen::compile_story(&story(&[action.clone()]), None, None, None).unwrap();
        assert!(!previous.contains("\twait("));
        for zero in [json!(0), json!(0.0)] {
            let mut explicit = action.clone();
            explicit["wait_after"] = zero;
            assert_eq!(
                previous,
                codegen::compile_story(&story(&[explicit]), None, None, None).unwrap()
            );
        }
    }
}

#[test]
fn mood_wait_requires_a_nonnegative_finite_number() {
    for enabled in [true, false] {
        for character in [None, Some("player")] {
            let mut action = json!({"type":"mood","enabled":enabled});
            if let Some(character) = character {
                action["character"] = json!(character);
            }
            for value in [json!(0), json!(0.125), json!(25)] {
                action["wait_after"] = value;
                validate::validate_story(&story(&[action.clone()])).unwrap();
            }
            for value in [
                json!(-0.125),
                json!("1"),
                json!(true),
                json!(null),
                serde_json::from_str::<Value>("1e999").unwrap(),
            ] {
                action["wait_after"] = value;
                assert!(
                    validate::validate_story(&story(&[action.clone()])).is_err(),
                    "{action}"
                );
            }
        }
    }
}
