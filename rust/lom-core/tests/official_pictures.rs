use serde_json::{json, Value};

fn story(color: Option<Value>) -> Value {
    let mut show =
        json!({"id":"show","type":"cg","kind":"big","action":"show","key":"big_fight_001"});
    if let Some(color) = color {
        show["color"] = color;
    }
    json!({"story_schema":2,"id":"main","title":"大图","start":"show","nodes":[
        show,{"id":"hide","type":"cg","kind":"big","action":"hide"},{"id":"end","type":"end"}]})
}

#[test]
fn big_picture_passes_the_required_native_background_color_argument() {
    for color in [None, Some(0), Some(1), Some(2)] {
        let lua =
            lom_core::codegen::compile_story(&story(color.map(|c| json!(c))), None, None, None)
                .unwrap();
        assert!(lua.contains(&format!(
            "mainui.ShowBigPicture(\"big_fight_001\", {})",
            color.unwrap_or(0)
        )));
    }
    for color in [json!(-1), json!(3), json!(1.5), json!("black")] {
        assert!(lom_core::validate::validate_story(&story(Some(color))).is_err());
    }
}
