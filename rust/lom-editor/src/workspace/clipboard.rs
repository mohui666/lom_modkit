//! Structured steps travel through the native clipboard, including between processes.
use super::*;

const FORMAT: &str = "lom-modkit/steps";

impl App {
    pub(super) fn copy_to_system(&mut self, ctx: &egui::Context) {
        self.copy();
        if self.clipboard.is_empty() {
            return;
        }
        match serde_json::to_string(
            &json!({"format": FORMAT, "version": 1, "nodes": self.clipboard}),
        ) {
            Ok(text) => {
                ctx.copy_text(text);
                self.status = crate::i18n::message(
                    "workspace.clipboard.copied",
                    &[("count", self.clipboard.len().to_string())],
                );
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }

    pub(super) fn request_system_paste(&mut self, ctx: &egui::Context) {
        self.node_paste_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        ctx.request_repaint();
    }

    pub(super) fn paste_from_system(&mut self, text: &str) {
        self.node_paste_requested = false;
        match decode(text, &self.catalog) {
            Ok(nodes) => {
                self.clipboard = nodes;
                self.paste();
            }
            Err(error) => self.error = Some(crate::error_ui::capture(error)),
        }
    }
}

fn decode(text: &str, catalog: &Catalog) -> anyhow::Result<Vec<Value>> {
    let value: Value =
        serde_json::from_str(text).context(crate::i18n::key("workspace.clipboard.invalid"))?;
    anyhow::ensure!(
        value["format"] == FORMAT && value["version"] == 1,
        "{}",
        crate::i18n::key("workspace.clipboard.invalid")
    );
    let nodes = value["nodes"]
        .as_array()
        .filter(|nodes| !nodes.is_empty())
        .context(crate::i18n::key("workspace.clipboard.invalid"))?;
    let mut ids = BTreeSet::new();
    for node in nodes {
        let id = node["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context(crate::i18n::key("workspace.clipboard.invalid"))?;
        let kind = node["type"]
            .as_str()
            .context(crate::i18n::key("workspace.clipboard.invalid"))?;
        anyhow::ensure!(
            node.is_object()
                && ids.insert(id)
                && catalog.schema["NODE_SCHEMAS"].get(kind).is_some(),
            "{}",
            crate::i18n::key("workspace.clipboard.invalid")
        );
    }
    Ok(nodes.clone())
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
