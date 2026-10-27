use serde_json::{json, Value};

fn story(size: Option<Value>, mode: &str) -> Value {
    let mut node = json!({"id":"line","type":"say","character":"player","text":"台词","mode":mode});
    if let Some(size) = size {
        node["font_size"] = size;
    }
    json!({"story_schema":2,"id":"main","title":"字号","start":"line","nodes":[node,{"id":"end","type":"end"}]})
}

#[test]
fn numeric_sizes_work_in_every_dialogue_mode_and_defaults_keep_legacy_lua() {
    for mode in ["character", "think", "narrative", "center"] {
        let plain = lom_core::codegen::compile_story(&story(None, mode), None, None, None).unwrap();
        for size in [json!(0), json!("normal")] {
            assert_eq!(
                plain,
                lom_core::codegen::compile_story(&story(Some(size), mode), None, None, None)
                    .unwrap()
            );
        }
        for size in [1, 24, 36, 77, 200] {
            let lua =
                lom_core::codegen::compile_story(&story(Some(json!(size)), mode), None, None, None)
                    .unwrap();
            assert!(lua.contains(&format!("say(\"{{size={size}}}\" .. luamanager.GetStoryText(\"MOD_MOD_main_line\") .. \"{{/size}}\")")));
            assert!(!lua.contains("台词"));
        }
    }
}

#[test]
fn invalid_sizes_are_rejected_and_existing_named_sizes_remain_readable() {
    for size in [
        json!(-1),
        json!(201),
        json!(36.5),
        json!(true),
        json!(null),
        json!("36"),
        json!([]),
    ] {
        assert!(lom_core::validate::validate_story(&story(Some(size), "narrative")).is_err());
    }
    for (size, expected) in [("small", 24), ("large", 60)] {
        let lua = lom_core::codegen::compile_story(
            &story(Some(json!(size)), "narrative"),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(lua.contains(&format!("{{size={expected}}}")));
    }
}
