//! Reconnect the surviving routes when deleting structured original steps.
use super::*;
use lom_core::analysis::{follows_next, successors};

pub(super) struct StepDeletion {
    pub(super) removed: BTreeSet<String>,
    redirects: BTreeMap<String, String>,
    finish: BTreeSet<String>,
    routes: Vec<Route>,
    labels: BTreeMap<String, String>,
}

struct Route {
    from: String,
    targets: Vec<String>,
    selected: Option<String>,
    outcomes: BTreeMap<String, String>,
}

fn id(node: &Value) -> &str {
    node["id"].as_str().unwrap_or("")
}

fn resume(story: &Value, index: usize) -> Option<&str> {
    let node = &story["nodes"][index];
    if node["type"] != "end" || node["resume_original"] != true {
        return None;
    }
    lom_core::vanilla_import::resume_index(story, index).map(|next| id(&story["nodes"][next]))
}

impl StepDeletion {
    pub(super) fn plan(original: &Value, removed: BTreeSet<String>, catalog: &Catalog) -> Self {
        let nodes = original["nodes"]
            .as_array()
            .expect("structured story nodes");
        let positions: BTreeMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (id(node), index))
            .collect();
        let kept: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !removed.contains(id(node)))
            .collect();
        let last = kept.last().map(|(index, _)| *index);
        let mut incoming = BTreeMap::<String, Vec<usize>>::new();
        for &(index, _) in &kept {
            for target in successors(original, index) {
                if removed.contains(&target) {
                    incoming.entry(target).or_default().push(index);
                }
            }
        }
        let start = original["start"].as_str().unwrap_or("");
        if removed.contains(start) {
            incoming.entry(start.into()).or_default();
        }

        let mut redirects = BTreeMap::new();
        let mut finish = BTreeSet::new();
        let mut routes = Vec::new();
        for (from, predecessors) in incoming {
            let mut exits = BTreeSet::new();
            let mut visited = BTreeSet::new();
            let mut pending = vec![from.clone()];
            while let Some(step) = pending.pop() {
                let Some(&index) = positions.get(step.as_str()) else {
                    continue;
                };
                if !removed.contains(&step) {
                    exits.insert(index);
                } else if visited.insert(step) {
                    pending.extend(successors(original, index));
                }
            }
            match exits.len() {
                1 => {
                    redirects.insert(from, id(&nodes[*exits.first().unwrap()]).into());
                }
                0 if from != start
                    && predecessors.iter().all(|&index| {
                        Some(index) == last
                            && (nodes[index]["goto"] == from
                                || resume(original, index) == Some(from.as_str())
                                || (follows_next(&nodes[index])
                                    && nodes[index]["type"] != "branch"))
                    }) =>
                {
                    finish.insert(from);
                }
                _ => {
                    let selected = exits
                        .is_empty()
                        .then(|| {
                            let original_index = positions[from.as_str()];
                            kept.iter()
                                .find(|(index, _)| *index > original_index)
                                .map(|(_, step)| id(step).to_owned())
                        })
                        .flatten();
                    let targets = if exits.is_empty() {
                        kept.iter().map(|(_, node)| id(node).into()).collect()
                    } else {
                        exits
                            .into_iter()
                            .map(|index| id(&nodes[index]).into())
                            .collect()
                    };
                    let node = &nodes[positions[from.as_str()]];
                    let outcomes = [
                        ("success", "flow_graph.condition_true"),
                        ("failure", "flow_graph.condition_false"),
                    ]
                    .into_iter()
                    .filter_map(|(field, label)| {
                        Some((node[field].as_str()?.into(), crate::i18n::key(label)))
                    })
                    .collect();
                    routes.push(Route {
                        from,
                        targets,
                        selected,
                        outcomes,
                    });
                }
            }
        }
        routes.sort_by_key(|route| positions[route.from.as_str()]);
        let labels = routes
            .iter()
            .flat_map(|route| std::iter::once(&route.from).chain(&route.targets))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| {
                let index = positions[id.as_str()];
                let node = &nodes[index];
                let kind = catalog.label(node["type"].as_str().unwrap_or(""));
                let name = node["text"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| format!("{kind} · {}", short(text, 36)))
                    .unwrap_or(kind);
                (id.clone(), format!("{} · {name} · {id}", index + 1))
            })
            .collect();
        Self {
            removed,
            redirects,
            finish,
            routes,
            labels,
        }
    }

    pub(super) fn needs_confirmation(&self) -> bool {
        !self.routes.is_empty()
    }

    pub(super) fn ready(&self) -> bool {
        self.routes.iter().all(|route| route.selected.is_some())
    }

    #[cfg(test)]
    pub(super) fn choose(&mut self, from: &str, to: &str) {
        let route = self
            .routes
            .iter_mut()
            .find(|route| route.from == from)
            .unwrap();
        assert!(route.targets.iter().any(|target| target == to));
        route.selected = Some(to.into());
    }

    pub(super) fn apply(mut self, original: &Value, story: &mut Value) {
        let old_nodes = original["nodes"]
            .as_array()
            .expect("structured story nodes");
        for route in self.routes {
            self.redirects
                .insert(route.from, route.selected.expect("confirmed continuation"));
        }

        let mut kept = old_nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !self.removed.contains(id(node)))
            .peekable();
        let mut used: BTreeSet<_> = old_nodes.iter().map(id).map(str::to_owned).collect();
        let mut bridge = |target: &str| super::steps::continuation_step(&mut used, target);

        let mut nodes = story["nodes"].as_array_mut().unwrap().drain(..);
        let mut updated = Vec::with_capacity(nodes.len());
        if let Some(target) = original["start"]
            .as_str()
            .and_then(|start| self.redirects.get(start))
        {
            if kept.peek().is_some_and(|(_, next)| id(next) != target) {
                // The importer enters the first physical step. Keep the chosen
                // entry without moving either of its original branches.
                updated.push(bridge(target));
            }
        }
        for mut node in &mut nodes {
            let (index, old) = kept.next().expect("retained step");
            if !self.redirects.is_empty() {
                retarget(&mut node, &self.redirects);
            }
            for field in ["goto", "resume_node"] {
                if node[field]
                    .as_str()
                    .is_some_and(|target| self.finish.contains(target))
                {
                    node.as_object_mut().unwrap().remove(field);
                }
            }
            if old["resume_node"].is_null() && old["resume_original"] == true {
                if let Some(target) =
                    resume(original, index).and_then(|target| self.redirects.get(target))
                {
                    node["resume_node"] = json!(target);
                }
            }

            let next = old_nodes
                .get(index + 1)
                .and_then(|next| self.redirects.get(id(next)))
                .filter(|_| follows_next(old))
                .filter(|target| {
                    kept.peek()
                        .is_none_or(|(_, next)| id(next) != target.as_str())
                });
            let fallback = if let Some(target) = next {
                if old["type"] == "branch" {
                    // A condition's default path is its next physical step.
                    Some(bridge(target))
                } else {
                    node["goto"] = json!(target);
                    None
                }
            } else {
                None
            };
            updated.push(node);
            updated.extend(fallback);
        }
        drop(nodes);
        story["start"] = updated[0]["id"].clone();
        story["nodes"] = Value::Array(updated);
    }
}

impl App {
    pub(super) fn show_step_deletion(&mut self, ctx: &egui::Context, enter: bool, escape: bool) {
        let draft = self.step_delete_dialog.as_mut().unwrap();
        let mut cancel = escape;
        let mut submit = false;
        let key = crate::i18n::key;
        egui::Modal::new(egui::Id::new("step-delete"))
            .frame(shell::dialog_frame_for(ctx))
            .show(ctx, |ui| {
                ui.set_width(460.0_f32.min(ctx.content_rect().width() - 60.0));
                cancel |= shell::dialog_header(ui, &key("editor.step_delete.title"));
                ui.add_space(12.0);
                ui.label(key("editor.step_delete.hint"));
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for route in &mut draft.routes {
                            ui.label(&draft.labels[&route.from]);
                            route_picker(ui, route, &draft.labels);
                            ui.add_space(8.0);
                        }
                    });
                shell::dialog_buttons(ui, |ui| {
                    let ready = draft.ready();
                    let confirm =
                        ui.add_enabled(ready, egui::Button::new(key("editor.step_delete.title")));
                    submit = confirm.clicked() || (ready && enter && confirm.has_focus());
                    let dismiss = ui.button(tr("取消"));
                    cancel |= dismiss.clicked() || (enter && dismiss.has_focus());
                });
            });
        if cancel {
            self.step_delete_dialog = None;
        } else if submit {
            self.confirm_step_deletion();
        }
    }
}

fn route_picker(ui: &mut egui::Ui, route: &mut Route, labels: &BTreeMap<String, String>) {
    let placeholder = crate::i18n::key("editor.step_delete.select");
    let caption = |id: &String| {
        route.outcomes.get(id).map_or_else(
            || labels[id].clone(),
            |outcome| format!("{outcome} · {}", labels[id]),
        )
    };
    let current = route.selected.as_ref().map_or(placeholder, caption);
    crate::forms::resource_combo(ui, ("step-continuation", &route.from), &current, |ui| {
        let search = ui.id().with("search");
        let mut query = ui.data(|data| data.get_temp::<String>(search).unwrap_or_default());
        let change = ui
            .add(
                egui::TextEdit::singleline(&mut query)
                    .id(search)
                    .desired_width(f32::INFINITY)
                    .hint_text(crate::i18n::key("editor.form.search_choices")),
            )
            .changed();
        let filter = query.trim().to_lowercase();
        let matches: Vec<_> = route
            .targets
            .iter()
            .filter(|id| filter.is_empty() || caption(id).to_lowercase().contains(&filter))
            .collect();
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if change {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        let height = ui.spacing().interact_size.y;
        scroll.show_rows(ui, height, matches.len(), |ui, visible| {
            for target in &matches[visible] {
                if ui
                    .selectable_label(route.selected.as_ref() == Some(*target), caption(target))
                    .clicked()
                {
                    route.selected = Some((*target).clone());
                    ui.close();
                }
            }
        });
        ui.data_mut(|data| data.insert_temp(search, query));
    });
}
