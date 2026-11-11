//! Explicit edits to an event project's mode and opening binding.
use crate::{project::Project, scenario, validate};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

fn documents(project: &Project, mode: &str) -> Result<(Value, Value)> {
    ensure!(
        project.manifest["play_mode"] == mode,
        "作品当前玩法不是 {mode}"
    );
    let config = project.scenario.clone().context("scenario.json 不存在")?;
    ensure!(
        config.is_object() && config["events"].is_array(),
        "scenario.json: events 必须为数组"
    );
    Ok((project.manifest.clone(), config))
}

fn check_story(project: &Project, story: &str) -> Result<()> {
    ensure!(
        project.stories.contains_key(story),
        "scenario.json: 开场章节不存在：{story}"
    );
    Ok(())
}

/// Editing one binding must remain possible while another field needs repair.
pub fn set_entry(project: &mut Project, event: &str, story: &str) -> Result<()> {
    check_story(project, story)?;
    let (mut manifest, mut config) = documents(project, "sandbox")?;
    let entry = config["events"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["id"] == event)
        .context("scenario.json: 开场事件不存在")?;
    ensure!(
        entry["trigger"] == "start",
        "scenario.json: 请选择 trigger=start 的开场事件"
    );
    entry["script"] = json!(story);
    config["entry_event"] = json!(event);
    manifest["entry"] = json!(story);
    project.manifest = manifest;
    project.scenario = Some(config);
    Ok(())
}

fn append_opening(config: &mut Value, story: &str) -> Result<String> {
    let events = config["events"]
        .as_array_mut()
        .context("scenario.json: events 必须为数组")?;
    ensure!(events.len() < 512, "scenario.json: 最多 512 条事件");
    ensure!(
        !events.iter().any(|event| event["trigger"] == "start"),
        "scenario.json: 已有开场事件，请选择并关联该事件"
    );
    let id = (1..)
        .map(|number| {
            if number == 1 {
                "opening".to_owned()
            } else {
                format!("opening{number}")
            }
        })
        .find(|id| !events.iter().any(|event| event["id"] == *id))
        .unwrap();
    let mut entry = scenario::default_config()["events"][0].clone();
    entry["id"] = json!(id);
    entry["script"] = json!(story);
    events.push(entry);
    config["entry_event"] = json!(id);
    Ok(id)
}

pub fn create_entry(project: &mut Project, story: &str) -> Result<String> {
    check_story(project, story)?;
    let (mut manifest, mut config) = documents(project, "sandbox")?;
    let id = append_opening(&mut config, story)?;
    manifest["entry"] = json!(story);
    project.manifest = manifest;
    project.scenario = Some(config);
    Ok(id)
}

/// Keep every former opening's identity, script, conditions and outcomes. The
/// author chooses its new trigger explicitly before applying this conversion.
pub fn to_addon(project: &mut Project, trigger: &str, position: Option<&str>) -> Result<()> {
    let (mut manifest, mut config) = documents(project, "sandbox")?;
    ensure!(
        manifest["campaign"].is_object(),
        "manifest.json: campaign 必须是对象"
    );
    let events = config["events"].as_array_mut().unwrap();
    if events.iter().any(|event| event["trigger"] == "start") {
        ensure!(
            ["auto", "position"].contains(&trigger),
            "scenario.json: 请选择原开场事件转换后的触发方式"
        );
        if trigger == "position" {
            ensure!(
                position.is_some_and(|position| validate::node_schema()["campaign_positions"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(position))),
                "scenario.json: 请选择有效的触发地点"
            );
        }
    }
    for event in events
        .iter_mut()
        .filter(|event| event["trigger"] == "start")
    {
        event["trigger"] = json!(trigger);
        if trigger == "position" {
            event["position"] = json!(position.unwrap());
        } else {
            event.as_object_mut().unwrap().remove("position");
        }
    }
    config.as_object_mut().unwrap().remove("entry_event");
    manifest["play_mode"] = json!("addon");
    manifest["campaign"]["new_game"] = json!(false);
    manifest["campaign"]["disable_official_events"] = json!(false);
    validate::validate_manifest(&manifest)?;
    scenario::validate(&config, &manifest, &project.stories)?;
    project.manifest = manifest;
    project.scenario = Some(config);
    Ok(())
}

/// A dedicated opening leaves all ordinary event rules and outcomes intact.
pub fn to_sandbox(project: &mut Project, story: &str) -> Result<()> {
    check_story(project, story)?;
    let (mut manifest, mut config) = documents(project, "addon")?;
    ensure!(
        manifest["campaign"].is_object(),
        "manifest.json: campaign 必须是对象"
    );
    append_opening(&mut config, story)?;
    manifest["play_mode"] = json!("sandbox");
    manifest["entry"] = json!(story);
    manifest["campaign"]["new_game"] = json!(true);
    validate::validate_manifest(&manifest)?;
    scenario::validate(&config, &manifest, &project.stories)?;
    project.manifest = manifest;
    project.scenario = Some(config);
    Ok(())
}
