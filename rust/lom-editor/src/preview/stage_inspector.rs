use super::*;

pub(crate) fn state_origin(
    story: &Value,
    target: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Option<(usize, Option<usize>)> {
    let nodes = story["nodes"].as_array()?;
    let path = route(story, target).unwrap_or_else(|| vec![target.to_owned()]);
    for id in path.iter().rev() {
        let index = nodes.iter().position(|node| node["id"] == *id)?;
        let node = &nodes[index];
        if node["type"] == "parallel" {
            if let Some(action) = lom_core::parallel::actions(node)
                .iter()
                .rposition(&predicate)
            {
                return Some((index, Some(action)));
            }
        } else if predicate(node) {
            return Some((index, None));
        }
    }
    None
}

fn origin_link(
    ui: &mut egui::Ui,
    label: String,
    origin: Option<(usize, Option<usize>)>,
    destination: &mut Option<(usize, Option<usize>)>,
) {
    if let Some(origin) = origin {
        if ui
            .small_button(label)
            .on_hover_text(crate::i18n::key("usability.state.locate"))
            .clicked()
        {
            *destination = Some(origin);
        }
    } else {
        ui.small(label);
    }
}

impl Preview {
    pub(crate) fn stage_inspector(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        story: &Value,
        selected: usize,
    ) -> Option<(usize, Option<usize>)> {
        let node = &story["nodes"][selected];
        let target = text(node, "id");
        let state = self.state(story, target).clone();
        let mut destination = None;
        egui::CollapsingHeader::new(crate::i18n::key("usability.state.title"))
            .id_salt("stage-state-inspector")
            .show(ui, |ui| {
                ui.small(crate::i18n::key("usability.state.hint"));
                egui::ScrollArea::vertical()
                    .id_salt("stage-state-rows")
                    .max_height(190.0)
                    .show(ui, |ui| {
                        if state["reached"] == false {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                crate::i18n::key("usability.state.unreachable"),
                            );
                        }
                        let blackout = number(&state["blackout"], "amount", 0.0) >= 1.0;
                        if blackout {
                            origin_link(
                                ui,
                                crate::i18n::key("usability.state.blackout"),
                                state_origin(story, target, |n| n["type"] == "transition"),
                                &mut destination,
                            );
                        }
                        let mut ids: std::collections::BTreeSet<String> = state["actors"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .map(|(id, _)| id.clone())
                            .collect();
                        for action in if node["type"] == "parallel" {
                            lom_core::parallel::actions(node).iter().collect::<Vec<_>>()
                        } else {
                            vec![node]
                        } {
                            if let Some(id) = action["character"].as_str().filter(|s| !s.is_empty())
                            {
                                ids.insert(id.to_owned());
                            }
                        }
                        for id in ids {
                            let actor = &state["actors"][&id];
                            ui.strong(character_display_name(project, &id));
                            if !actor.is_object() {
                                let origin = state_origin(story, target, |n| {
                                    n["type"] == "hide_all"
                                        || n["character"] == id && n["type"] == "hide"
                                });
                                origin_link(
                                    ui,
                                    crate::i18n::key(if origin.is_some() {
                                        "usability.state.hidden"
                                    } else {
                                        "usability.state.not_shown"
                                    }),
                                    origin,
                                    &mut destination,
                                );
                                continue;
                            }
                            ui.horizontal_wrapped(|ui| {
                                origin_link(
                                    ui,
                                    format!(
                                        "{} · {:.0}, {:.0}",
                                        text(actor, "position"),
                                        number(actor, "offset_x", 0.0),
                                        number(actor, "offset_y", 0.0)
                                    ),
                                    state_origin(story, target, |n| {
                                        n["character"] == id
                                            && matches!(text(n, "type"), "show" | "move" | "offset")
                                    }),
                                    &mut destination,
                                );
                                origin_link(
                                    ui,
                                    text(actor, "portrait").to_owned(),
                                    state_origin(story, target, |n| {
                                        n["character"] == id
                                            && matches!(
                                                text(n, "type"),
                                                "show" | "portrait" | "say"
                                            )
                                    }),
                                    &mut destination,
                                );
                                origin_link(
                                    ui,
                                    crate::i18n::key(if actor["dimmed"] == true {
                                        "usability.state.dimmed"
                                    } else {
                                        "usability.state.bright"
                                    }),
                                    state_origin(story, target, |n| {
                                        n["character"] == id
                                            && (n["type"] == "dim"
                                                || n["type"] == "portrait" && n["brighten"] == true)
                                            || n["type"] == "say"
                                    }),
                                    &mut destination,
                                );
                            });
                            let lookup = if id == "player" && actor["appearance"] == "beautified" {
                                "player_beautified"
                            } else {
                                &id
                            };
                            let texture = self.texture(
                                ui.ctx(),
                                project,
                                lookup,
                                true,
                                text(actor, "portrait"),
                            );
                            if texture.is_none() {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    crate::i18n::key(if id.starts_with("user:") {
                                        "usability.state.missing_project_image"
                                    } else {
                                        "usability.state.missing_preview"
                                    }),
                                );
                            } else if let Some((texture, meta)) = texture {
                                let stage =
                                    Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
                                let (x, feet) = self.anchor(text(actor, "position"));
                                let base = Pos2::new(
                                    x * 1920.0 + number(actor, "offset_x", 0.0) as f32,
                                    feet * 1080.0 - number(actor, "offset_y", 0.0) as f32,
                                );
                                let scale = number(&meta, "scale", 100.0) as f32 / 100.0
                                    * number(actor, "layout_scale", 100.0) as f32
                                    / 100.0;
                                let calibration = lom_core::portrait_alignment::layout(
                                    &meta,
                                    text(actor, "portrait"),
                                );
                                let flipped = (text(actor, "facing") == "left")
                                    != (meta["art_facing"].as_str().unwrap_or("left") != "right");
                                let bounds = portrait_rect(
                                    stage,
                                    base,
                                    texture.size_vec2(),
                                    scale,
                                    calibration,
                                    if flipped { -1.0 } else { 1.0 },
                                    &meta,
                                );
                                if !stage.intersects(bounds) {
                                    origin_link(
                                        ui,
                                        crate::i18n::key("usability.state.offscreen"),
                                        state_origin(story, target, |n| {
                                            n["character"] == id
                                                && matches!(
                                                    text(n, "type"),
                                                    "show" | "move" | "offset"
                                                )
                                        }),
                                        &mut destination,
                                    );
                                }
                            }
                            if blackout {
                                ui.small(crate::i18n::key("usability.state.covered"));
                            }
                            ui.add_space(3.0);
                        }
                        let background = state["background"]
                            .as_str()
                            .or_else(|| state["view"].as_str())
                            .unwrap_or("—");
                        origin_link(
                            ui,
                            format!(
                                "{}: {background}",
                                crate::i18n::key("usability.state.background")
                            ),
                            state_origin(story, target, |n| {
                                matches!(text(n, "type"), "scene" | "background" | "combat")
                            }),
                            &mut destination,
                        );
                        for (slot, item) in state["overlays"].as_object().into_iter().flatten() {
                            origin_link(
                                ui,
                                format!("{slot}: {}", text(item, "image")),
                                state_origin(story, target, |n| {
                                    n["type"] == "overlay"
                                        && n["slot"].as_str().unwrap_or("main") == slot
                                }),
                                &mut destination,
                            );
                        }
                        for (kind, item) in
                            state["official_pictures"].as_object().into_iter().flatten()
                        {
                            origin_link(
                                ui,
                                format!("{kind}: {}", text(item, "key")),
                                state_origin(story, target, |n| {
                                    n["type"] == "cg" && n["kind"] == *kind
                                }),
                                &mut destination,
                            );
                        }
                        if state["custom_cg"].is_object() {
                            origin_link(
                                ui,
                                format!("CG: {}", text(&state["custom_cg"], "image")),
                                state_origin(story, target, |n| n["type"] == "custom_cg"),
                                &mut destination,
                            );
                        }
                        for (filter, on) in state["camera_filters"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .filter(|(_, on)| **on == true)
                        {
                            let _ = on;
                            origin_link(
                                ui,
                                format!("{}: {filter}", crate::i18n::key("usability.state.filter")),
                                state_origin(story, target, |n| {
                                    n["type"] == "camera" && n["name"] == *filter
                                }),
                                &mut destination,
                            );
                        }
                    });
            });
        destination
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origins_follow_the_preview_route_and_locate_parallel_children() {
        let story = json!({"start":"enter","nodes":[
            {"id":"enter","type":"show","character":"player","position":"L1","goto":"group"},
            {"id":"unused","type":"offset","character":"player","x":90,"goto":"end"},
            {"id":"group","type":"parallel","actions":[{"type":"wait","seconds":1},{"type":"portrait","character":"player","portrait":"angry1","brighten":true}]},
            {"id":"end","type":"end"}]});
        assert_eq!(
            state_origin(&story, "group", |n| n["type"] == "portrait"),
            Some((2, Some(1)))
        );
        assert_eq!(
            state_origin(&story, "group", |n| n["type"] == "offset"),
            None
        );
        assert_eq!(
            state_origin(&story, "group", |n| n["type"] == "show"),
            Some((0, None))
        );
    }
}
