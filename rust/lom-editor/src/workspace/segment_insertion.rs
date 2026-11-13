//! Copy an authored chapter into the original chapter's existing dispatcher.
use super::*;
use anyhow::{ensure, Result};

pub(super) fn insert_segment(
    mut target: Value,
    source: &Value,
    anchor: &str,
    before: bool,
) -> Result<(Value, usize, usize)> {
    ensure!(
        target["vanilla_import"]["schema"] == 2,
        "整段插入需要可编辑的原版章节"
    );
    ensure!(
        source.get("vanilla_import").is_none() && source.get("vanilla_patch").is_none(),
        "请选择本作品的普通剧情章节"
    );
    lom_core::validate::validate_story(source)?;
    let source_locale = lom_core::localization::localization_config(source);
    if target.get("localization").is_none() {
        if let Some(config) = &source_locale {
            target["localization"] = json!({"default_locale":config["default_locale"]});
        }
    }
    if source_locale.is_some() {
        let config = lom_core::localization::localization_config(&target).unwrap();
        let default = config["default_locale"].as_str().unwrap();
        let fallback = config["fallback_locale"].as_str().unwrap_or(default);
        if fallback != default {
            // Resolve the old chapter's fallback before combining two catalogs:
            // a copied fallback translation must not override its default text.
            let mut catalogs = BTreeMap::new();
            for locale in lom_core::localization::SUPPORTED_LOCALES {
                if locale != default {
                    catalogs.insert(
                        locale,
                        lom_core::localization::resolved_catalog(&target, locale)?,
                    );
                }
            }
            target = lom_core::localization::apply_story_locale(&target, default)?;
            target["localization"] = json!({"default_locale":default,"translations":catalogs});
        }
    }
    let localized = source_locale
        .as_ref()
        .map(|_| {
            lom_core::localization::apply_story_locale(
                source,
                target["localization"]["default_locale"].as_str().unwrap(),
            )
        })
        .transpose()?;
    let originals = localized.as_ref().unwrap_or(source)["nodes"]
        .as_array()
        .context("所选章节缺少步骤")?;
    ensure!(
        originals.iter().any(|node| node["type"] != "end"),
        "所选章节只有结束步骤，请先编写剧情"
    );
    for node in originals {
        let kind = node["type"].as_str().unwrap_or("");
        ensure!(!matches!(kind, "combat" | "battle" | "goto_scene" | "death" | "raw" | "block" | "native_action" | "native_branch"),
            "章节 {} 的步骤 {}（{}）会切换场景或执行原生逻辑，当前无法在原版中途执行后接回；请拆成演出段落或改用独立章节入口",
            source["id"], node["id"], kind);
        ensure!(
            kind != "end"
                || node["next_script"].as_str().unwrap_or("").is_empty()
                    && matches!(
                        node["event_result"].as_str().unwrap_or(""),
                        "" | "completed"
                    ),
            "章节 {} 的结束步骤 {} 连接其他章节或事件结果，请改为普通结束后再整段插入",
            source["id"],
            node["id"]
        );
    }
    let nodes = target["nodes"].as_array().context("原版章节缺少步骤")?;
    let at = nodes
        .iter()
        .position(|node| node["id"] == anchor)
        .context("插入位置已不存在")?;
    ensure!(nodes[at]["type"] == "say", "请选择原版对白作为插入位置");
    let continuation = if before {
        anchor.to_owned()
    } else {
        let next = lom_core::analysis::successors(&target, at);
        ensure!(next.len() == 1, "所选原句没有唯一的后续步骤，无法自动接回");
        next[0].clone()
    };
    let used: BTreeSet<_> = nodes
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect();
    let prefix = (1..)
        .map(|index| format!("segment{index}"))
        .find(|prefix| {
            !used.contains(format!("{prefix}_entry").as_str())
                && (0..originals.len())
                    .all(|index| !used.contains(format!("{prefix}_{index}").as_str()))
        })
        .unwrap();
    let mapping: BTreeMap<_, _> = originals
        .iter()
        .enumerate()
        .map(|(index, node)| {
            (
                node["id"].as_str().unwrap().to_owned(),
                format!("{prefix}_{index}"),
            )
        })
        .collect();
    let entry = mapping
        .get(source["start"].as_str().context("所选章节缺少入口")?)
        .context("所选章节入口不存在")?
        .clone();
    let no_goto = lom_core::validate::node_schema()["no_goto_types"]
        .as_array()
        .unwrap();
    let mut copied = originals.clone();
    for (index, node) in copied.iter_mut().enumerate() {
        node["id"] = json!(mapping[originals[index]["id"].as_str().unwrap()]);
        if let Some(editor) = node.get_mut("_editor").and_then(Value::as_object_mut) {
            editor.remove("vanilla_origin");
        }
        retarget(node, &mapping);
        if node["type"] == "end" {
            node.as_object_mut().unwrap().remove("next_script");
            node.as_object_mut().unwrap().remove("event_result");
            node["resume_original"] = json!(true);
            node["resume_node"] = json!(continuation);
        } else if !no_goto.iter().any(|kind| kind == &node["type"])
            && node["goto"].as_str().unwrap_or("").is_empty()
        {
            node["goto"] = json!(originals
                .get(index + 1)
                .and_then(|node| node["id"].as_str())
                .map(|id| mapping[id].clone())
                .unwrap_or_else(|| continuation.clone()));
        }
    }
    // Preserve the authored entry even when it is not the first physical node.
    if copied[0]["id"] != entry {
        copied.insert(
            0,
            json!({"id":format!("{prefix}_entry"),"type":"wait","seconds":0,"goto":entry,
            "_editor":{"note":crate::i18n::key("vanilla.segment_entry")}}),
        );
    }
    let count = copied.len();
    let start = if before {
        let redirects = BTreeMap::from([(
            anchor.to_owned(),
            copied[0]["id"].as_str().unwrap().to_owned(),
        )]);
        if target["start"] == anchor {
            target["start"] = copied[0]["id"].clone();
        }
        for node in target["nodes"].as_array_mut().unwrap() {
            retarget(node, &redirects);
        }
        target["nodes"]
            .as_array_mut()
            .unwrap()
            .splice(at..at, copied);
        at
    } else {
        super::steps::insert_nodes_after(&mut target, at, copied)?
    };
    if source_locale.is_some() {
        let default = lom_core::localization::normalize_locale(
            target["localization"]["default_locale"].as_str().unwrap(),
        )
        .to_owned();
        for locale in lom_core::localization::SUPPORTED_LOCALES {
            if locale == default {
                continue;
            }
            for (key, value) in lom_core::localization::resolved_catalog(source, locale)? {
                for (old, new) in &mapping {
                    if key.starts_with(&format!("{old}.")) {
                        target["localization"]["translations"][locale]
                            [format!("{new}{}", &key[old.len()..])] = json!(value);
                    }
                }
            }
        }
    }
    lom_core::native_flow::refresh_branch_labels(&mut target);
    lom_core::validate::validate_story(&target)?;
    Ok((target, start, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nlocal total=3\nif checkpointmanager.Condition(\"OUTER\") then\ntotal=total+4\nsay(\"原版甲\")\nelse\ntotal=total+7\nsay(\"原版乙\")\nend\nmark_progress(total)\nsay(\"原版尾句\")\n";

    fn original() -> Value {
        lom_core::vanilla_import::import(ORIGINAL, "main", "原版章节", "DEMO", &BTreeMap::new())
            .unwrap()
    }

    fn authored() -> Value {
        json!({"story_schema":2,"id":"extra","title":"新增段落","start":"intro","nodes":[
            {"id":"intro","type":"say","text":"段落开场","mode":"narrative"},
            {"id":"choice","type":"choice","options":[{"text":"甲","goto":"left"},{"text":"乙","goto":"right"}]},
            {"id":"left","type":"say","mode":"narrative","text":"段落甲","goto":"left_end"},
            {"id":"right","type":"say","mode":"narrative","text":"段落乙","goto":"right_end"},
            {"id":"left_end","type":"end","event_result":"completed"},
            {"id":"right_end","type":"end"}
        ]})
    }

    #[test]
    fn full_segment_keeps_both_choices_in_the_selected_original_branch_and_rejoins() {
        let original = original();
        let source = authored();
        let unchanged = source.clone();
        let nodes = original["nodes"].as_array().unwrap();
        let at = nodes
            .iter()
            .position(|node| node["text"] == "原版甲")
            .unwrap();
        let anchor = nodes[at]["id"].as_str().unwrap();
        let successor = nodes[at]["goto"].clone();
        let other = nodes
            .iter()
            .find(|node| node["type"] == "native_branch")
            .unwrap()["failure"]
            .clone();
        for before in [true, false] {
            let (inserted, start, count) =
                insert_segment(original.clone(), &source, anchor, before).unwrap();
            assert_eq!(count, 6);
            let copied = &inserted["nodes"].as_array().unwrap()[start..start + count];
            assert_eq!(copied[0]["goto"], copied[1]["id"]);
            assert_eq!(copied[1]["options"][0]["goto"], copied[2]["id"]);
            assert_eq!(copied[1]["options"][1]["goto"], copied[3]["id"]);
            assert_eq!(copied[2]["goto"], copied[4]["id"]);
            assert_eq!(copied[3]["goto"], copied[5]["id"]);
            for end in &copied[4..] {
                assert_eq!(end["resume_original"], true);
                assert_eq!(
                    end["resume_node"],
                    if before {
                        json!(anchor)
                    } else {
                        successor.clone()
                    }
                );
                assert!(end.get("event_result").is_none());
            }
            let branch = inserted["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["type"] == "native_branch")
                .unwrap();
            assert_eq!(branch["failure"], other);
            let incoming = nodes.iter().find(|node| node["goto"] == anchor).unwrap()["id"].clone();
            let incoming = inserted["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["id"] == incoming)
                .unwrap();
            assert_eq!(
                incoming["goto"],
                if before {
                    copied[0]["id"].clone()
                } else {
                    json!(anchor)
                }
            );
            assert_eq!(
                lom_core::vanilla_import::parse(&inserted)
                    .unwrap()
                    .unwrap()
                    .source(),
                ORIGINAL
            );
            lom_core::codegen::compile_story(&inserted, None, None, None).unwrap();
            let (again, _, _) = insert_segment(inserted, &source, anchor, before).unwrap();
            lom_core::validate::validate_story(&again).unwrap();
        }
        assert_eq!(source, unchanged);
    }

    #[test]
    fn segment_entry_and_translations_survive_different_chapter_languages() {
        let mut source = authored();
        source["start"] = json!("choice");
        source["localization"] = json!({"default_locale":"chs","fallback_locale":"cht","translations":{
            "cht":{"intro.text":"段落開場","left.text":"段落甲繁體"},
            "ja":{"left.text":"左の会話","choice.options.0.text":"左"}
        }});
        for target_language in [None, Some("ja")] {
            let mut target = original();
            if let Some(locale) = target_language {
                target["localization"] = json!({"default_locale":locale,"fallback_locale":"ko"});
            }
            let anchor = target["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["text"] == "原版甲")
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned();
            let (inserted, start, count) = insert_segment(target, &source, &anchor, false).unwrap();
            assert_eq!(count, 7);
            assert_eq!(
                inserted["nodes"][start]["goto"],
                inserted["nodes"][start + 2]["id"]
            );
            let left = inserted["nodes"][start + 3]["id"].as_str().unwrap();
            for locale in lom_core::localization::SUPPORTED_LOCALES {
                let expected = lom_core::localization::resolved_catalog(&source, locale).unwrap();
                let actual = lom_core::localization::resolved_catalog(&inserted, locale).unwrap();
                assert_eq!(actual[&format!("{left}.text")], expected["left.text"]);
                let choice = inserted["nodes"][start + 2]["id"].as_str().unwrap();
                assert_eq!(
                    actual[&format!("{choice}.options.0.text")],
                    expected["choice.options.0.text"]
                );
            }
        }
    }

    #[test]
    fn segment_reports_scene_switches_and_cross_chapter_ends_without_changing_source() {
        let target = original();
        let anchor = target["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["type"] == "say")
            .unwrap()["id"]
            .as_str()
            .unwrap();
        let mut source = authored();
        source["nodes"][4] = json!({"id":"left_end","type":"raw","code":"change_scene()"});
        assert!(insert_segment(target.clone(), &source, anchor, false)
            .unwrap_err()
            .to_string()
            .contains("left_end"));
        source = authored();
        source["nodes"][4]
            .as_object_mut()
            .unwrap()
            .remove("event_result");
        source["nodes"][4]["next_script"] = json!("elsewhere");
        assert!(insert_segment(target.clone(), &source, anchor, true)
            .unwrap_err()
            .to_string()
            .contains("left_end"));
        assert_eq!(target, original());
    }

    #[test]
    fn released_chapter_opens_copies_saves_exports_and_reopens_with_original_untouched() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lom-core/tests/fixtures/released-1.5.4-name-base.lommod");
        let bytes = std::fs::read(&fixture).unwrap();
        let old = Project::open(&fixture).unwrap();
        let mut project = Project::new_vanilla_patch("DEMO").unwrap();
        let target = original();
        let anchor = target["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["type"] == "say")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let (story, _, _) = insert_segment(target, &old.stories["main"], &anchor, false).unwrap();
        project.stories.insert("main".into(), story);
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("project");
        project.save_to(&directory).unwrap();
        let reopened = Project::open(&directory).unwrap();
        let package = temp.path().join("export.lommod");
        reopened.export(&package).unwrap();
        assert_eq!(Project::open(&package).unwrap().stories, project.stories);
        assert_eq!(
            lom_core::package::inspect_package(&package).unwrap()["source_lua_match"],
            true
        );
        assert_eq!(std::fs::read(fixture).unwrap(), bytes);
    }
}
