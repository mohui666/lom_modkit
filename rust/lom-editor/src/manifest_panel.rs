use crate::{
    forms::{self, Catalog},
    i18n::tr,
};
use eframe::egui;
use serde_json::{json, Value};

pub fn show(ui: &mut egui::Ui, manifest: &mut Value, catalog: &Catalog, stories: &[String]) {
    ui.heading(tr("作品设置"));
    let event_mode=manifest.get("play_mode").is_some();
    if event_mode {
        ui.label(if manifest["play_mode"]=="addon" { tr("作品模式：原版附加剧情") } else { tr("作品模式：自由剧本") });
        ui.small(tr("此设置适用于整部作品。事件的时间、地点与任务进度在左侧“剧情安排”中设置。"));
    }
    for (key, label, kind) in [
        ("id", "作品 ID", "str"),
        ("campaign_id", "存档身份", "str"),
        ("name", "名称", "str"),
        ("version", "版本", "str"),
        ("author", "作者", "str"),
        ("description", "简介", "text"),
        ("entry", "入口章节", "story_ref"),
    ] {
        if event_mode && matches!(key,"id"|"campaign_id"|"entry") {continue;}
        catalog.field(ui, manifest, key, label, kind, false, &[], stories);
    }
    if event_mode {
        ui.collapsing(tr("高级：作品标识"),|ui| {
            for (key,label) in [("id","作品 ID"),("campaign_id","存档身份")] {
                catalog.field(ui,manifest,key,label,"str",false,&[],stories);
            }
            ui.small(tr("自由剧本的入口由开场事件管理。"));
        });
    }
    ui.collapsing(tr("兼容性"), |ui| {
        for key in [
            "min_host_version",
            "tested_host_version",
            "game_version",
            "tested_game_version",
        ] {
            catalog.field(ui, manifest, key, key, "str", true, &[], stories);
        }
    });
    ui.separator();
    if event_mode {
        if manifest["play_mode"]=="sandbox" {
            catalog.field(ui,&mut manifest["campaign"],"disable_official_events","禁用官方事件","bool",false,&[],stories);
        }
        return;
    }
    ui.heading(tr("自由模式触发规则"));
    if !manifest["campaign"].is_object() {
        if ui.button(tr("启用战役设置")).clicked() {
            manifest["campaign"] = json!({"new_game":true,"triggers":[]});
        }
    } else {
        let campaign = &mut manifest["campaign"];
        catalog.field(
            ui,
            campaign,
            "new_game",
            "新游戏入口",
            "bool",
            false,
            &[],
            stories,
        );
        catalog.field(
            ui,
            campaign,
            "disable_official_events",
            "禁用官方事件",
            "bool",
            true,
            &[],
            stories,
        );
        if ui.button(tr("添加自由模式触发规则")).clicked() {
            if !campaign["triggers"].is_array() {
                campaign["triggers"] = json!([]);
            }
            campaign["triggers"].as_array_mut().unwrap().push(json!({"type":"position","position":"Center","script":stories.first().cloned().unwrap_or_default()}));
        }
        let mut remove = None;
        for (i, t) in campaign
            .get_mut("triggers")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .enumerate()
        {
            ui.push_id(i, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("#{}", i + 1));
                        if ui.button(tr("删除规则")).clicked() {
                            remove = Some(i);
                        }
                    });
                    if t["type"] != "position" {
                        forms::value_editor(ui, "触发规则", t, 0);
                        return;
                    }
                    for (key, label, kind, optional) in [
                        ("position", "位置", "free_position", false),
                        ("script", "章节", "story_ref", false),
                        ("when_flag_set", "要求旗标已设定", "flag_ref", true),
                        ("when_flag_clear", "要求旗标未设定", "flag_ref", true),
                    ] {
                        catalog.field(ui, t, key, label, kind, optional, &[], stories);
                    }
                    bounded(ui, t, "when_month", "限定月份", 12);
                    bounded(ui, t, "when_stage", "限定旬", 3);
                    let mut enabled = t.get("when_affinity").is_some();
                    if ui.checkbox(&mut enabled, tr("好感度条件")).changed() {
                        if enabled {
                            t["when_affinity"] = json!({"character":"","min":0});
                        } else {
                            t.as_object_mut().unwrap().remove("when_affinity");
                        }
                    }
                    if enabled {
                        catalog.field(
                            ui,
                            &mut t["when_affinity"],
                            "character",
                            "好感度人物",
                            "affinity_character",
                            false,
                            &[],
                            stories,
                        );
                        catalog.field(
                            ui,
                            &mut t["when_affinity"],
                            "min",
                            "好感度下限",
                            "int",
                            false,
                            &[],
                            stories,
                        );
                    }
                });
            });
        }
        if let Some(i) = remove {
            campaign["triggers"].as_array_mut().unwrap().remove(i);
        }
    }
    ui.collapsing(tr("完整元数据（保留扩展字段）"), |ui| {
        forms::value_editor(ui, "manifest", manifest, 0);
    });
}
fn bounded(ui: &mut egui::Ui, value: &mut Value, key: &str, label: &str, max: i64) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        let mut selection = value.get(key).cloned();
        let before = selection.clone();
        egui::ComboBox::from_id_salt(key)
            .selected_text(
                selection
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| tr("不限")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selection, None, tr("不限"));
                for n in 1..=max {
                    ui.selectable_value(&mut selection, Some(json!(n)), n.to_string());
                }
            });
        if before != selection {
            match selection {
                Some(v) => value[key] = v,
                None => {
                    value.as_object_mut().unwrap().remove(key);
                }
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn painting_does_not_insert_or_replace_campaign_triggers() {
        let ctx = egui::Context::default();
        let catalog = Catalog::new();
        for triggers in [
            None,
            Some(json!([])),
            Some(Value::Null),
            Some(json!("future")),
        ] {
            let mut manifest = lom_core::project::Project::new().manifest;
            manifest["campaign"] = json!({"new_game":true,"extension":"keep"});
            if let Some(value) = triggers {
                manifest["campaign"]["triggers"] = value;
            }
            let before = manifest.clone();
            let valid_before = lom_core::validate::validate_manifest(&manifest).is_ok();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, &mut manifest, &catalog, &["main".into()]);
                });
            });
            assert_eq!(manifest, before, "Read-only painting changed the manifest");
            assert_eq!(
                lom_core::validate::validate_manifest(&manifest).is_ok(),
                valid_before,
                "Read-only painting changed manifest validity"
            );
        }
    }
    #[test]
    fn painting_preserves_unknown_fields_and_future_values() {
        let ctx = egui::Context::default();
        let catalog = Catalog::new();
        let mut m = json!({"id":"sample","entry":"main","campaign":{"new_game":true,"future":"keep","triggers":[{"type":"position","position":"future_position","script":"main","when_month":99,"when_affinity":{"character":"future_character","min":7},"extension":42}]}});
        let before = m.clone();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default()
                .show(ctx, |ui| show(ui, &mut m, &catalog, &["main".into()]));
        });
        assert_eq!(m, before);
    }
}
