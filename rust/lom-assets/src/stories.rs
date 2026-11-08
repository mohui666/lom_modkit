use crate::unity::{self, Reader};
use anyhow::{ensure, Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use unity_rs_core::studio::Studio;

const PREFIX: &str = "story/chinesetraditional/";
pub struct Stories {
    pub studio: Studio,
    pub sources: BTreeMap<String, String>,
}
impl Stories {
    pub fn open(game: &Path, with_source: bool) -> Result<Self> {
        let studio = unity::open(&game.join("Mortal_Data/globalgamemanagers"))?;
        let manager = studio
            .objects()
            .find(|o| o.class_id() == 147)
            .context("没有找到游戏资源目录")?;
        let entries = manager.read_resource_manager(Default::default())?;
        let mut sources = BTreeMap::new();
        for entry in entries
            .container
            .iter()
            .filter(|e| e.key.starts_with(PREFIX))
        {
            let object = unity::resolve(&studio, manager, entry.asset)?;
            ensure!(object.class_id() == 49, "原版章节资源不是 Lua 文本");
            let name = object.name().context("原版章节资源没有名称")?.to_owned();
            let source = if with_source {
                String::from_utf8(object.read_text_bytes(16 * 1024 * 1024)?)?
                    .trim_start_matches('\u{feff}')
                    .to_owned()
            } else {
                String::new()
            };
            sources.insert(name, source);
        }
        ensure!(!sources.is_empty(), "没有找到原版章节，请检查游戏资源结构");
        Ok(Self { studio, sources })
    }
    pub fn scene_path(&self, game: &Path) -> Result<PathBuf> {
        let settings = self
            .studio
            .objects()
            .find(|o| o.class_id() == 141)
            .context("游戏缺少场景目录")?
            .read_build_settings(Default::default())?;
        let scenes = settings.scenes.context("游戏缺少场景列表")?;
        let index = scenes
            .iter()
            .position(|p| unity::basename(p) == "Story.unity")
            .context("没有找到原版剧情场景")?;
        Ok(game.join(format!("Mortal_Data/sharedassets{index}.assets")))
    }
}
pub fn read(game: &Path, target: Option<&str>) -> Result<Value> {
    let stories = Stories::open(game, target.is_some())?;
    if let Some(target) = target {
        ensure!(
            Regex::new(r"^[A-Za-z0-9_-]{1,64}$")?.is_match(target) && !target.starts_with("MOD_"),
            "原版章节编号无效"
        );
        let source = stories
            .sources
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(target))
            .map(|(_, s)| s)
            .with_context(|| format!("当前游戏中没有原版章节：{target}"))?;
        Ok(json!({"target":target,"source":source,"translations":translations(game,source)?}))
    } else {
        Ok(json!({"chapters":stories.sources.keys().collect::<Vec<_>>()}))
    }
}
pub fn translations(game: &Path, source: &str) -> Result<BTreeMap<String, String>> {
    let mut keys = BTreeSet::new();
    let mut options = BTreeSet::new();
    for capture in
        Regex::new(r#"GetStoryText\s*\(\s*["']([A-Za-z0-9_-]+)["']"#)?.captures_iter(source)
    {
        keys.insert(capture[1].to_owned());
    }
    for capture in
        Regex::new(r#"["'](O_[A-Za-z0-9_]+(?:\|[^"'\r\n]*)?)["']"#)?.captures_iter(source)
    {
        let reference = capture[1].to_owned();
        keys.insert(reference.split('|').next().unwrap().into());
        options.insert(reference);
    }
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let studio = unity::open(&game.join("Mortal_Data/level0"))?;
    let mut result = BTreeMap::new();
    for object in studio
        .objects()
        .filter(|o| o.class_id() == 114 && unity::basename(o.source_path()) == "level0")
    {
        let (namespace, class) = unity::script_type(&studio, object)?;
        if namespace != "Lean.Localization" || class != "LeanLanguageCSV" {
            continue;
        }
        let bytes = unity::raw(object)?;
        let mut r = Reader::at(&bytes, 28);
        r.string()?;
        r.skip(12)?;
        if r.string()? != "ChineseSimplified" {
            continue;
        }
        for _ in 0..3 {
            r.string()?;
        }
        r.skip(4)?;
        for _ in 0..r.count(1_000_000)? {
            let name = r.string()?;
            let text = r.string()?;
            if let Some(key) = name.strip_prefix("Story/") {
                if keys.contains(key) {
                    result.insert(key.to_owned(), text);
                }
            }
        }
    }
    ensure!(
        !result.is_empty(),
        "没有读到原版章节的简体文字，请核对当前游戏版本"
    );
    for reference in options {
        if let Some((key, suffix)) = reference.split_once('|') {
            if let Some(text) = result.get(key) {
                result.insert(reference.clone(), format!("{text}|{suffix}"));
            }
        }
    }
    Ok(result)
}
pub fn positions(game: &Path, stories: &Stories) -> Result<BTreeMap<String, Vec<String>>> {
    let scene = stories.scene_path(game)?;
    let studio = unity::open(&scene)?;
    let available: BTreeSet<_> = stories.sources.keys().map(|n| n.to_lowercase()).collect();
    let mut result = BTreeMap::new();
    for object in studio.objects().filter(|o| {
        o.class_id() == 114 && Path::new(o.source_path()).file_name() == scene.file_name()
    }) {
        let (namespace, class) = unity::script_type(&studio, object)?;
        if namespace != "Mortal.Story" || class != "PositionResultData" {
            continue;
        }
        let bytes = unity::raw(object)?;
        let mut r = Reader::at(&bytes, 28);
        let name = r.string()?;
        let count = r.count(100000)?;
        let events = (0..count).map(|_| r.ptr()).collect::<Result<Vec<_>>>()?;
        r.string()?;
        let prefix = r.string()?;
        let mut candidates = BTreeSet::new();
        let default = format!("{prefix}_001");
        if available.contains(&default.to_lowercase()) {
            candidates.insert(default);
        }
        for pointer in events {
            let event = unity::resolve(&studio, object, pointer)?;
            let bytes = unity::raw(event)?;
            let mut r = Reader::at(&bytes, 28);
            r.string()?;
            r.string()?;
            let value = unity::mapping(&studio, event, r.ptr()?)?;
            candidates.insert(format!("{prefix}_{value}"));
        }
        result.insert(name, candidates.into_iter().collect());
    }
    ensure!(!result.is_empty(), "没有读到原版场所检定配置");
    Ok(result)
}
pub fn audio_key(game: &Path, kind: &str, name: &str) -> Result<String> {
    let expected = match kind {
        "music" => "MusicDataCollection",
        "sound" => "SoundDataCollection",
        "env" => "EnvSoundDataCollection",
        _ => anyhow::bail!("试听音频类型无效"),
    };
    let stories = Stories::open(game, false)?;
    let scene = stories.scene_path(game)?;
    let studio = unity::open(&scene)?;
    for object in studio.objects().filter(|o| {
        o.class_id() == 114 && Path::new(o.source_path()).file_name() == scene.file_name()
    }) {
        let (namespace, class) = unity::script_type(&studio, object)?;
        if namespace != "Mortal.Core" || class != expected {
            continue;
        }
        let bytes = unity::raw(object)?;
        let mut r = Reader::at(&bytes, 28);
        r.string()?;
        for _ in 0..r.count(100000)? {
            let entry = unity::resolve(&studio, object, r.ptr()?)?;
            let bytes = unity::raw(entry)?;
            let mut entry = Reader::at(&bytes, 28);
            let asset_name = entry.string()?;
            let key = entry.string()?;
            if asset_name == name {
                return Ok(key);
            }
        }
    }
    anyhow::bail!("当前游戏的音频集合中没有资源“{name}”。请重新选择资源并确认音效类型；重建工程不会补回缺失资源。")
}
