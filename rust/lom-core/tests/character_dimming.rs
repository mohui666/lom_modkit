use serde_json::json;

#[test]
fn explicit_dimming_uses_the_persistent_bridge_before_and_after_speech() {
    for actor in ["player", "user:demo.speaker"] {
        for parallel in [false, true] {
            let dark = json!({"id":"dark","type":"dim","character":actor,"dimmed":true});
            let dark = if parallel {
                json!({"id":"dark","type":"parallel","actions":[
                    {"type":"dim","character":actor,"dimmed":true},
                    {"type":"wait","seconds":0.2}]})
            } else {
                dark
            };
            let story = json!({"story_schema":2,"id":"main","title":"压暗持续","start":"enter","nodes":[
                {"id":"enter","type":"show","character":actor,"position":"M"}, dark,
                {"id":"speech","type":"say","character":actor,"text":"说话后仍保持压暗"},
                {"id":"bright","type":"dim","character":actor,"dimmed":false},
                {"id":"end","type":"end"}]});
            let lua = lom_core::codegen::compile_story(&story, None, None, None).unwrap();
            let before = lua
                .find(&format!("mod_char_dim(\"{actor}\", true)"))
                .unwrap();
            let speech = lua.find("GetStoryText").unwrap();
            let after = lua
                .find(&format!("mod_char_dim(\"{actor}\", false)"))
                .unwrap();
            assert!(before < speech && speech < after);
            assert!(!lua.contains("stage.SetDimmed"));
        }
    }
}
