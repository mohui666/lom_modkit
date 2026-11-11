//! User preference changes.
use super::*;

impl App {
    pub(super) fn set_legacy_character_rotation(
        &mut self,
        legacy: bool,
        path: &Path,
    ) -> anyhow::Result<()> {
        let mut prefs = if path.exists() {
            lom_core::load_json(path)?
        } else {
            json!({})
        };
        anyhow::ensure!(prefs.is_object(), "个人设置格式无效，未修改旋转选项");
        prefs["legacy_character_rotation"] = json!(legacy);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.preview.cancel_layout_drag();
        self.preview.legacy_character_rotation = legacy;
        Ok(())
    }
    pub(super) fn set_startup_update_check(
        &mut self,
        enabled: bool,
        path: &Path,
    ) -> anyhow::Result<()> {
        let mut prefs = lom_core::load_json(path)
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        prefs["check_updates_on_startup"] = json!(enabled);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.check_updates_on_startup = enabled;
        Ok(())
    }
    pub(super) fn set_auto_save(
        &mut self,
        enabled: bool,
        seconds: u64,
        path: &Path,
    ) -> anyhow::Result<()> {
        anyhow::ensure!([30, 60, 300, 600].contains(&seconds), "自动保存间隔无效");
        let mut prefs = if path.exists() {
            lom_core::load_json(path)?
        } else {
            json!({})
        };
        anyhow::ensure!(prefs.is_object(), "个人设置格式无效，未修改自动保存设置");
        prefs["auto_save_project"] = json!(enabled);
        prefs["auto_save_seconds"] = json!(seconds);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        self.auto_save_project = enabled;
        self.auto_save_seconds = seconds;
        self.last_auto_save = Instant::now();
        Ok(())
    }
    pub(super) fn set_interface_locale(&mut self, locale: &str, path: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(crate::i18n::LOCALES.contains(&locale), "不支持的界面语言");
        let mut prefs = lom_core::load_json(path)
            .ok()
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        prefs["ui_locale"] = json!(locale);
        lom_core::project::atomic_write(path, &lom_core::stable_json(&prefs)?)?;
        crate::i18n::set_locale(locale);
        crate::macos_window::update_settings_menu_title();
        Ok(())
    }
}
