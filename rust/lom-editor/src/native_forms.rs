//! Localized, purpose-oriented controls for imported vanilla commands.
use eframe::egui::{self, Ui};
use serde_json::{json, Value};
use std::sync::OnceLock;

fn key(id: &str) -> String {
    crate::i18n::key(id)
}
fn message(id: &str, values: &[(&str, String)]) -> String {
    crate::i18n::message(id, values)
}
fn registry() -> &'static Value {
    static REGISTRY: OnceLock<Value> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        serde_json::from_str(include_str!("../data/native-api-labels.json")).unwrap()
    })
}
fn raw_name(value: &Value) -> String {
    match value["kind"].as_str().unwrap_or("") {
        "name" | "local" => value["name"].as_str().unwrap_or("").into(),
        "member" => format!(
            "{}.{}",
            raw_name(&value["object"]),
            value["field"].as_str().unwrap_or("")
        ),
        _ => String::new(),
    }
}
fn raw_expression(value: &Value) -> String {
    if matches!(value["kind"].as_str(), Some("name" | "local" | "member")) {
        let raw = raw_name(value);
        if !raw.starts_with('.') {
            return raw;
        }
    }
    lom_core::native_flow::expression(value).unwrap_or_else(|_| value.to_string())
}
fn call_name(value: &Value) -> String {
    raw_name(&value["function"])
}
fn argument_spec(function: &str, index: usize) -> &'static Value {
    &registry()[function]["args"][index]
}
fn argument_label(function: &str, index: usize) -> String {
    argument_spec(function, index)["label"]
        .as_str()
        .map(key)
        .unwrap_or_else(|| {
            message(
                "native.form.parameter",
                &[("number", (index + 1).to_string())],
            )
        })
}
fn call_title(function: &Value) -> String {
    let name = raw_name(function);
    registry()[&name]["title"]
        .as_str()
        .map(key)
        .unwrap_or_else(|| message("native.summary.unknown", &[("name", name)]))
}
fn operation_call_title(expression: &Value) -> String {
    let method = call_name(expression);
    if method == "runwait" && expression["args"][0]["kind"] == "call" {
        return operation_call_title(&expression["args"][0]);
    }
    if method == "runblock" {
        if let Some(block) = expression["args"][1]["value"].as_str() {
            let id = format!("native.block.{block}");
            let translated = key(&id);
            if translated != id {
                return translated;
            }
        }
    }
    call_title(&expression["function"])
}

fn flow_variable(target: &Value) -> Option<&str> {
    if target["kind"] != "member"
        || target["field"] != "value"
        || target["object"]["kind"] != "call"
        || call_name(&target["object"]) != "getvar"
    {
        return None;
    }
    target["object"]["args"][1]["value"].as_str()
}
fn assignment_setting(target: &Value) -> Option<(&'static str, &'static str)> {
    Some(match raw_name(target).as_str() {
        "sayoptions.waitforinput" => ("native.assignment.wait_input", "toggle"),
        "sayoptions.fadewhendone" => ("native.assignment.hide_dialog", "toggle"),
        _ => match flow_variable(target)? {
            "ViewName" => ("native.assignment.background", "views"),
            "ShockPosition" => ("native.assignment.shock_target", "characters"),
            "ShockDuration" => ("native.assignment.shock_duration", ""),
            "ShakeDuration" => ("native.assignment.shake_duration", ""),
            "FlashName" => ("native.assignment.flash_color", "flash_color"),
            "FlashDuration" => ("native.assignment.flash_duration", ""),
            _ => return None,
        },
    })
}
fn assignment_value_label(target: &Value) -> String {
    key(match raw_name(target).as_str() {
        "sayoptions.waitforinput" => "native.field.wait_input",
        "sayoptions.fadewhendone" => "native.field.hide_dialog",
        _ => match flow_variable(target) {
            Some("ViewName") => "native.parameter.view",
            Some("ShockPosition") => "native.field.shock_target",
            Some("FlashName") => "native.field.flash_color",
            Some("ShockDuration" | "ShakeDuration" | "FlashDuration") => {
                "native.parameter.duration"
            }
            _ => "native.form.value",
        },
    })
}
fn operation_title(operation: &Value) -> String {
    match operation["kind"].as_str().unwrap_or("") {
        "call" => operation_call_title(&operation["expression"]),
        "assign" => {
            let target = &operation["target"];
            if let Some((title, _)) = assignment_setting(target) {
                return key(title);
            }
            if target["kind"] == "index" {
                return message(
                    "native.assignment.option",
                    &[("number", expression_summary(&target["index"]))],
                );
            }
            if operation["value"]["kind"] == "call" {
                return operation_call_title(&operation["value"]);
            }
            if raw_name(&operation["value"]) == "dicemenudialog.ResultSelection" {
                return key("native.assignment.dice_result");
            }
            if operation["value"]["kind"] == "table" {
                return key("native.assignment.prepare_options");
            }
            key("native.assignment.store_value")
        }
        "return" => key("native.summary.return"),
        other => message("native.summary.unknown", &[("name", other.into())]),
    }
}
pub fn summary(node: &Value) -> String {
    match node["type"].as_str().unwrap_or("") {
        "native_branch" => {
            let descriptor = &node["_editor"]["native_condition"];
            match descriptor["kind"].as_str() {
                Some("choice") => {
                    let labels = descriptor["labels"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" / ");
                    message(
                        "native.condition.choice",
                        &[
                            ("number", descriptor["index"].as_str().unwrap_or("").into()),
                            ("text", labels),
                        ],
                    )
                }
                Some("check") => message(
                    "native.condition.check",
                    &[
                        ("name", descriptor["key"].as_str().unwrap_or("").into()),
                        ("result", descriptor["result"].as_str().unwrap_or("").into()),
                    ],
                ),
                _ => expression_summary(&node["condition"]),
            }
        }
        "native_action" => {
            let Some(operations) = node["operations"].as_array() else {
                return key("native.summary.missing");
            };
            let Some(first) = operations.first() else {
                return key("native.summary.merge");
            };
            if let Some(decision) = operations.iter().find_map(decision_call) {
                return decision_summary(decision, operations);
            }
            let title = operation_title(first);
            if operations.len() > 1 {
                message(
                    "native.summary.count",
                    &[("title", title), ("count", operations.len().to_string())],
                )
            } else {
                title
            }
        }
        _ => String::new(),
    }
}

fn decision_call(operation: &Value) -> Option<&Value> {
    let expression = match operation["kind"].as_str()? {
        "call" => &operation["expression"],
        "assign" => &operation["value"],
        _ => return None,
    };
    [
        "choose",
        "checkpointmanager.Switch",
        "checkpointmanager.Condition",
        "checkpointmanager.Dice",
        "checkpointmanager.Position",
    ]
    .contains(&call_name(expression).as_str())
    .then_some(expression)
}

fn same_binding(left: &Value, right: &Value) -> bool {
    if left["kind"] == "local" && right["kind"] == "local" {
        return left["symbol"].is_string() && left["symbol"] == right["symbol"];
    }
    left["kind"] == "name" && right["kind"] == "name" && left["name"] == right["name"]
}

fn decision_summary(decision: &Value, operations: &[Value]) -> String {
    let method = call_name(decision);
    if method == "choose" {
        let binding = &decision["args"][0];
        let mut slots = std::collections::BTreeSet::new();
        for operation in operations.iter().filter(|op| op["kind"] == "assign") {
            if same_binding(&operation["target"], binding) && operation["value"]["kind"] == "table"
            {
                for (index, entry) in operation["value"]["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    slots.insert(if entry["key"].is_null() {
                        (index + 1).to_string()
                    } else {
                        entry["key"].to_string()
                    });
                }
            }
            if operation["target"]["kind"] == "index"
                && same_binding(&operation["target"]["object"], binding)
            {
                slots.insert(operation["target"]["index"].to_string());
            }
        }
        if !slots.is_empty() {
            return message(
                "native.summary.choice_count",
                &[("count", slots.len().to_string())],
            );
        }
    }
    let title = operation_call_title(decision);
    decision["args"][0]["value"]
        .as_str()
        .map(|name| {
            message(
                "native.summary.decision",
                &[("title", title.clone()), ("name", name.to_owned())],
            )
        })
        .unwrap_or(title)
}

pub fn show(ui: &mut Ui, node: &mut Value) -> bool {
    match node["type"].as_str().unwrap_or("") {
        "native_branch" => edit_field(ui, &key("native.form.condition"), node, "condition", 0),
        "native_action" => {
            let Some(operations) = node.get_mut("operations").and_then(Value::as_array_mut) else {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    key("native.form.missing_operations"),
                );
                return false;
            };
            if operations.is_empty() {
                ui.label(key("native.form.merge"));
                return false;
            }
            let expanded = operations.len() <= 4;
            let mut changed = false;
            for (index, operation) in operations.iter_mut().enumerate() {
                let title = operation_title(operation);
                ui.push_id(index, |ui| {
                    changed |= egui::CollapsingHeader::new(format!("{} · {title}", index + 1))
                        .default_open(expanded)
                        .show(ui, |ui| {
                            let changed = match operation["kind"].as_str().unwrap_or("") {
                                "call" => edit_call(ui, &mut operation["expression"], 0),
                                "assign" => edit_assignment(ui, operation),
                                "return" => edit_field(
                                    ui,
                                    &key("native.form.return_value"),
                                    operation,
                                    "value",
                                    0,
                                ),
                                _ => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        key("native.form.unknown_operation"),
                                    );
                                    false
                                }
                            };
                            egui::CollapsingHeader::new(key("native.form.technical")).show(
                                ui,
                                |ui| {
                                    if let Some(target) = operation.get("target") {
                                        ui.small(key("native.form.original_target"));
                                        ui.monospace(raw_expression(target));
                                    }
                                    if let Some(expr) = operation.get("expression") {
                                        ui.small(key("native.form.original_call"));
                                        ui.monospace(raw_expression(expr));
                                    }
                                },
                            );
                            changed
                        })
                        .body_returned
                        .unwrap_or(false);
                });
            }
            changed
        }
        _ => false,
    }
}
fn edit_assignment(ui: &mut Ui, operation: &mut Value) -> bool {
    let target = operation["target"].clone();
    if let Some((_, resource)) = assignment_setting(&target) {
        let label = assignment_value_label(&target);
        let Some(value) = operation.get_mut("value") else {
            return missing(ui, &label);
        };
        return edit_parameter(ui, &label, value, resource, None, 0);
    }
    if operation["value"]["kind"] == "call" {
        let changed = edit_call(ui, &mut operation["value"], 0);
        ui.small(key("native.form.stored_result"));
        return changed;
    }
    edit_field(ui, &key("native.form.value"), operation, "value", 0)
}
fn missing(ui: &mut Ui, label: &str) -> bool {
    ui.colored_label(
        ui.visuals().error_fg_color,
        message("native.form.missing_field", &[("label", label.into())]),
    );
    false
}
fn edit_field(ui: &mut Ui, label: &str, parent: &mut Value, field: &str, depth: usize) -> bool {
    if let Some(value) = parent.get_mut(field) {
        edit_expression(ui, label, value, depth)
    } else {
        missing(ui, label)
    }
}
fn edit_expression(ui: &mut Ui, label: &str, value: &mut Value, depth: usize) -> bool {
    ui.push_id(label, |ui| {
        let composite = matches!(
            value["kind"].as_str(),
            Some("unary" | "binary" | "call" | "table")
        );
        if composite && depth > 1 {
            egui::CollapsingHeader::new(format!("{label} · {}", expression_summary(value)))
                .default_open(false)
                .show(ui, |ui| expression_fields(ui, value, depth))
                .body_returned
                .unwrap_or(false)
        } else {
            ui.strong(label);
            expression_fields(ui, value, depth)
        }
    })
    .inner
}
fn expression_fields(ui: &mut Ui, expression: &mut Value, depth: usize) -> bool {
    match expression["kind"].as_str().unwrap_or("") {
        "literal" => edit_literal(ui, expression),
        "name" | "local" | "member" | "index" => {
            ui.label(expression_summary(expression))
                .on_hover_text(raw_expression(expression));
            false
        }
        "unary" => {
            let mut changed = operator(ui, expression, &["not", "-", "#"]);
            changed |= edit_field(
                ui,
                &key("native.form.operand"),
                expression,
                "value",
                depth + 1,
            );
            changed
        }
        "binary" => {
            let mut changed = operator(
                ui,
                expression,
                &[
                    "==", "~=", ">", ">=", "<", "<=", "and", "or", "+", "-", "*", "/", "%", "^",
                    "..",
                ],
            );
            changed |= edit_field(ui, &key("native.form.left"), expression, "left", depth + 1);
            changed |= edit_field(
                ui,
                &key("native.form.right"),
                expression,
                "right",
                depth + 1,
            );
            changed
        }
        "call" => edit_call(ui, expression, depth),
        "table" => edit_table(ui, expression, depth),
        other => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                message("native.form.unknown_expression", &[("kind", other.into())]),
            );
            false
        }
    }
}
fn edit_literal(ui: &mut Ui, expression: &mut Value) -> bool {
    let display = expression["display"].as_str().map(str::to_owned);
    let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    match expression.get_mut("value") {
        Some(Value::String(value)) => {
            let mut text = display.unwrap_or_else(|| value.clone());
            let changed = if text.contains('\n') || text.chars().count() > 70 {
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            } else {
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY))
                    .changed()
            };
            if changed {
                *value = text;
                expression.as_object_mut().unwrap().remove("display");
            }
            changed
        }
        Some(Value::Bool(value)) => ui.checkbox(value, key("native.form.on")).changed(),
        Some(Value::Number(number)) => {
            if let Some(mut value) = number.as_i64() {
                if ui
                    .add_sized(size, egui::DragValue::new(&mut value))
                    .changed()
                {
                    *number = value.into();
                    return true;
                }
            } else if let Some(mut value) = number.as_u64() {
                if ui
                    .add_sized(size, egui::DragValue::new(&mut value))
                    .changed()
                {
                    *number = value.into();
                    return true;
                }
            } else if let Some(mut value) = number.as_f64() {
                if ui
                    .add_sized(size, egui::DragValue::new(&mut value))
                    .changed()
                {
                    if let Some(updated) = serde_json::Number::from_f64(value) {
                        *number = updated;
                        return true;
                    }
                }
            }
            false
        }
        Some(Value::Null) => {
            ui.label(key("native.form.nil"));
            false
        }
        _ => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                key("native.form.invalid_literal"),
            );
            false
        }
    }
}
fn edit_call(ui: &mut Ui, expression: &mut Value, depth: usize) -> bool {
    let function = call_name(expression);
    if function == "runwait" && expression["args"][0]["kind"] == "call" {
        let changed = edit_call(ui, &mut expression["args"][0], depth);
        ui.small(key("native.form.wait_completion"));
        return changed;
    }
    let owner = character_id(&expression["args"][0]).map(str::to_owned);
    let Some(arguments) = expression.get_mut("args").and_then(Value::as_array_mut) else {
        ui.colored_label(ui.visuals().error_fg_color, key("native.form.missing_args"));
        return false;
    };
    if arguments.is_empty() {
        ui.small(key("native.form.no_parameters"));
    }
    let mut changed = false;
    for (index, argument) in arguments.iter_mut().enumerate() {
        let label = argument_label(&function, index);
        let resource = argument_spec(&function, index)["resource"]
            .as_str()
            .unwrap_or("");
        changed |= edit_parameter(ui, &label, argument, resource, owner.as_deref(), depth + 1);
    }
    changed
}
fn table_field(field: &str) -> Option<(&'static str, &'static str)> {
    Some(match field {
        "character" => ("native.parameter.character", "characters"),
        "portrait" => ("native.parameter.portrait", "portrait"),
        "fromPosition" => ("native.field.from_position", "positions"),
        "toPosition" => ("native.field.to_position", "positions"),
        "facing" => ("native.field.facing", "facing"),
        "fadeDuration" => ("native.parameter.fade", ""),
        "moveDuration" => ("native.field.move_duration", ""),
        "useDefaultSettings" => ("native.field.default_settings", "toggle"),
        _ => return None,
    })
}
fn edit_table(ui: &mut Ui, expression: &mut Value, depth: usize) -> bool {
    let owner = expression["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["key"]["value"] == "character")
        .and_then(|entry| character_id(&entry["value"]))
        .map(str::to_owned);
    let Some(entries) = expression.get_mut("entries").and_then(Value::as_array_mut) else {
        ui.colored_label(
            ui.visuals().error_fg_color,
            key("native.form.table_missing"),
        );
        return false;
    };
    if entries.is_empty() {
        ui.small(key("native.form.empty_table"));
    }
    let mut changed = false;
    for (index, entry) in entries.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.separator();
            if let Some((label, resource)) = entry["key"]["value"].as_str().and_then(table_field) {
                if let Some(value) = entry.get_mut("value") {
                    changed |= edit_parameter(
                        ui,
                        &key(label),
                        value,
                        resource,
                        owner.as_deref(),
                        depth + 1,
                    );
                }
            } else {
                ui.small(message(
                    "native.form.entry",
                    &[("number", (index + 1).to_string())],
                ));
                if !entry["key"].is_null() {
                    changed |=
                        edit_field(ui, &key("native.form.entry_key"), entry, "key", depth + 1);
                }
                changed |= edit_field(
                    ui,
                    &key("native.form.entry_content"),
                    entry,
                    "value",
                    depth + 1,
                );
            }
        });
    }
    changed
}
fn character_id(expression: &Value) -> Option<&str> {
    if expression["kind"] == "call" && call_name(expression) == "characters.Get" {
        return expression["args"][0]["value"].as_str();
    }
    if expression["kind"] == "member" {
        return character_id(&expression["object"]);
    }
    expression["value"].as_str()
}
fn character_value(expression: &mut Value) -> Option<&mut Value> {
    if expression["kind"] == "call" && call_name(expression) == "characters.Get" {
        return expression.get_mut("args")?.as_array_mut()?.get_mut(0);
    }
    if expression["kind"] == "member" {
        return character_value(expression.get_mut("object")?);
    }
    None
}
fn edit_parameter(
    ui: &mut Ui,
    label: &str,
    expression: &mut Value,
    resource: &str,
    owner: Option<&str>,
    depth: usize,
) -> bool {
    if resource == "characters" {
        if let Some(value) = character_value(expression) {
            return edit_parameter(ui, label, value, resource, owner, depth);
        }
    }
    if resource == "toggle" && expression["kind"] == "literal" {
        if let Some(value) = expression.get("value").and_then(Value::as_bool) {
            let mut selected = value;
            if ui.checkbox(&mut selected, label).changed() {
                expression["value"] = json!(selected);
                return true;
            }
            return false;
        }
        if let Some(value) = expression["value"].as_i64().filter(|n| [0, 1].contains(n)) {
            let mut selected = value == 1;
            if ui.checkbox(&mut selected, label).changed() {
                expression["value"] = json!(if selected { 1 } else { 0 });
                return true;
            }
            return false;
        }
    }
    if !resource.is_empty() && expression["kind"] == "literal" {
        let options = resource_options(resource, owner);
        if !options.is_empty() {
            ui.strong(label);
            return resource_picker(ui, label, expression, &options);
        }
    }
    if let Some(options) = reference_options(resource) {
        if options
            .iter()
            .any(|(expr, _)| raw_name(expr) == raw_name(expression))
        {
            ui.strong(label);
            let current = options
                .iter()
                .find(|(expr, _)| raw_name(expr) == raw_name(expression))
                .unwrap()
                .1
                .clone();
            let mut selected = None;
            crate::forms::resource_combo(ui, label, &current, |ui| {
                for (expr, caption) in &options {
                    if ui
                        .selectable_label(raw_name(expr) == raw_name(expression), caption)
                        .clicked()
                    {
                        selected = Some(expr.clone());
                        ui.close();
                    }
                }
            });
            if let Some(selected) = selected {
                *expression = selected;
                return true;
            }
            return false;
        }
    }
    edit_expression(ui, label, expression, depth)
}
fn resource_picker(
    ui: &mut Ui,
    label: &str,
    expression: &mut Value,
    options: &[(Value, String)],
) -> bool {
    let current = expression["value"].clone();
    let caption = options
        .iter()
        .find(|(id, _)| id == &current)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| {
            message(
                "native.form.unlisted_resource",
                &[(
                    "id",
                    current
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| current.to_string()),
                )],
            )
        });
    let query_id = ui.id().with((label, "native-resource-search"));
    let mut query = ui
        .ctx()
        .data(|data| data.get_temp::<String>(query_id))
        .unwrap_or_default();
    let mut selected = None;
    crate::forms::resource_combo(ui, label, &caption, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut query)
                .desired_width(f32::INFINITY)
                .hint_text(key("native.form.search")),
        );
        let query = query.to_lowercase();
        for (id, name) in options {
            if !query.is_empty() && !format!("{name} {id}").to_lowercase().contains(&query) {
                continue;
            }
            if ui
                .selectable_label(id == &current, name)
                .on_hover_text(id.to_string())
                .clicked()
            {
                selected = Some(id.clone());
                ui.close();
            }
        }
    })
    .response
    .on_hover_text(current.to_string());
    ui.ctx().data_mut(|data| data.insert_temp(query_id, query));
    if let Some(selected) = selected {
        expression["value"] = selected;
        expression.as_object_mut().unwrap().remove("display");
        return true;
    }
    if let Some(id) = current.as_str() {
        let mut manual = id.to_owned();
        let changed = egui::CollapsingHeader::new(key("native.form.manual_id"))
            .show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut manual).desired_width(f32::INFINITY))
                    .changed()
            })
            .body_returned
            .unwrap_or(false);
        if changed {
            expression["value"] = json!(manual);
            expression.as_object_mut().unwrap().remove("display");
            return true;
        }
    }
    false
}
fn catalog_label(category: &str, id: &str, fallback: &str) -> String {
    let value = crate::i18n::term(category, id, fallback);
    if crate::i18n::locale() == "chs" {
        crate::i18n::simplified_label(&value)
    } else {
        value
    }
}
fn resource_options(resource: &str, owner: Option<&str>) -> Vec<(Value, String)> {
    if resource == "block" {
        return [
            "out",
            "view",
            "out_white",
            "view_white",
            "out_2",
            "shock",
            "vshock",
            "shake",
            "flash",
        ]
        .iter()
        .map(|id| (json!(id), key(&format!("native.block.{id}"))))
        .collect();
    }
    if resource == "filter" {
        return [
            "stage-memory",
            "stage-dream",
            "stage-fire",
            "stage-blurdim",
            "stage-bloody",
            "stage-blur",
            "stage-sepia",
            "stage-gray-10",
            "stage-gray-30",
            "stage-gray-50",
            "stage-gray-70",
            "stage-gray-100",
        ]
        .iter()
        .map(|id| {
            (
                json!(id),
                key(&format!(
                    "native.filter.{}",
                    id.strip_prefix("stage-").unwrap()
                )),
            )
        })
        .collect();
    }
    if resource == "item" {
        let kind = match owner {
            Some("book") => "items_book",
            Some("misc") => "items_misc",
            Some("special") => "items_special",
            _ => return Vec::new(),
        };
        return resource_options(kind, None);
    }
    let values: &[(&str, &str)] = match resource {
        "scene" => &[
            ("Free", "free"),
            ("Title", "title"),
            ("Story", "story"),
            ("Combat", "combat"),
            ("Battle", "battle"),
            ("GameOver", "gameover"),
            ("End", "ending"),
            ("DemoEnd", "demoend"),
        ],
        "facing" => &[("left", "left"), ("right", "right")],
        "direction" => &[("lr", "lr"), ("rl", "rl"), ("tb", "tb"), ("bt", "bt")],
        "flash_color" => &[("white", "white"), ("red", "red"), ("black", "black")],
        "calculation" => &[("", "default"), ("center", "center"), ("mental", "mental")],
        "item_type" => &[("book", "book"), ("misc", "misc"), ("special", "special")],
        "item_change" => &[("add", "add"), ("remove", "remove")],
        _ => &[],
    };
    if !values.is_empty() {
        return values
            .iter()
            .map(|(id, label)| (json!(id), key(&format!("native.enum.{label}"))))
            .collect();
    }
    if resource == "picture_color" {
        return [(0, "white"), (1, "black"), (2, "transparent")]
            .iter()
            .map(|(id, label)| (json!(id), key(&format!("native.enum.{label}"))))
            .collect();
    }
    if resource == "dayenv" {
        return [(1, "day"), (2, "night")]
            .iter()
            .map(|(id, label)| (json!(id), key(&format!("native.enum.{label}"))))
            .collect();
    }
    let data = lom_core::validate::editor_data();
    if resource == "portrait" {
        let Some(character) = data["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["id"].as_str() == owner)
        else {
            return Vec::new();
        };
        return character["portraits"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|id| (json!(id), catalog_label("portraits", id, id)))
            .collect();
    }
    let mut rows: Vec<(String, String)> = data[resource]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            if let Some(id) = row.as_str() {
                return Some((id.into(), catalog_label(resource, id, id)));
            }
            let id = row["id"].as_str()?;
            let name = row["name"].as_str().unwrap_or(id);
            Some((id.into(), catalog_label(resource, id, name)))
        })
        .collect();
    if resource == "views" {
        for (id, label) in [
            ("black", "native.name.black_view"),
            ("white", "native.name.white_view"),
            ("red", "native.name.red_view"),
        ] {
            if let Some((_, caption)) = rows.iter_mut().find(|(value, _)| value == id) {
                *caption = key(label);
            } else {
                rows.push((id.into(), key(label)));
            }
        }
    }
    crate::forms::number_duplicate_labels(&mut rows);
    rows.into_iter()
        .map(|(id, name)| (json!(id), name))
        .collect()
}
fn reference_options(resource: &str) -> Option<Vec<(Value, String)>> {
    let (object, rows): (&str, Vec<(&str, String)>) = match resource {
        "say_dialog" => (
            "saydialogs",
            vec![
                ("character", key("native.name.character_dialog")),
                ("think", key("native.name.think_dialog")),
                ("narrative", key("native.name.narrative_dialog")),
                ("center", key("native.name.center_dialog")),
                ("center2", key("native.name.center2_dialog")),
            ],
        ),
        "flowchart" => (
            "flowcharts",
            vec![
                ("view", key("native.name.view_flow")),
                ("common", key("native.name.common_flow")),
            ],
        ),
        "menu_dialog" => (
            "menudialogs",
            lom_core::validate::editor_data()["menu_dialogs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| row.as_str().or_else(|| row["id"].as_str()))
                .map(|id| (id, menu_label(id)))
                .collect(),
        ),
        _ => return None,
    };
    Some(
        rows.into_iter()
            .map(|(field, label)| {
                (
                    json!({"kind":"member","object":{"kind":"name","name":object},"field":field}),
                    label,
                )
            })
            .collect(),
    )
}
fn menu_label(id: &str) -> String {
    let key_id = if id == "Dice" {
        "native.name.dice_menu".into()
    } else {
        format!("native.menu.{}", id.to_lowercase())
    };
    let translated = key(&key_id);
    if translated == key_id {
        message("native.name.unknown", &[("name", id.into())])
    } else {
        translated
    }
}
fn operator_key(operator: &str) -> &'static str {
    match operator {
        "==" => "eq",
        "~=" => "ne",
        ">" => "gt",
        ">=" => "ge",
        "<" => "lt",
        "<=" => "le",
        "and" => "and",
        "or" => "or",
        "+" => "add",
        "-" => "subtract",
        "*" => "multiply",
        "/" => "divide",
        "%" => "modulo",
        "^" => "power",
        ".." => "concat",
        "not" => "not",
        "#" => "length",
        _ => "eq",
    }
}
fn operator_label(operator: &str) -> String {
    key(&format!("native.operator.{}", operator_key(operator)))
}
fn operator(ui: &mut Ui, expression: &mut Value, options: &[&str]) -> bool {
    let current = expression["op"].as_str().unwrap_or("").to_owned();
    let mut selected = current.clone();
    egui::ComboBox::from_id_salt("operator")
        .selected_text(operator_label(&current))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for op in options {
                ui.selectable_value(&mut selected, (*op).into(), operator_label(op));
            }
        });
    if selected != current {
        expression["op"] = json!(selected);
        true
    } else {
        false
    }
}
fn variable_label(name: &str) -> String {
    let id = if name == "choice" || name.starts_with("choice_") {
        Some("native.name.choice_result")
    } else if name.starts_with("random_") {
        Some("native.name.random")
    } else if name.starts_with("switch_") && name.ends_with("_result") {
        Some("native.name.check_result")
    } else if name.ends_with("_result") {
        Some("native.name.dice_result")
    } else if name.starts_with("dice") {
        Some("native.name.dice_options")
    } else if name.starts_with("option") {
        Some("native.name.options")
    } else if name == "narrative" {
        Some("native.name.narrative")
    } else {
        None
    };
    if let Some(id) = id {
        return key(id);
    }
    if let Some(number) = name
        .strip_prefix("var")
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        return message("native.name.variable", &[("number", number.into())]);
    }
    message("native.name.unknown", &[("name", name.into())])
}
fn expression_summary(expression: &Value) -> String {
    match expression["kind"].as_str().unwrap_or("") {
        "name" => variable_label(expression["name"].as_str().unwrap_or("")),
        "local" => {
            let name = expression["name"].as_str().unwrap_or("");
            let label = variable_label(name);
            if label == message("native.name.unknown", &[("name", name.into())]) {
                key("native.name.intermediate")
            } else {
                label
            }
        }
        "member" => {
            let raw = raw_name(expression);
            let fixed = match raw.as_str() {
                "saydialogs.character" => Some("native.name.character_dialog"),
                "saydialogs.think" => Some("native.name.think_dialog"),
                "saydialogs.narrative" => Some("native.name.narrative_dialog"),
                "saydialogs.center" => Some("native.name.center_dialog"),
                "saydialogs.center2" => Some("native.name.center2_dialog"),
                "flowcharts.view" => Some("native.name.view_flow"),
                "flowcharts.common" => Some("native.name.common_flow"),
                "dicemenudialog.ResultSelection" => Some("native.assignment.dice_result"),
                _ => None,
            };
            if let Some(id) = fixed {
                return key(id);
            }
            if let Some(menu) = raw.strip_prefix("menudialogs.") {
                return menu_label(menu);
            }
            if let Some((title, _)) = assignment_setting(expression) {
                return key(title);
            }
            match expression["field"].as_str().unwrap_or("") {
                "ResultCount" => key("native.name.result_count"),
                "Result" => key("native.name.result"),
                "Header" => key("native.name.header"),
                "Additions" => key("native.name.additions"),
                "State" | "holder" | "gameObject" => {
                    let label = key("native.name.holder");
                    character_id(expression)
                        .map(|id| format!("{label} · {}", catalog_label("characters", id, id)))
                        .unwrap_or(label)
                }
                _ => message(
                    "native.name.unknown",
                    &[("name", raw_expression(expression))],
                ),
            }
        }
        "index" => message(
            "native.assignment.option",
            &[("number", expression_summary(&expression["index"]))],
        ),
        "literal" => match &expression["value"] {
            Value::String(value) => expression["display"]
                .as_str()
                .unwrap_or(value)
                .chars()
                .take(70)
                .collect(),
            Value::Bool(value) => key(if *value {
                "native.enum.true"
            } else {
                "native.enum.false"
            }),
            Value::Null => key("native.form.nil"),
            other => other.to_string(),
        },
        "call" => {
            let method = call_name(expression);
            let title = operation_call_title(expression);
            let Some(first) = expression["args"].as_array().and_then(|args| args.first()) else {
                return title;
            };
            let resource = argument_spec(&method, 0)["resource"].as_str().unwrap_or("");
            let value = resource_options(resource, None)
                .into_iter()
                .find(|(id, _)| id == &first["value"])
                .map(|(_, name)| name)
                .unwrap_or_else(|| expression_summary(first));
            format!("{title} · {value}")
        }
        "table" => message(
            "native.name.list_count",
            &[(
                "count",
                expression["entries"]
                    .as_array()
                    .map_or(0, Vec::len)
                    .to_string(),
            )],
        ),
        "binary" => format!(
            "{} {} {}",
            expression_summary(&expression["left"]),
            operator_label(expression["op"].as_str().unwrap_or("")),
            expression_summary(&expression["right"])
        ),
        "unary" => format!(
            "{} {}",
            operator_label(expression["op"].as_str().unwrap_or("")),
            expression_summary(&expression["value"])
        ),
        other => message("native.form.unknown_expression", &[("kind", other.into())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_corpus_command_parameter_and_assignment_has_semantic_labels() {
        let inventory: Value =
            serde_json::from_str(include_str!("../data/native-api-inventory.json")).unwrap();
        let translations: Value =
            serde_json::from_str(include_str!("../data/i18n/native.json")).unwrap();
        assert_eq!(inventory["scripts"], 2200);
        assert_eq!(inventory["calls"].as_object().unwrap().len(), 158);
        for (method, arity) in inventory["calls"].as_object().unwrap() {
            let metadata = &registry()[method];
            assert!(metadata.is_object(), "Missing command {method}");
            let title = metadata["title"].as_str().unwrap();
            assert!(translations[title].is_object(), "Missing title {title}");
            for index in 0..arity.as_u64().unwrap() as usize {
                let argument = &metadata["args"][index];
                let label = argument["label"]
                    .as_str()
                    .unwrap_or_else(|| panic!("Missing argument {method}/{index}"));
                assert!(translations[label].is_object(), "Missing parameter {label}");
            }
        }
        for variable in inventory["flow_variables"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
        {
            let target = json!({"kind":"member","field":"value","object":{"kind":"call","function":{"kind":"name","name":"getvar"},"args":[{"kind":"member","object":{"kind":"name","name":"flowcharts"},"field":if variable=="ViewName"{"view"}else{"common"}},{"kind":"literal","value":variable}]}});
            assert!(
                assignment_setting(&target).is_some(),
                "Missing assignment purpose {variable}"
            );
        }
        for field in inventory["table_fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
        {
            assert!(table_field(field).is_some(), "Missing table field {field}");
        }
        for block in inventory["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
        {
            assert!(
                translations[format!("native.block.{block}")].is_object(),
                "Missing block {block}"
            );
        }
    }

    #[test]
    fn native_messages_cover_four_locales_and_preserve_placeholders() {
        let translations: Value =
            serde_json::from_str(include_str!("../data/i18n/native.json")).unwrap();
        let parameter = regex::Regex::new(r"\{([A-Za-z_][A-Za-z_0-9]*)\}").unwrap();
        for (name, entry) in translations.as_object().unwrap() {
            let baseline = parameter
                .captures_iter(entry["chs"].as_str().unwrap())
                .map(|m| m[1].to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            for locale in crate::i18n::LOCALES {
                let template = entry[locale]
                    .as_str()
                    .unwrap_or_else(|| panic!("Missing {locale}/{name}"));
                assert!(!template.trim().is_empty(), "Empty {locale}/{name}");
                let placeholders = parameter
                    .captures_iter(template)
                    .map(|m| m[1].to_owned())
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    placeholders, baseline,
                    "Placeholder mismatch {locale}/{name}"
                );
                let arguments = baseline
                    .iter()
                    .map(|key| (key.as_str(), format!("value_{key}")))
                    .collect::<Vec<_>>();
                let output = crate::i18n::message_for_locale(name, &arguments, locale);
                assert!(!output.starts_with("native."), "Unresolved {locale}/{name}");
                assert!(
                    !parameter.is_match(&output),
                    "Unreplaced placeholder {locale}/{name}"
                );
            }
        }
    }

    #[test]
    fn normal_settings_show_purposes_and_preserve_resource_ids() {
        let mut node = json!({"type":"native_action","operations":[
            {"kind":"assign","target":{"kind":"member","object":{"kind":"name","name":"sayoptions"},"field":"waitforinput"},"value":{"kind":"literal","value":true}},
            {"kind":"assign","target":{"kind":"member","object":{"kind":"name","name":"sayoptions"},"field":"fadewhendone"},"value":{"kind":"literal","value":false}},
            {"kind":"assign","target":{"kind":"member","field":"value","object":{"kind":"call","function":{"kind":"name","name":"getvar"},"args":[{"kind":"member","object":{"kind":"name","name":"flowcharts"},"field":"view"},{"kind":"literal","value":"ViewName"}]}},"value":{"kind":"literal","value":"room_night"}}
        ]});
        let before = node.clone();
        let context = egui::Context::default();
        context.enable_accesskit();
        let (output, changed) = frame(&context, &mut node, vec![]);
        assert!(!changed);
        assert_eq!(node, before);
        let text = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in ["等待玩家确认后继续", "对白结束后收起", "设置场景背景"]
        {
            assert!(text.contains(expected), "{text}");
        }
        for hidden in ["sayoptions", "ViewName", "所属对象", "字段 value"] {
            assert!(!text.contains(hidden), "{text}");
        }
        let views = resource_options("views", None);
        let (id, label) = views.iter().find(|(id, _)| id == "room_night").unwrap();
        assert_eq!(id, "room_night");
        assert_ne!(label, "room_night");
        assert!(label.contains("夜"));
        let menus = reference_options("menu_dialog").unwrap();
        assert!(menus
            .iter()
            .any(|(expr, _)| raw_name(expr) == "menudialogs.Options"));
        assert!(menus.iter().all(|(expr, _)| expr["kind"] == "member"));
    }

    #[test]
    fn numeric_switches_and_resource_selection_write_original_value_types() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut toggle = json!({"type":"native_action","operations":[{"kind":"call","expression":{"kind":"call","function":{"kind":"member","object":{"kind":"name","name":"luamanager"},"field":"SetEnableActions"},"args":[{"kind":"literal","value":1}]}}]});
        let (output, _) = frame(&context, &mut toggle, vec![]);
        let checkbox = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.role() == egui::accesskit::Role::CheckBox)
            .unwrap()
            .0;
        let (_, changed) = frame(
            &context,
            &mut toggle,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: checkbox,
                    data: None,
                },
            )],
        );
        assert!(changed);
        assert_eq!(toggle["operations"][0]["expression"]["args"][0]["value"], 0);

        let context = egui::Context::default();
        context.enable_accesskit();
        let mut background = json!({"type":"native_action","operations":[{"kind":"assign","target":{"kind":"member","field":"value","object":{"kind":"call","function":{"kind":"name","name":"getvar"},"args":[{"kind":"member","object":{"kind":"name","name":"flowcharts"},"field":"view"},{"kind":"literal","value":"ViewName"}]}},"value":{"kind":"literal","value":"room_night"}}]});
        let (output, _) = frame(&context, &mut background, vec![]);
        let combo = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.role() == egui::accesskit::Role::ComboBox)
            .unwrap()
            .0;
        let (output, _) = frame(
            &context,
            &mut background,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: combo,
                    data: None,
                },
            )],
        );
        let caption = key("native.name.black_view");
        let option = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(caption.as_str()))
            .unwrap()
            .0;
        let (_, changed) = frame(
            &context,
            &mut background,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: option,
                    data: None,
                },
            )],
        );
        assert!(changed);
        assert_eq!(background["operations"][0]["value"]["value"], "black");
    }

    #[test]
    fn decision_groups_and_descriptors_take_precedence_over_setup() {
        let group = json!({"type":"native_action","operations":[
            {"kind":"assign","target":{"kind":"local","name":"option_001","symbol":"v0"},"value":{"kind":"table","entries":[]}},
            {"kind":"assign","target":{"kind":"index","object":{"kind":"local","name":"option_001","symbol":"v0"},"index":{"kind":"literal","value":1}},"value":{"kind":"literal","value":"O_1","display":"向左"}},
            {"kind":"assign","target":{"kind":"index","object":{"kind":"local","name":"option_001","symbol":"v0"},"index":{"kind":"literal","value":2}},"value":{"kind":"literal","value":"O_2","display":"向右"}},
            {"kind":"assign","target":{"kind":"local","name":"choice_option_001","symbol":"v1"},"value":{"kind":"call","function":{"kind":"name","name":"choose"},"args":[{"kind":"local","name":"option_001","symbol":"v0"}]}}
        ]});
        assert_eq!(summary(&group), "显示原版选项 · 已配置 2 项");
        let descriptor = json!({"type":"native_branch","condition":{"kind":"literal","value":true},"_editor":{"native_condition":{"kind":"choice","index":"2","labels":["向右"]}}});
        assert_eq!(summary(&descriptor), "选择第 2 项 · 向右");
        for locale in crate::i18n::LOCALES {
            let output = crate::i18n::message_for_locale(
                "native.condition.choice",
                &[("number", "2".into()), ("text", "向右".into())],
                locale,
            );
            assert!(output.contains('2'));
            assert!(output.contains("向右"));
            assert!(!output.contains("{number}"));
        }
    }

    fn frame(
        context: &egui::Context,
        node: &mut Value,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        let mut changed = false;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 1500.0),
                )),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| changed = show(ui, node));
            },
        );
        (output, changed)
    }

    #[test]
    fn opening_parameter_forms_preserves_expressions_and_unknown_fields() {
        let expressions = [
            json!({"kind":"literal","value":"O_KEY","display":"原版选项文字"}),
            json!({"kind":"literal","value":17}),
            json!({"kind":"literal","value":18446744073709551615u64}),
            json!({"kind":"literal","value":1.25}),
            json!({"kind":"literal","value":true}),
            json!({"kind":"literal","value":null}),
            json!({"kind":"name","name":"narrative"}),
            json!({"kind":"local","name":"choice","symbol":"v7"}),
            json!({"kind":"member","object":{"kind":"name","name":"sayoptions"},"field":"waitforinput"}),
            json!({"kind":"index","object":{"kind":"local","name":"options","symbol":"v0"},"index":{"kind":"literal","value":1}}),
            json!({"kind":"unary","op":"not","value":{"kind":"literal","value":true}}),
            json!({"kind":"binary","op":"==","left":{"kind":"local","name":"choice","symbol":"v1"},"right":{"kind":"literal","value":1}}),
            json!({"kind":"call","function":{"kind":"name","name":"choose"},"args":[{"kind":"local","name":"options","symbol":"v0"}]}),
            json!({"kind":"table","entries":[{"key":null,"value":{"kind":"literal","value":"选项甲"}}]}),
            json!({"kind":"unknown","preserve":true}),
        ];
        for expression in expressions {
            let mut node = json!({"id":"branch","type":"native_branch","condition":expression,"success":"a","failure":"b","_editor":{"vanilla_origin":4}});
            let before = node.clone();
            let (_, changed) = frame(&egui::Context::default(), &mut node, vec![]);
            assert!(!changed);
            assert_eq!(node, before);
        }
        for mut node in [
            json!({"type":"native_branch"}),
            json!({"type":"native_action"}),
            json!({"type":"native_action","operations":[]}),
            json!({"type":"native_action","operations":[{"kind":"assign"}]}),
        ] {
            let before = node.clone();
            let (_, changed) = frame(&egui::Context::default(), &mut node, vec![]);
            assert!(!changed);
            assert_eq!(node, before);
        }
    }

    #[test]
    fn editing_literal_parameters_keeps_bindings_and_flow_targets() {
        let mut node = json!({"id":"action","type":"native_action","goto":"next","_editor":{"vanilla_origin":3},"operations":[
            {"kind":"call","expression":{"kind":"call","function":{"kind":"member","object":{"kind":"name","name":"luamanager"},"field":"SetNextScript"},"args":[{"kind":"literal","value":"OLD","display":"原版文字"}]}},
            {"kind":"assign","target":{"kind":"local","name":"enabled","symbol":"v3"},"value":{"kind":"literal","value":true}}
        ]});
        let mut expected = node.clone();
        let context = egui::Context::default();
        context.enable_accesskit();
        let (output, changed) = frame(&context, &mut node, vec![]);
        assert!(!changed);
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let input = nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::TextInput)
            .unwrap();
        let bounds = input.1.bounds().unwrap();
        assert!(bounds.x1 - bounds.x0 > 600.0);
        let input = input.0;
        let checkbox = nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::CheckBox)
            .unwrap()
            .0;
        let (_, changed) = frame(
            &context,
            &mut node,
            vec![
                egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Focus,
                    target: input,
                    data: None,
                }),
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                },
                egui::Event::Text("NEW".into()),
            ],
        );
        assert!(changed);
        expected["operations"][0]["expression"]["args"][0]["value"] = json!("NEW");
        expected["operations"][0]["expression"]["args"][0]
            .as_object_mut()
            .unwrap()
            .remove("display");
        assert_eq!(node, expected);
        let (_, changed) = frame(
            &context,
            &mut node,
            vec![egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Click,
                    target: checkbox,
                    data: None,
                },
            )],
        );
        assert!(changed);
        expected["operations"][1]["value"]["value"] = json!(false);
        assert_eq!(node, expected);
        assert_eq!(
            summary(&json!({"type":"native_action","operations":[]})),
            "分支汇合"
        );
        assert_eq!(
            summary(
                &json!({"type":"native_branch","condition":{"kind":"binary","op":"==","left":{"kind":"local","name":"choice","symbol":"v1"},"right":{"kind":"literal","value":2}}})
            ),
            "所选选项 等于 2"
        );
    }
}
