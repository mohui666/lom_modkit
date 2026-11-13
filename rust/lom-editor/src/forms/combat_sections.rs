//! Keep common duel settings visible and disclose the remaining parameters by task.
use super::*;

const GROUPS: [(&str, &[&str]); 5] = [
    (
        "usability.combat.opponent",
        &[
            "stamina_power",
            "strength",
            "internal",
            "dexterity",
            "talking",
            "defence",
            "sword",
            "fist",
            "martial_weapon",
            "mental",
            "weapon_poison_value",
            "weapon_paralyzed_value",
            "poison_resist",
            "paralyzed_resist",
            "disposition",
            "behaviour",
            "karma",
            "training",
        ],
    ),
    (
        "usability.combat.modifiers",
        &[
            "attack_damage_addition",
            "defence_addition",
            "ultimate_damage_rate",
            "attack_dice_addition",
            "weapon_damage_addition",
            "weapon_dice_addition",
            "weapon_hit_addition",
            "attack_parry_addition",
            "block_dodge_addition",
            "block_parry_addition",
        ],
    ),
    (
        "usability.combat.player",
        &[
            "player_max_health",
            "player_health",
            "player_max_stamina",
            "player_stamina",
            "player_stamina_power",
            "player_strength",
            "player_internal",
            "player_dexterity",
            "player_talking",
            "player_defence",
            "player_sword",
            "player_fist",
            "player_martial_weapon",
            "player_mental",
            "player_poison_resist",
            "player_paralyzed_resist",
            "player_disposition",
            "player_behaviour",
            "player_karma",
            "player_training",
        ],
    ),
    (
        "usability.combat.skills",
        &[
            "talents",
            "player_talents",
            "player_supports",
            "enemy_supports",
            "player_skills",
            "enemy_skills",
        ],
    ),
    (
        "usability.combat.behavior",
        &[
            "talk_rate",
            "attack_rate",
            "weapon_rate",
            "ultimate_rate",
            "block_rate",
        ],
    ),
];

pub(super) fn show(
    catalog: &Catalog,
    ui: &mut Ui,
    node: &mut Value,
    fields: &[Value],
    node_ids: &[String],
    story_ids: &[String],
) -> bool {
    let mut changed = false;
    let draw = |ui: &mut Ui, node: &mut Value, field: &Value| {
        catalog.field(
            ui,
            node,
            field[0].as_str().unwrap(),
            field[1].as_str().unwrap(),
            field[2].as_str().unwrap(),
            field[3].as_bool().unwrap_or(false),
            node_ids,
            story_ids,
        )
    };
    ui.strong(crate::i18n::key("usability.combat.common"));
    for field in fields.iter().filter(|field| {
        !GROUPS
            .iter()
            .any(|(_, keys)| keys.contains(&field[0].as_str().unwrap()))
    }) {
        changed |= draw(ui, node, field);
    }
    let problem = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("problem-field")));
    for (title, keys) in GROUPS {
        let count = keys.iter().filter(|key| node.get(**key).is_some()).count();
        let label = if count == 0 {
            crate::i18n::key(title)
        } else {
            crate::i18n::message(
                "usability.combat.configured",
                &[
                    ("group", crate::i18n::key(title)),
                    ("count", count.to_string()),
                ],
            )
        };
        egui::CollapsingHeader::new(label)
            .id_salt(("combat-section", node["id"].as_str().unwrap_or(""), title))
            .open(
                problem
                    .as_deref()
                    .filter(|key| keys.contains(key))
                    .map(|_| true),
            )
            .show(ui, |ui| {
                for field in fields
                    .iter()
                    .filter(|field| keys.contains(&field[0].as_str().unwrap()))
                {
                    changed |= draw(ui, node, field);
                }
            });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_combat_groups_keep_saved_values_and_reveal_the_problem_field() {
        let catalog = Catalog::new();
        let mut node = catalog.new_node("combat", "duel".into());
        node["player_health"] = json!(456);
        node["weapon_poison_value"] = json!(27);
        let original = node.clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |node: &mut Value| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(850.0, 2400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(!catalog.node_form(ui, node, &[], &[]).changed);
                    });
                },
            )
        };
        let labels = |output: egui::FullOutput| {
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .into_iter()
                .filter_map(|(_, node)| node.label().or(node.value()).map(str::to_owned))
                .collect::<Vec<_>>()
        };
        let output = labels(draw(&mut node));
        assert!(output.iter().any(|text| text.starts_with("对手最大血量")));
        assert!(!output.iter().any(|text| text.starts_with("赵活初始血量")));
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("problem-field"), "player_health".to_owned())
        });
        draw(&mut node);
        let output = labels(draw(&mut node));
        assert!(output.iter().any(|text| text.starts_with("赵活初始血量")));
        assert_eq!(node, original);
    }

    #[test]
    fn stopping_an_old_effect_hides_playback_fields_without_losing_them() {
        let catalog = Catalog::new();
        let mut node = json!({"id":"stop","type":"effect","name":"Hit_001","d":0,"x":17.5,"y":4.0,
            "a":2.5,"b":-3,"rotation":40,"speed":0.5,"tag":"loop","clear":true,"wait_after":0.2});
        let original = node.clone();
        assert_eq!(
            field_requirement(&node, "name", false),
            Requirement::Optional
        );
        let ctx = egui::Context::default();
        for playing in [false, true, false] {
            if node.get("play").is_some() || playing {
                node["play"] = json!(playing);
            }
            let before = node.clone();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!catalog.node_form(ui, &mut node, &[], &[]).changed);
                });
            });
            assert_eq!(node, before);
            assert_eq!(field_visible("effect", &node, "x"), playing);
            assert!(field_visible("effect", &node, "tag"));
        }
        node.as_object_mut().unwrap().remove("play");
        assert_eq!(node, original);
    }

    #[test]
    fn default_actions_fit_a_narrow_form_with_large_button_text() {
        let catalog = Catalog::new();
        let mut node = json!({"id":"stop","type":"effect","play":false,"tag":"loop","clear":true,"wait_after":0.2});
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.style_mut(|style| {
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(18.0));
        });
        let draw = |node: &mut Value| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(340.0, 1500.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        catalog.node_form(ui, node, &[], &[]);
                    });
                },
            )
        };
        draw(&mut node);
        let output = draw(&mut node);
        let label = crate::i18n::key("editor.defaults.set");
        let buttons: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter(|(_, node)| node.label().is_some_and(|text| text.starts_with(&label)))
            .collect();
        assert_eq!(buttons.len(), 4);
        for (_, button) in buttons {
            assert!(
                button.bounds().unwrap().x1 <= 332.0,
                "default action must remain inside the field: {:?}",
                button.bounds()
            );
        }
    }
}
