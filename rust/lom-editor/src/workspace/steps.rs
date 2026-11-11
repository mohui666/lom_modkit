//! Step selection, insertion, clipboard and reordering.
use super::*;

impl App {
    pub(super) fn next_id(&self, kind: &str) -> String {
        let s = self.story();
        let ids: BTreeSet<_> = s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str())
            .collect();
        (1..)
            .map(|n| format!("{kind}{n}"))
            .find(|s| !ids.contains(s.as_str()))
            .unwrap()
    }
    pub(super) fn add_node(&mut self, kind: &str) {
        if matches!(kind, "native_action" | "native_branch")
            && self.story()["vanilla_import"]["schema"] != 2
        {
            self.error = Some("原版指令与原版条件只用于结构化导入的原版章节。".into());
            return;
        }
        if self.story().get("vanilla_import").is_some() && matches!(kind, "combat" | "battle") {
            self.error = Some("新增战斗会切换场景，当前原版整章的局部状态无法跨场景续接。".into());
            return;
        }
        self.flush();
        let mut node = self.catalog.new_node(kind, self.next_id(kind));
        if self.story().get("vanilla_import").is_some() {
            if kind == "end" {
                node["resume_original"] = json!(true);
            }
            if kind == "say" {
                node["inherit_dialog"] = json!(true);
            }
        }
        let mut story = self.story();
        match insert_nodes_after(&mut story, self.selected, vec![node]) {
            Ok(at) => {
                self.project.stories.insert(self.current.clone(), story);
                self.selected = at;
                self.multiselect.clear();
                self.center = Center::Node;
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    pub(super) fn selected_indices(&self) -> Vec<usize> {
        if self.multiselect.is_empty() {
            vec![self.selected]
        } else {
            self.multiselect.iter().copied().collect()
        }
    }
    pub(super) fn copy(&mut self) {
        let s = self.story();
        self.clipboard = self
            .selected_indices()
            .into_iter()
            .filter_map(|i| s["nodes"].get(i).cloned())
            .collect();
        self.status = format!("已复制 {} 个节点，可切换章节后粘贴", self.clipboard.len());
    }
    pub(super) fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.flush();
        let mut clones = self.clipboard.clone();
        let mut story = self.story();
        let imported = story.get("vanilla_import").is_some();
        if story["vanilla_import"]["schema"] != 2
            && clones.iter().any(|node| {
                matches!(
                    node["type"].as_str(),
                    Some("native_action" | "native_branch")
                )
            })
        {
            self.error = Some("原版指令与原版条件只用于结构化导入的原版章节。".into());
            return;
        }
        if imported
            && clones
                .iter()
                .any(|node| matches!(node["type"].as_str(), Some("combat" | "battle")))
        {
            self.error = Some("新增战斗会切换场景，当前原版整章的局部状态无法跨场景续接。".into());
            return;
        }
        let mut used: BTreeSet<String> = story["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str().map(str::to_owned))
            .collect();
        let mut map = BTreeMap::new();
        for n in &mut clones {
            let old = n["id"].as_str().unwrap_or("node").to_owned();
            let id = (1..)
                .map(|i| format!("{old}_copy{i}"))
                .find(|v| !used.contains(v))
                .unwrap();
            used.insert(id.clone());
            map.insert(old, id.clone());
            n["id"] = id.into();
            if let Some(editor) = n.get_mut("_editor").and_then(Value::as_object_mut) {
                editor.remove("vanilla_origin");
            }
            if !imported {
                n.as_object_mut().unwrap().remove("inherit_dialog");
                n.as_object_mut().unwrap().remove("resume_original");
            }
        }
        for n in &mut clones {
            retarget(n, &map);
        }
        if story["vanilla_import"]["schema"] == 2 {
            lom_core::native_flow::rebind_copied_locals(&mut clones, &story);
        }
        let count = clones.len();
        match insert_nodes_after(&mut story, self.selected, clones) {
            Ok(at) => {
                self.project.stories.insert(self.current.clone(), story);
                self.selected = at;
                self.multiselect = (at..at + count).collect();
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
    pub(super) fn delete(&mut self) {
        let selected: BTreeSet<_> = self.selected_indices().into_iter().collect();
        let old_nodes = self.story()["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let old_ids: Vec<String> = old_nodes
            .iter()
            .filter_map(|n| n["id"].as_str().map(str::to_owned))
            .collect();
        if old_nodes.len() <= selected.len() {
            self.error = Some("每个章节至少需要保留一个节点。".into());
            return;
        }
        let removed: BTreeSet<String> = selected
            .iter()
            .filter_map(|i| old_nodes.get(*i)?.get("id")?.as_str().map(str::to_owned))
            .collect();
        let mut story = self.story();
        story["nodes"]
            .as_array_mut()
            .unwrap()
            .retain(|n| !removed.contains(n["id"].as_str().unwrap_or("")));
        let reconnect = if story["vanilla_import"]["schema"] == 2 {
            reconnect_deleted_structured(&mut story, &old_nodes, &removed)
        } else {
            reconnect_deleted_offsets(&mut story, &old_nodes, &removed)
        };
        if let Err(error) = reconnect {
            self.error = Some(crate::error_ui::capture(error));
            return;
        }
        if removed.contains(story["start"].as_str().unwrap_or("")) {
            story["start"] = story["nodes"][0]["id"].clone();
        }
        repair_deleted_metadata(&mut story, &old_ids, &removed);
        self.flush();
        self.project.stories.insert(self.current.clone(), story);
        self.selected = self.selected.min(
            self.story()["nodes"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0)
                .saturating_sub(1),
        );
        self.multiselect.clear();
    }
    pub(super) fn move_node(&mut self, to: usize) {
        let indices = self.selected_indices();
        let Some(&first) = indices.first() else {
            return;
        };
        let Some(&last) = indices.last() else { return };
        let len = self.story()["nodes"].as_array().map_or(0, Vec::len);
        if to >= len || to == self.selected {
            return;
        }
        let boundary = if indices.len() == 1 {
            to + usize::from(to > first)
        } else if to < self.selected {
            if first == 0 {
                return;
            }
            first - 1
        } else {
            if last + 1 >= len {
                return;
            }
            last + 2
        };
        self.move_steps(&indices, boundary);
    }
    pub(super) fn move_steps(&mut self, indices: &[usize], boundary: usize) {
        let mut story = self.story();
        let Some(nodes) = story["nodes"].as_array_mut() else {
            return;
        };
        let selected: BTreeSet<_> = indices
            .iter()
            .copied()
            .filter(|i| *i < nodes.len())
            .collect();
        if selected.is_empty() || boundary > nodes.len() {
            return;
        }
        let follow = story["start"] == story["nodes"][0]["id"];
        let primary_index = if selected.contains(&self.selected) {
            self.selected
        } else {
            *selected.first().unwrap()
        };
        let primary = story["nodes"][primary_index]["id"].clone();
        let old_nodes = story["nodes"].as_array().unwrap();
        let at = boundary - selected.range(..boundary).count();
        let mut remaining: Vec<_> = old_nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| !selected.contains(i))
            .map(|(_, n)| n.clone())
            .collect();
        let moving: Vec<_> = selected.iter().map(|i| old_nodes[*i].clone()).collect();
        let count = moving.len();
        remaining.splice(at..at, moving);
        if remaining == *old_nodes {
            return;
        }
        story["nodes"] = json!(remaining);
        if follow {
            story["start"] = story["nodes"][0]["id"].clone();
        }
        self.flush();
        self.selected = story["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["id"] == primary)
            .unwrap_or(at);
        self.multiselect = (at..at + count).collect();
        self.selection_anchor = Some((self.current.clone(), self.selected));
        self.project.stories.insert(self.current.clone(), story);
        self.status =
            crate::i18n::message("workspace.steps.moved", &[("count", count.to_string())]);
    }
    pub(super) fn select_step(&mut self, index: usize, extend: bool, toggle: bool) {
        if extend {
            let anchor = self
                .selection_anchor
                .as_ref()
                .filter(|(chapter, _)| chapter == &self.current)
                .map_or(self.selected, |(_, i)| *i);
            self.multiselect = (anchor.min(index)..=anchor.max(index)).collect();
            self.selected = index;
        } else if toggle {
            if self.multiselect.is_empty() {
                self.multiselect.insert(self.selected);
            }
            if !self.multiselect.insert(index) {
                self.multiselect.remove(&index);
                self.selected = self.multiselect.iter().next().copied().unwrap_or(index);
            } else {
                self.selected = index;
            }
            self.selection_anchor = Some((self.current.clone(), self.selected));
        } else {
            self.multiselect.clear();
            self.selected = index;
            self.selection_anchor = Some((self.current.clone(), index));
        }
        self.center = Center::Node;
        self.scroll_selection = (self.current.clone(), self.selected);
    }
}

pub(super) fn primary_insertion_edge(node: &mut Value) -> Option<&mut Value> {
    let kind = node["type"].as_str().unwrap_or("").to_owned();
    if node.get("success").is_some() {
        return node.get_mut("success");
    }
    match kind.as_str() {
        "choice" => node
            .get_mut("options")?
            .as_array_mut()?
            .first_mut()?
            .get_mut("goto"),
        "branch" => node
            .get_mut("cases")?
            .as_array_mut()?
            .first_mut()?
            .get_mut("goto"),
        "dice" => {
            if node["bands"].is_array() {
                node.get_mut("bands")?
                    .as_array_mut()?
                    .first_mut()?
                    .get_mut("goto")
            } else {
                node.get_mut("options")?
                    .as_array_mut()?
                    .first_mut()?
                    .get_mut("goto_大成功")
            }
        }
        "end" if node["resume_original"] == true => Some(
            node.as_object_mut()?
                .entry("resume_node")
                .or_insert(Value::Null),
        ),
        _ if lom_core::validate::node_schema()["no_goto_types"]
            .as_array()?
            .iter()
            .any(|value| value == &kind) =>
        {
            None
        }
        _ => Some(node.as_object_mut()?.entry("goto").or_insert(Value::Null)),
    }
}

pub(super) fn insert_nodes_after(
    story: &mut Value,
    selected: usize,
    mut inserted: Vec<Value>,
) -> anyhow::Result<usize> {
    let original = story["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("章节缺少步骤"))?;
    let at = (selected + 1).min(original.len());
    if story["vanilla_import"]["schema"] == 2 && !inserted.is_empty() && selected < original.len() {
        let mut anchor = original[selected].clone();
        let fallback = lom_core::analysis::successors(story, selected);
        let linear = !lom_core::validate::node_schema()["no_goto_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == &anchor["type"])
            || anchor["type"] == "end" && anchor["resume_original"] == true;
        let edge = primary_insertion_edge(&mut anchor)
            .ok_or_else(|| anyhow::anyhow!("这个步骤结束当前流程，请在它之前或分支内部插入。"))?;
        let continuation = edge
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| (linear && fallback.len() == 1).then(|| fallback[0].clone()));
        *edge = inserted[0]["id"].clone();
        if let Some(continuation) = continuation {
            let ids: BTreeSet<_> = inserted
                .iter()
                .filter_map(|node| node["id"].as_str().map(str::to_owned))
                .collect();
            let block = json!({"nodes":inserted});
            let mut redirects = BTreeMap::from([(String::new(), continuation.clone())]);
            for index in 0..inserted.len() {
                for target in lom_core::analysis::successors(&block, index) {
                    if !ids.contains(&target) {
                        redirects.insert(target, continuation.clone());
                    }
                }
                if let Some(target) = inserted[index]["resume_node"]
                    .as_str()
                    .filter(|target| !ids.contains(*target))
                {
                    redirects.insert(target.to_owned(), continuation.clone());
                }
            }
            for index in 0..inserted.len() {
                let next = inserted
                    .get(index + 1)
                    .map(|node| node["id"].clone())
                    .unwrap_or(json!(continuation));
                let node = &mut inserted[index];
                retarget(node, &redirects);
                if node["type"] == "end"
                    && node["resume_original"] == true
                    && node["resume_node"].is_null()
                {
                    node["resume_node"] = next;
                } else if !lom_core::validate::node_schema()["no_goto_types"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|kind| kind == &node["type"])
                    && node["goto"].is_null()
                {
                    node["goto"] = next;
                }
            }
        }
        story["nodes"][selected] = anchor;
    }
    story["nodes"]
        .as_array_mut()
        .unwrap()
        .splice(at..at, inserted);
    Ok(at)
}
pub(super) fn reconnect_deleted_structured(
    story: &mut Value,
    old_nodes: &[Value],
    removed: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let mut original = story.clone();
    original["nodes"] = json!(old_nodes);
    let indices: BTreeMap<_, _> = old_nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| Some((node["id"].as_str()?, index)))
        .collect();
    let mut redirects = BTreeMap::new();
    for id in removed {
        let mut current = id.as_str();
        let mut visited = BTreeSet::new();
        while removed.contains(current) {
            anyhow::ensure!(
                visited.insert(current),
                "步骤“{id}”的删除范围形成循环，请先连接保留的后续步骤。"
            );
            let next = lom_core::analysis::successors(&original, indices[current]);
            anyhow::ensure!(
                next.len() <= 1,
                "步骤“{current}”有多个不同后继，请先选择并连接要保留的分支，再删除。"
            );
            let Some(target) = next.first() else { break };
            if !removed.contains(target) {
                redirects.insert(id.clone(), target.clone());
                break;
            }
            current = old_nodes[indices[target.as_str()]]["id"].as_str().unwrap();
        }
    }
    let forbidden: BTreeMap<_, _> = removed
        .iter()
        .map(|id| (id.clone(), String::new()))
        .collect();
    for node in story["nodes"].as_array_mut().unwrap() {
        retarget(node, &redirects);
        let mut probe = node.clone();
        retarget(&mut probe, &forbidden);
        anyhow::ensure!(
            probe == *node,
            "步骤“{}”仍指向没有明确后继的删除步骤，请先连接保留的后续步骤。",
            node["id"].as_str().unwrap_or("")
        );
    }
    if let Some(start) = story["start"]
        .as_str()
        .filter(|start| removed.contains(*start))
    {
        story["start"] = json!(redirects
            .get(start)
            .ok_or_else(|| anyhow::anyhow!("入口步骤没有明确后继，请先设置保留的入口。"))?);
    }
    Ok(())
}
pub(super) fn reconnect_deleted_offsets(
    story: &mut Value,
    old_nodes: &[Value],
    removed: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let offsets: BTreeMap<_, _> = old_nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node["type"] == "offset")
        .filter_map(|(index, node)| {
            let id = node["id"].as_str()?;
            removed.contains(id).then_some((id.to_owned(), index))
        })
        .collect();
    if offsets.is_empty() {
        return Ok(());
    }
    let original = json!({"nodes": old_nodes});
    let mut redirects = BTreeMap::new();
    let mut terminal = BTreeSet::new();
    for (id, &index) in &offsets {
        let mut current = index;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            // Follow the original links, including non-adjacent targets and
            // multi-deletions. Never pick an arbitrary arm of a deleted branch.
            let next = lom_core::analysis::successors(&original, current);
            let [next] = next.as_slice() else {
                if next.is_empty() {
                    terminal.insert(id.clone());
                }
                break;
            };
            let Some(index) = old_nodes.iter().position(|node| node["id"] == *next) else {
                break;
            };
            if !removed.contains(next) {
                redirects.insert(id.clone(), next.clone());
                break;
            }
            current = index;
        }
    }
    let forbidden: BTreeMap<_, _> = offsets
        .keys()
        .map(|id| (id.clone(), String::new()))
        .collect();
    let nodes = story["nodes"].as_array_mut().unwrap();
    let last = nodes.len().saturating_sub(1);
    for (index, node) in nodes.iter_mut().enumerate() {
        retarget(node, &redirects);
        // An automatically inserted final offset has no successor. Removing
        // its final incoming jump restores the original end of the sequence.
        if index == last
            && node["goto"]
                .as_str()
                .is_some_and(|id| terminal.contains(id))
        {
            node.as_object_mut().unwrap().remove("goto");
        }
        let mut probe = node.clone();
        retarget(&mut probe, &forbidden);
        anyhow::ensure!(
            probe == *node,
            "步骤“{}”的位移后续不明确，请先修改跳转后再删除。",
            node["id"].as_str().unwrap_or("")
        );
    }
    if let Some(next) = story["start"].as_str().and_then(|id| redirects.get(id)) {
        story["start"] = json!(next);
    }
    Ok(())
}
pub(super) fn repair_deleted_metadata(
    story: &mut Value,
    old_ids: &[String],
    removed: &BTreeSet<String>,
) {
    fn repair(item: &mut Value, old_ids: &[String], removed: &BTreeSet<String>) -> bool {
        let start = old_ids
            .iter()
            .position(|id| item["start"].as_str() == Some(id));
        let end = old_ids
            .iter()
            .position(|id| item["end"].as_str() == Some(id));
        if let (Some(start), Some(end)) = (start, end) {
            let survivors: Vec<_> = old_ids[start.min(end)..=start.max(end)]
                .iter()
                .filter(|id| !removed.contains(*id))
                .collect();
            if survivors.is_empty() {
                return false;
            }
            if removed.contains(item["start"].as_str().unwrap_or("")) {
                item["start"] = json!(if start <= end {
                    survivors[0]
                } else {
                    survivors[survivors.len() - 1]
                });
            }
            if removed.contains(item["end"].as_str().unwrap_or("")) {
                item["end"] = json!(if start <= end {
                    survivors[survivors.len() - 1]
                } else {
                    survivors[0]
                });
            }
        }
        if let Some(groups) = item.get_mut("groups").and_then(Value::as_array_mut) {
            groups.retain_mut(|group| repair(group, old_ids, removed));
        }
        true
    }
    if let Some(sections) = story
        .get_mut("_editor")
        .and_then(|e| e.get_mut("sections"))
        .and_then(Value::as_array_mut)
    {
        sections.retain_mut(|section| repair(section, old_ids, removed));
    }
    if let Some(translations) = story
        .get_mut("localization")
        .and_then(|l| l.get_mut("translations"))
        .and_then(Value::as_object_mut)
    {
        for catalog in translations.values_mut() {
            if let Some(entries) = catalog.as_object_mut() {
                entries.retain(|key, _| !removed.contains(key.split('.').next().unwrap_or("")));
            }
        }
    }
}
