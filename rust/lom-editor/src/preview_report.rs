use anyhow::Result;
use lom_core::project::Project;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub(crate) fn assessment(project: &Project, options: &Value, base: &Path) -> Result<Value> {
    let library = crate::asset_query::library_status(options, base).unwrap_or_else(
        |error| json!({"state":"error","freshness":"unknown","error":format!("{error:#}")}),
    );
    let mut game_only: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (script, story) in &project.stories {
        for node in story["nodes"].as_array().into_iter().flatten() {
            let kind = node["type"].as_str().unwrap_or("");
            if matches!(
                kind,
                "raw" | "combat" | "battle" | "dice" | "panel" | "native_branch" | "use_item"
            ) {
                game_only
                    .entry(kind.into())
                    .or_default()
                    .push(json!({"script":script,"node":node["id"]}));
            }
        }
    }
    let story_entries: Vec<_> = project.scenario.as_ref().and_then(|config| config["events"].as_array()).into_iter().flatten()
        .filter(|event| event["trigger"] == "story_entry")
        .map(|event| json!({"event_id":event["id"],"script":event["script"],"target":event["target"],"from":event["from"]})).collect();
    let mut limitations =
        vec!["编辑器舞台与部分滤镜为布局预览，不等同于游戏原生画面；渲染完成也不代表已人工看图。"];
    if !story_entries.is_empty() {
        limitations.push("单章预览不执行原版前置章节，也不模拟其遗留界面；请用 playtest-event 的 predecessor 模式检查原版→MOD→原版的衔接。");
    }
    Ok(json!({
        "kind":"editor_simulation",
        "simulation_scope":"stage_layout_state_only","route_execution_tested":false,
        "simulation_performed":true,
        "rendering_performed":false,"rendered_images":0,
        "visual_review_performed":false,"game_tested":false,
        "library":library,
        "missing_preview_capabilities":game_only.into_iter().map(|(kind,locations)|
            json!({"kind":kind,"locations":locations,"reason":"requires_game_execution"})).collect::<Vec<_>>(),
        "original_context":{"executed":false,"inherited_ui_simulated":false,"story_entry_events":story_entries},
        "limitations":limitations
    }))
}
