use super::{content_specs, nodes, text, Stories};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn bundled_paths(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .map(|s| s.replace('\\', "/").trim_start_matches('/').to_owned())
        .filter(|s| s.starts_with("assets/"))
        .collect()
}
pub fn referenced_asset_paths(stories: &Stories, assets: &[String]) -> BTreeSet<String> {
    let bundled = bundled_paths(assets);
    let direct: BTreeSet<_> = stories
        .values()
        .flat_map(nodes)
        .filter_map(|n| n["image"].as_str())
        .map(|s| s.replace('\\', "/"))
        .collect();
    let ids: BTreeSet<_> = stories
        .values()
        .flat_map(nodes)
        .flat_map(content_specs)
        .filter_map(|r| r.raw.strip_prefix("user:"))
        .collect();
    bundled
        .into_iter()
        .filter(|p| {
            direct.contains(p)
                || ids
                    .iter()
                    .any(|id| format!("/{p}").contains(&format!("/{id}/")))
        })
        .collect()
}
pub fn unused_asset_paths(stories: &Stories, assets: &[String]) -> Vec<String> {
    bundled_paths(assets)
        .difference(&referenced_asset_paths(stories, assets))
        .cloned()
        .collect()
}
pub fn calculate_project_statistics(stories: &Stories, assets: Option<&[String]>) -> Value {
    let mut types: BTreeMap<&str, usize> = BTreeMap::new();
    let mut chars = BTreeSet::new();
    let mut images = BTreeSet::new();
    let mut audio = BTreeSet::new();
    let mut dialogue = 0;
    let mut voiced = 0;
    let mut options = 0;
    let mut endings = 0;
    let mut unreachable = 0;
    for story in stories.values().filter(|s| s.is_object()) {
        let reached = super::stage::reached(story);
        unreachable += nodes(story)
            .iter()
            .filter(|n| !reached.contains(text(n, "id")))
            .count();
        for n in nodes(story).iter().filter(|n| n.is_object()) {
            let kind = text(n, "type");
            *types.entry(kind).or_default() += 1;
            if !text(n, "character").is_empty() {
                chars.insert(text(n, "character"));
            }
            if kind == "say" {
                dialogue += 1;
                if !text(n, "voice").trim().is_empty() {
                    voiced += 1;
                }
            }
            if kind == "choice" {
                options += n["options"].as_array().map_or(0, Vec::len);
            }
            if ["end", "death", "goto_scene", "combat", "battle"].contains(&kind) {
                endings += 1;
            }
            for r in content_specs(n) {
                if r.raw.starts_with("user:") {
                    match r.kind {
                        "audio" => {
                            audio.insert(r.raw.to_owned());
                        }
                        "image" => {
                            images.insert(r.raw.to_owned());
                        }
                        _ => {}
                    }
                }
            }
            let image = text(n, "image");
            if !image.is_empty() && !image.starts_with("user:") {
                images.insert(image.replace('\\', "/"));
            }
        }
    }
    json!({"stories":stories.values().filter(|s|s.is_object()).count(),"nodes":types.values().sum::<usize>(),"node_types":types,"dialogue_count":dialogue,"choice_nodes":types.get("choice").copied().unwrap_or(0),"choice_options":options,"endings":endings,"characters":chars.len(),"images":images.len(),"audio":audio.len(),"voiced_dialogue":voiced,"unvoiced_dialogue":dialogue-voiced,"voice_coverage":if dialogue==0{0.0}else{voiced as f64/dialogue as f64*100.0},"unreachable_nodes":unreachable,"unused_assets":assets.map(|a|unused_asset_paths(stories,a).len())})
}
fn row(scope: &str, key: &str, label: &str, counts: [usize; 2]) -> Value {
    let total = counts[0] + counts[1];
    json!({"scope":scope,"key":key,"label":label,"voiced":counts[0],"unvoiced":counts[1],"total":total,"percent":if total==0{0.0}else{counts[0] as f64/total as f64*100.0}})
}
pub fn calculate_voice_coverage(stories: &Stories) -> Value {
    let mut by_story = BTreeMap::new();
    let mut by_char: BTreeMap<String, ([usize; 2], String)> = BTreeMap::new();
    let mut missing = Vec::new();
    let mut total = [0, 0];
    for (sid, s) in stories.iter().filter(|(_, s)| s.is_object()) {
        let title = if text(s, "title").is_empty() {
            sid
        } else {
            text(s, "title")
        };
        let mut count = [0, 0];
        for n in nodes(s).iter().filter(|n| n["type"] == "say") {
            let (id, label) = if ["narrative", "center"].contains(&text(n, "mode")) {
                ("__narrator__", "旁白")
            } else if !text(n, "character").is_empty() {
                (text(n, "character"), text(n, "character"))
            } else {
                ("__unspecified__", "未指定人物")
            };
            let ix = usize::from(text(n, "voice").trim().is_empty());
            count[ix] += 1;
            total[ix] += 1;
            by_char.entry(id.into()).or_insert(([0, 0], label.into())).0[ix] += 1;
            if ix == 1 {
                missing.push(json!({"story_id":sid,"story_title":title,"node_id":text(n,"id"),"character_id":id,"character_label":label,"text":text(n,"text")}));
            }
        }
        by_story.insert(sid, row("story", sid, title, count));
    }
    let mut chars: Vec<_> = by_char.into_iter().collect();
    use unicode_casefold::UnicodeCaseFold;
    chars.sort_by_key(|(id, (_, label))| {
        (id != "__narrator__", label.case_fold().collect::<String>())
    });
    json!({"total":row("total","total","项目总计",total),"stories":by_story.into_values().collect::<Vec<_>>(),"characters":chars.into_iter().map(|(id,(counts,label))|row("character",&id,&label,counts)).collect::<Vec<_>>(),"unvoiced_dialogues":missing})
}
