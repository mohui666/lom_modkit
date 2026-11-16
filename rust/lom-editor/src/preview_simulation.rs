//! Reuse stage prefixes without retaining a full stage for every story node.
use super::{effects, initial_state, text, Simulation};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

const CHECKPOINT_STRIDE: usize = 64;
const MAX_CHECKPOINTS: usize = 12;
const MAX_CHECKPOINT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Routes {
    has_nodes: bool,
    nodes: BTreeMap<String, usize>,
    previous: BTreeMap<String, Previous>,
}
struct Previous {
    node: Option<String>,
    depth: usize,
}
impl Routes {
    fn new(story: &Value) -> Self {
        let Some(nodes) = story["nodes"].as_array() else {
            return Self::default();
        };
        let mut result = Self {
            has_nodes: true,
            ..Self::default()
        };
        let mut links: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            if let Some(id) = node["id"].as_str() {
                // The existing simulator applies the first duplicate ID, while
                // route discovery includes links from every occurrence.
                result.nodes.entry(id.to_owned()).or_insert(index);
            }
            links
                .entry(text(node, "id"))
                .or_default()
                .extend(lom_core::analysis::successors(story, index));
        }
        let Some(start) = story["start"]
            .as_str()
            .or_else(|| nodes.first()?.get("id")?.as_str())
        else {
            return result;
        };
        result.previous.insert(
            start.to_owned(),
            Previous {
                node: None,
                depth: 1,
            },
        );
        let mut queue = VecDeque::from([start.to_owned()]);
        while let Some(id) = queue.pop_front() {
            let depth = result.previous[&id].depth + 1;
            for next in links.get(id.as_str()).into_iter().flatten() {
                if !result.previous.contains_key(next) {
                    result.previous.insert(
                        next.clone(),
                        Previous {
                            node: Some(id.clone()),
                            depth,
                        },
                    );
                    queue.push_back(next.clone());
                }
            }
        }
        result
    }

    fn path(&self, target: &str) -> Option<Vec<String>> {
        self.previous.get(target)?;
        let mut path = vec![target.to_owned()];
        let mut current = target;
        while let Some(previous) = self
            .previous
            .get(current)
            .and_then(|route| route.node.as_ref())
        {
            path.push(previous.clone());
            current = previous;
        }
        path.reverse();
        Some(path)
    }
}

#[derive(Clone)]
struct Stage {
    value: Value,
    effects: effects::Simulation,
    // Offline state exposes uncaptured effects. Rendering additionally needs
    // library-aware restart/end decisions, so keep that timeline separately.
    rendered_effects: Option<effects::Simulation>,
}
impl Stage {
    fn new(story: &Value, library: Option<&effects::Library>, has_effects: bool) -> Self {
        Self {
            value: initial_state(story),
            effects: effects::Simulation::default(),
            rendered_effects: (has_effects && library.is_some()).then(effects::Simulation::default),
        }
    }

    fn apply(
        &mut self,
        node: &Value,
        id: &str,
        managed: bool,
        has_effects: bool,
        library: Option<&effects::Library>,
    ) {
        super::apply(&mut self.value, node);
        if has_effects {
            self.effects.apply_node(node, id, managed, None);
            if let Some(effects) = &mut self.rendered_effects {
                effects.apply_node(node, id, managed, library);
            }
        }
    }

    fn advance_after(
        &mut self,
        node: &Value,
        has_effects: bool,
        library: Option<&effects::Library>,
    ) {
        if has_effects {
            self.effects.advance_after(node, None);
            if let Some(effects) = &mut self.rendered_effects {
                effects.advance_after(node, library);
            }
        }
    }

    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + value_bytes(&self.value)
            + self.effects.retained_bytes()
            + self
                .rendered_effects
                .as_ref()
                .map_or(0, effects::Simulation::retained_bytes)
    }
}

struct Checkpoint {
    node: String,
    depth: usize,
    stage: Stage,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Cache {
    story: Option<Value>,
    revision: Option<u64>,
    routes: Routes,
    has_effects: bool,
    managed_effects: bool,
    checkpoints: VecDeque<Checkpoint>,
    checkpoint_bytes: usize,
    #[cfg(test)]
    last_applied_nodes: usize,
}
impl Cache {
    pub(super) fn node_path(&self, target: &str) -> Vec<Option<usize>> {
        self.routes
            .path(target)
            .unwrap_or_else(|| vec![target.to_owned()])
            .iter()
            .map(|id| self.routes.nodes.get(id).copied())
            .collect()
    }

    pub(super) fn synchronize(&mut self, story: &Value, revision: Option<u64>) -> bool {
        if self.story.as_ref().is_some_and(|cached| {
            revision.is_some() && revision == self.revision || crate::json::same(cached, story)
        }) {
            self.revision = revision;
            return false;
        }
        let has_effects =
            lom_core::parallel::story_actions(story).any(|(_, node)| node["type"] == "effect");
        let managed_effects = effects::managed(story);
        if has_effects == self.has_effects && managed_effects == self.managed_effects {
            if let Some((depth, changed)) = self.unchanged_prefix_depth(story) {
                // Text and action edits usually leave the route unchanged. Keep
                // the stages before the first edited node instead of replaying
                // hundreds of preceding steps on every keystroke or drag.
                self.checkpoints.retain(|point| point.depth < depth);
                self.checkpoint_bytes = self.checkpoints.iter().map(|point| point.bytes).sum();
                let nodes = self.story.as_mut().unwrap()["nodes"]
                    .as_array_mut()
                    .unwrap();
                for index in changed {
                    nodes[index] = story["nodes"][index].clone();
                }
                self.revision = revision;
                return true;
            }
        }
        *self = Self {
            story: Some(story.clone()),
            revision,
            routes: Routes::new(story),
            has_effects,
            managed_effects,
            ..Self::default()
        };
        true
    }

    fn unchanged_prefix_depth(&self, story: &Value) -> Option<(usize, Vec<usize>)> {
        let cached = self.story.as_ref()?;
        // Imported resume links may depend on other, unchanged nodes too.
        if story.get("vanilla_import").is_some() {
            return None;
        }
        let (old_fields, fields) = (cached.as_object()?, story.as_object()?);
        if old_fields.len() != fields.len()
            || old_fields.iter().any(|(key, value)| {
                key != "nodes"
                    && fields
                        .get(key)
                        .is_none_or(|current| !crate::json::same(value, current))
            })
        {
            return None;
        }
        let (old_nodes, nodes) = (cached["nodes"].as_array()?, story["nodes"].as_array()?);
        if old_nodes.len() != nodes.len() {
            return None;
        }
        let mut depth = usize::MAX;
        let mut changed = Vec::new();
        for (index, (old, node)) in old_nodes.iter().zip(nodes).enumerate() {
            if crate::json::same(old, node) {
                continue;
            }
            if old["id"] != node["id"]
                || lom_core::analysis::successors(cached, index)
                    != lom_core::analysis::successors(story, index)
            {
                return None;
            }
            if let Some(previous) = self.routes.previous.get(text(node, "id")) {
                depth = depth.min(previous.depth);
            }
            changed.push(index);
        }
        Some((depth, changed))
    }

    fn remember(&mut self, node: &str, depth: usize, stage: &Stage) {
        let bytes = std::mem::size_of::<Checkpoint>() + node.len() + stage.retained_bytes();
        if bytes > MAX_CHECKPOINT_BYTES {
            return;
        }
        if let Some(index) = self.checkpoints.iter().position(|point| point.node == node) {
            self.checkpoint_bytes -= self.checkpoints.remove(index).unwrap().bytes;
        }
        while self.checkpoints.len() >= MAX_CHECKPOINTS
            || self.checkpoint_bytes + bytes > MAX_CHECKPOINT_BYTES
        {
            self.checkpoint_bytes -= self.checkpoints.pop_front().unwrap().bytes;
        }
        self.checkpoints.push_back(Checkpoint {
            node: node.to_owned(),
            depth,
            stage: stage.clone(),
            bytes,
        });
        self.checkpoint_bytes += bytes;
    }

    pub(super) fn simulate(
        &mut self,
        story: &Value,
        target: &str,
        library: Option<&effects::Library>,
    ) -> Simulation {
        #[cfg(test)]
        {
            self.last_applied_nodes = 0;
        }
        if !self.routes.has_nodes {
            let stage = Stage::new(story, library, self.has_effects);
            return Simulation {
                target: target.to_owned(),
                before: stage.value.clone(),
                after: stage.value,
                has_effects: self.has_effects,
                effects: effects::Simulation::default(),
            };
        }
        let route = self.routes.previous.get(target);
        let reached = route.is_some();
        let steps = route.map_or(1, |route| route.depth);
        let mut walk = vec![target.to_owned()];
        let mut closest = None;
        if reached {
            // Only prefixes preceding the target are usable: state_before must
            // still contain the old dialog, actors and effects at that boundary.
            let mut current = target;
            while let Some(previous) = self
                .routes
                .previous
                .get(current)
                .and_then(|route| route.node.as_ref())
            {
                closest = self
                    .checkpoints
                    .iter()
                    .position(|point| point.node == *previous);
                if closest.is_some() {
                    break;
                }
                walk.push(previous.clone());
                current = previous;
            }
        }
        walk.reverse();
        let (mut stage, completed) = if let Some(index) = closest {
            let point = self.checkpoints.remove(index).unwrap();
            let result = (point.stage.clone(), point.depth);
            self.checkpoints.push_back(point);
            result
        } else {
            (Stage::new(story, library, self.has_effects), 0)
        };
        let nodes = story["nodes"].as_array().unwrap();
        for (index, id) in walk.iter().enumerate().take(walk.len() - 1) {
            if let Some(node) = self.routes.nodes.get(id).map(|index| &nodes[*index]) {
                stage.apply(node, id, self.managed_effects, self.has_effects, library);
                stage.advance_after(node, self.has_effects, library);
                #[cfg(test)]
                {
                    self.last_applied_nodes += 1;
                }
            }
            let depth = completed + index + 1;
            if reached && depth % CHECKPOINT_STRIDE == 0 {
                self.remember(id, depth, &stage);
            }
        }
        let mut before = stage.value.clone();
        before["effects"] = stage.effects.sample(0.0, None);
        let node = self.routes.nodes.get(target).map(|index| &nodes[*index]);
        if let Some(node) = node {
            stage.apply(
                node,
                target,
                self.managed_effects,
                self.has_effects,
                library,
            );
            #[cfg(test)]
            {
                self.last_applied_nodes += 1;
            }
        }
        let effects = stage
            .rendered_effects
            .as_ref()
            .unwrap_or(&stage.effects)
            .clone();
        let after_effects = stage.effects.sample(0.0, None);
        if reached {
            if let Some(node) = node {
                stage.advance_after(node, self.has_effects, library);
            }
            self.remember(target, steps, &stage);
        }
        let mut after = stage.value;
        for state in [&mut before, &mut after] {
            state["reached"] = reached.into();
            state["steps"] = steps.into();
        }
        after["effects"] = after_effects;
        Simulation {
            target: target.to_owned(),
            before,
            after,
            has_effects: self.has_effects,
            effects,
        }
    }
}

// Conservative accounting of cloned JSON payloads. The checkpoint count is
// separately capped, so allocator/map overhead cannot grow with node count.
pub(super) fn value_bytes(value: &Value) -> usize {
    std::mem::size_of::<Value>()
        + match value {
            Value::String(value) => value.len(),
            Value::Array(values) => values.iter().map(value_bytes).sum(),
            Value::Object(values) => values
                .iter()
                .map(|(key, value)| {
                    key.len() + 8 * std::mem::size_of::<usize>() + value_bytes(value)
                })
                .sum(),
            _ => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_states(cache: &mut Cache, story: &Value, target: &str) {
        let actual = cache.simulate(story, target, None);
        assert_eq!(
            actual.before,
            super::super::simulate_until(story, target, false)
        );
        assert_eq!(actual.after, super::super::simulate(story, target));
    }

    #[test]
    fn adjacent_targets_apply_only_new_nodes_and_checkpoints_remain_bounded() {
        let mut nodes = vec![json!({"id":"n0","type":"show","character":"player"})];
        nodes.extend((1..1800).map(|index| {
            json!({"id":format!("n{index}"),"type":"offset","character":"player","x":index})
        }));
        let story = json!({"start":"n0","nodes":nodes});
        let mut cache = Cache::default();
        assert!(cache.synchronize(&story, None));
        assert_states(&mut cache, &story, "n1500");
        assert_eq!(cache.last_applied_nodes, 1501);
        assert_states(&mut cache, &story, "n1501");
        assert_eq!(cache.last_applied_nodes, 1);
        assert_states(&mut cache, &story, "n1502");
        assert_eq!(cache.last_applied_nodes, 1);
        for target in ["n80", "n81", "n1700", "n1701", "n0"] {
            assert_states(&mut cache, &story, target);
            assert!(cache.checkpoints.len() <= MAX_CHECKPOINTS);
            assert!(cache.checkpoint_bytes <= MAX_CHECKPOINT_BYTES);
        }
    }

    #[test]
    fn branch_loop_parallel_unreachable_and_missing_targets_match_full_replay() {
        let story = json!({"start":"start","mood":true,"nodes":[
            {"id":"start","type":"choice","options":[{"goto":"left"},{"goto":"right"}]},
            {"id":"left","type":"parallel","goto":"join","actions":[
                {"type":"show","character":"left","position":"L1"},
                {"type":"effect","name":"Rain_001","tag":"weather","stop_after":0.2,"wait_after":0.1},
                {"type":"camera","name":"Gray","active":true}]},
            {"id":"right","type":"show","character":"right","goto":"join"},
            {"id":"join","type":"choice","options":[{"goto":"loop"},{"goto":"done"}]},
            {"id":"loop","type":"hide","character":"left","goto":"join"},
            {"id":"done","type":"end"},
            {"id":"isolated","type":"parallel","actions":[
                {"type":"show","character":"isolated"},
                {"type":"effect","name":"Glow_001","c":0}]}
        ]});
        let mut cache = Cache::default();
        cache.synchronize(&story, None);
        for target in [
            "left", "join", "right", "done", "loop", "isolated", "missing", "join",
        ] {
            assert_states(&mut cache, &story, target);
        }
        let join = cache.simulate(&story, "join", None);
        assert!(join.before["actors"].get("left").is_some());
        assert!(join.before["actors"].get("right").is_none());
        assert_eq!(join.after["steps"], 3);
    }

    #[test]
    fn duplicate_ids_and_missing_start_nodes_preserve_existing_route_rules() {
        for story in [
            json!({"start":"same","nodes":[
                {"id":"same","type":"show","character":"first","goto":"a"},
                {"id":"same","type":"show","character":"second","goto":"b"},
                {"id":"a","type":"end"},{"id":"b","type":"end"}]}),
            json!({"start":"missing","nodes":[{"id":"a","type":"show","character":"first"}]}),
            json!({"start":"missing","nodes":[]}),
            json!({"nodes":[]}),
            json!({"mood":true}),
        ] {
            let mut cache = Cache::default();
            cache.synchronize(&story, None);
            for target in ["a", "same", "b", "missing"] {
                assert_eq!(
                    cache.routes.path(target),
                    super::super::route(&story, target)
                );
                assert_states(&mut cache, &story, target);
            }
        }
    }

    #[test]
    fn revisions_preserve_unchanged_checkpoints_and_edits_or_undo_invalidate_them() {
        let original = json!({"start":"a","nodes":[
            {"id":"a","type":"show","character":"player","position":"L1"},
            {"id":"b","type":"say","text":"before"} ]});
        let mut cache = Cache::default();
        assert!(cache.synchronize(&original, Some(1)));
        assert_states(&mut cache, &original, "a");
        assert!(!cache.synchronize(&original.clone(), Some(1)));
        assert!(!cache.synchronize(&original, Some(2)));
        assert_eq!(cache.checkpoints.len(), 1);
        let mut edited = original.clone();
        edited["nodes"][0]["position"] = json!("R1");
        assert!(cache.synchronize(&edited, Some(3)));
        assert!(cache.checkpoints.is_empty());
        assert_states(&mut cache, &edited, "b");
        assert!(cache.synchronize(&original, Some(4)));
        assert_states(&mut cache, &original, "b");
        edited["nodes"][0]["goto"] = json!("missing");
        assert!(cache.synchronize(&edited, None));
        assert_states(&mut cache, &edited, "b");
    }

    #[test]
    fn typing_and_parallel_edits_keep_the_unchanged_large_chapter_prefix() {
        let mut nodes = vec![json!({"id":"n0","type":"show","character":"player"})];
        nodes.extend(
            (1..999).map(|index| json!({"id":format!("n{index}"),"type":"say","text":"原对白"})),
        );
        nodes.push(json!({"id":"target","type":"say","text":"末句"}));
        let original = json!({"start":"n0","nodes":nodes});
        let mut story = original.clone();
        let mut cache = Cache::default();
        cache.synchronize(&story, Some(1));
        assert_states(&mut cache, &story, "target");
        assert_eq!(cache.last_applied_nodes, 1000);

        for revision in 2..6 {
            story["nodes"][999]["text"] = json!(format!("连续输入 {revision}"));
            assert!(cache.synchronize(&story, Some(revision)));
            assert_states(&mut cache, &story, "target");
            assert!(cache.last_applied_nodes <= CHECKPOINT_STRIDE);
        }
        story["nodes"][999] = json!({"id":"target","type":"parallel","actions":[
            {"type":"offset","character":"player","x":40,"duration":0.5},
            {"type":"wait","seconds":1}
        ]});
        cache.synchronize(&story, Some(6));
        assert_states(&mut cache, &story, "target");
        assert!(cache.last_applied_nodes <= CHECKPOINT_STRIDE);
        story["nodes"][999]["actions"][0]["x"] = json!(60);
        cache.synchronize(&story, Some(7));
        assert_states(&mut cache, &story, "target");
        assert!(cache.last_applied_nodes <= CHECKPOINT_STRIDE);

        // Undo an earlier stage change as well as the text/action edits.
        story["nodes"][0]["position"] = json!("R1");
        cache.synchronize(&story, Some(8));
        assert!(cache.checkpoints.is_empty());
        assert_states(&mut cache, &story, "target");
        cache.synchronize(&original, Some(9));
        assert_states(&mut cache, &original, "target");
        assert!(cache.checkpoints.len() <= MAX_CHECKPOINTS);
        assert!(cache.checkpoint_bytes <= MAX_CHECKPOINT_BYTES);
    }

    #[test]
    fn editing_routes_initial_state_or_global_effect_mode_discards_prefixes() {
        let original = json!({"start":"show","nodes":[
            {"id":"show","type":"show","character":"player"},
            {"id":"line","type":"say","text":"正文"},
            {"id":"end","type":"end"}
        ]});
        for edited in [
            {
                let mut s = original.clone();
                s["nodes"][0]["goto"] = json!("end");
                s
            },
            {
                let mut s = original.clone();
                s["mood"] = json!(true);
                s
            },
            {
                let mut s = original.clone();
                s["nodes"][1] = json!({"id":"line","type":"effect","name":"Rain_001","tag":"rain","start_delay":0.5});
                s
            },
        ] {
            let mut cache = Cache::default();
            cache.synchronize(&original, Some(1));
            assert_states(&mut cache, &original, "end");
            cache.synchronize(&edited, Some(2));
            assert!(cache.checkpoints.is_empty());
            assert_states(&mut cache, &edited, "end");
        }
    }

    #[test]
    fn cached_prefixes_keep_delayed_effect_time_and_temporary_narrative_dimming() {
        let story = json!({"start":"enter","nodes":[
            {"id":"enter","type":"parallel","actions":[
                {"type":"show","character":"a"}, {"type":"show","character":"b"}]},
            {"id":"group","type":"parallel","actions":[
                {"type":"effect","name":"Rain_001","c":0,"start_delay":0.5},
                {"type":"wait","seconds":1.0}]},
            {"id":"focus","type":"say","mode":"narrative","highlight_characters":["a"],"text":"旁白"},
            {"id":"wait","type":"wait","seconds":0.5},
            {"id":"line","type":"say","character":"b","text":"对话"}
        ]});
        let mut cache = Cache::default();
        cache.synchronize(&story, Some(1));
        for target in ["group", "focus", "wait", "line", "focus", "wait"] {
            assert_states(&mut cache, &story, target);
        }
        let waited = cache.simulate(&story, "wait", None);
        assert_eq!(waited.before["actors"]["b"]["dimmed"], true);
        assert_eq!(waited.after["actors"]["b"]["dimmed"], false);
    }

    #[test]
    fn oversized_stages_are_not_retained_as_checkpoints() {
        let story =
            json!({"nodes":[{"id":"a","type":"say","text":"x".repeat(MAX_CHECKPOINT_BYTES)}]});
        let mut cache = Cache::default();
        cache.synchronize(&story, None);
        let result = cache.simulate(&story, "a", None);
        assert_eq!(
            result.after["dialog"]["text"].as_str().unwrap().len(),
            MAX_CHECKPOINT_BYTES
        );
        assert!(cache.checkpoints.is_empty());
        assert_eq!(cache.checkpoint_bytes, 0);
    }
}
