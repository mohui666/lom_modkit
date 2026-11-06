//! Read-only bridge from the game's explicit author bookmark shortcut.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bookmark {
    pub schema: u32,
    pub script: String,
    pub text_key: String,
    pub text: String,
    pub game_dir: PathBuf,
    pub year: i32,
    pub month: i32,
    pub stage: i32,
    pub captured_utc: String,
}
impl Bookmark {
    pub fn caption(&self) -> String {
        let stage = match self.stage {
            1 => "上旬",
            2 => "中旬",
            3 => "下旬",
            _ => "",
        };
        if self.year > 0 {
            format!("第{}年{}月{stage} · {}", self.year, self.month, self.script)
        } else {
            self.script.clone()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineEntry {
    pub script: String,
    pub title: String,
    pub series: String,
    pub key: String,
    pub main: bool,
    pub next_key: String,
    pub condition: String,
    pub place: String,
    #[serde(rename = "when")]
    pub when_label: String,
    pub from_round: Option<i32>,
    pub to_round: Option<i32>,
    pub active: bool,
}
impl TimelineEntry {
    pub fn matches_date(&self, year: i32, month: i32, unspecified: bool) -> bool {
        if year == 0 && month == 0 {
            return true;
        }
        let Some(start) = self.from_round else {
            return unspecified;
        };
        let end = self.to_round.unwrap_or(start);
        if end < start {
            return false;
        }
        (start..=end).any(|round| {
            (year == 0 || round / 36 + 1 == year) && (month == 0 || round % 36 / 3 + 1 == month)
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    pub schema: u32,
    pub game_dir: PathBuf,
    pub game_version: String,
    pub missions: Vec<TimelineEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTransition {
    pub from: String,
    pub target: String,
    pub captured_utc: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRoutes {
    pub schema: u32,
    pub game_dir: PathBuf,
    pub session: String,
    pub captured_utc: String,
    pub status: String,
    pub script: String,
    pub temp_script1: String,
    pub travel_script: String,
    pub transitions: Vec<RuntimeTransition>,
}

/// The value at capture time, never a prediction for an unrelated saved game.
impl RuntimeRoutes {
    pub fn return_target(&self) -> Option<&str> {
        if self.status != "story" {
            return None;
        }
        let target = match self.script.to_ascii_lowercase().as_str() {
            "s0019_01" | "s2504_01" | "talk_girl5_check" => &self.temp_script1,
            "s8500_01_06" => &self.travel_script,
            _ => return None,
        };
        (!target.is_empty()).then_some(target.as_str())
    }

    pub fn has_return_slot(&self) -> bool {
        self.status == "story"
            && matches!(
                self.script.to_ascii_lowercase().as_str(),
                "s0019_01" | "s2504_01" | "talk_girl5_check" | "s8500_01_06"
            )
    }
}

pub fn read_runtime_routes(root: &Path) -> Result<Option<RuntimeRoutes>> {
    let path = root.join("route-runtime.json");
    let info = match fs::metadata(&path) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(info.len() <= 65536, "路线运行快照文件过大");
    let snapshot: RuntimeRoutes = serde_json::from_value(crate::load_json(&path)?)?;
    ensure!(
        snapshot.schema == 1
            && snapshot.transitions.len() <= 32
            && !snapshot.session.is_empty()
            && snapshot.session.len() <= 64
            && matches!(snapshot.status.as_str(), "story" | "inactive"),
        "路线运行快照格式无效"
    );
    for target in [
        &snapshot.script,
        &snapshot.temp_script1,
        &snapshot.travel_script,
    ] {
        ensure!(
            target.is_empty() || crate::vanilla_patch::valid_target(target),
            "路线运行快照章节无效"
        );
    }
    for transition in &snapshot.transitions {
        ensure!(
            crate::vanilla_patch::valid_target(&transition.from)
                && crate::vanilla_patch::valid_target(&transition.target),
            "路线跳转记录章节无效"
        );
    }
    Ok(Some(snapshot))
}

pub fn default_root() -> PathBuf {
    crate::content::default_repository_root()
        .parent()
        .unwrap()
        .join("rust/story-bookmarks")
}

pub fn read(root: &Path) -> Result<Vec<Bookmark>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut marks = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("mark-") || !name.ends_with(".json") || !entry.file_type()?.is_file() {
            continue;
        }
        ensure!(entry.metadata()?.len() <= 65536, "章节标记文件过大：{name}");
        let mark: Bookmark = serde_json::from_value(crate::load_json(entry.path())?)
            .with_context(|| format!("章节标记格式无效：{name}"))?;
        ensure!(
            mark.schema == 1
                && crate::vanilla_patch::valid_target(&mark.script)
                && (0..=9).contains(&mark.year)
                && (0..=12).contains(&mark.month)
                && (0..=3).contains(&mark.stage),
            "章节标记内容无效：{name}"
        );
        marks.push(mark);
    }
    marks.sort_by(|a, b| b.captured_utc.cmp(&a.captured_utc));
    Ok(marks)
}

pub fn read_timeline(root: &Path) -> Result<Option<Timeline>> {
    let path = root.join("timeline.json");
    let info = match fs::metadata(&path) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(info.len() <= 8 * 1024 * 1024, "任务时间线文件过大");
    let mut timeline: Timeline = serde_json::from_value(crate::load_json(&path)?)?;
    ensure!(
        timeline.schema == 2 && timeline.missions.len() <= 20000,
        "任务时间线格式无效"
    );
    for row in &timeline.missions {
        ensure!(
            crate::vanilla_patch::valid_target(&row.script)
                && row
                    .from_round
                    .is_none_or(|round| (0..=323).contains(&round))
                && row.to_round.is_none_or(|round| (0..=323).contains(&round)),
            "任务时间线日期或章节无效"
        );
    }
    timeline
        .missions
        .sort_by_key(|row| row.from_round.unwrap_or(i32::MAX));
    Ok(Some(timeline))
}
