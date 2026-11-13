//! Timeline audio, separate from single-resource audition and project data.
use crate::audio;
use anyhow::{Context, Result};
use lom_core::project::Project;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Channel {
    Music,
    Env,
    Sound,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    channel: Channel,
    name: String,
}
impl Key {
    fn kind(&self) -> &str {
        match self.channel {
            Channel::Music => "music",
            Channel::Env => "env",
            Channel::Sound => "sound",
        }
    }
}
#[derive(Clone)]
enum Cue {
    Play(Key),
    StopAll,
    Fade(Channel, f64),
}

fn cues(node: &Value) -> Vec<Cue> {
    if node["type"] == "parallel" {
        return node["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(cues)
            .collect();
    }
    let channel = match node["type"].as_str() {
        Some("music") => Channel::Music,
        Some("sound") if node["kind"] == "env" => Channel::Env,
        Some("sound") => Channel::Sound,
        _ => return vec![],
    };
    match node["op"].as_str().unwrap_or("play") {
        "stop" if channel == Channel::Music => vec![Cue::StopAll],
        "fadeout" => vec![Cue::Fade(
            if channel == Channel::Sound {
                Channel::Env
            } else {
                channel
            },
            node["seconds"]
                .as_f64()
                .unwrap_or(if channel == Channel::Music { 2.0 } else { 1.0 })
                .max(0.0),
        )],
        "play" => node["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(|name| {
                Cue::Play(Key {
                    channel,
                    name: name.into(),
                })
            })
            .into_iter()
            .collect(),
        _ => vec![],
    }
}

fn restored(story: &Value, index: usize) -> Vec<Cue> {
    let Some(nodes) = story["nodes"].as_array() else {
        return vec![];
    };
    let Some(id) = nodes.get(index).and_then(|node| node["id"].as_str()) else {
        return vec![];
    };
    let mut tracks = BTreeMap::new();
    for id in crate::preview::route(story, id)
        .unwrap_or_default()
        .into_iter()
        .take_while(|next| next != id)
    {
        if let Some(node) = nodes.iter().find(|node| node["id"] == id) {
            for cue in cues(node) {
                match cue {
                    Cue::Play(key) if key.channel != Channel::Sound => {
                        tracks.insert(key.channel, key);
                    }
                    Cue::StopAll => tracks.clear(),
                    Cue::Fade(channel, _) => {
                        tracks.remove(&channel);
                    }
                    _ => (),
                }
            }
        }
    }
    tracks.into_values().map(Cue::Play).collect()
}

#[derive(Default)]
struct Asset {
    player: Option<audio::Player>,
    error: Option<String>,
}
struct Pending {
    key: Key,
    fade: Option<f64>,
}
struct Voice {
    key: Key,
    gain: f32,
    fade: Option<(f64, f64, f32)>,
    #[cfg(windows)]
    sink: rodio::Sink,
}

#[derive(Default)]
pub(crate) struct Timeline {
    location: Option<(String, String)>,
    cache: BTreeMap<Key, Asset>,
    pending: Vec<Pending>,
    voices: Vec<Voice>,
    last_tick: Option<Instant>,
    advancing: bool,
    warnings: Vec<String>,
    #[cfg(windows)]
    output: Option<rodio::OutputStream>,
    #[cfg(all(test, windows))]
    pub(crate) test_mixer: Option<rodio::mixer::Mixer>,
}

impl Timeline {
    pub fn stop(&mut self) {
        self.finish();
        self.warnings.clear();
    }
    /// Keep preparation errors readable after the last visual step finishes.
    pub fn finish(&mut self) {
        self.voices.clear();
        self.pending.clear();
        self.cache.clear();
        self.location = None;
        self.last_tick = None;
        self.advancing = false;
        #[cfg(windows)]
        {
            self.output = None;
        }
    }
    pub fn pause(&mut self) {
        self.advancing = false;
        #[cfg(windows)]
        for voice in &self.voices {
            voice.sink.pause();
        }
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn busy(&self) -> bool {
        self.cache
            .values()
            .any(|asset| asset.player.as_ref().is_some_and(audio::Player::preparing))
    }
    pub fn needs_tick(&self) -> bool {
        self.busy()
            || self.buffering()
            || self
                .cache
                .values()
                .any(|asset| asset.player.is_none() && asset.error.is_none())
            || (self.advancing && self.voices.iter().any(|voice| voice.fade.is_some()))
    }
    pub fn buffering(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn location(&self) -> Option<&(String, String)> {
        self.location.as_ref()
    }
    pub fn begin(&mut self, chapter: &str, story: &Value, index: usize) {
        self.stop();
        self.apply(restored(story, index));
        self.enter(chapter, &story["nodes"][index]);
    }
    pub fn enter(&mut self, chapter: &str, node: &Value) {
        self.location = Some((chapter.into(), node["id"].as_str().unwrap_or("").into()));
        self.apply(cues(node));
    }
    pub fn prefetch(&mut self, node: &Value) {
        for cue in cues(node) {
            if let Cue::Play(key) = cue {
                self.cache.entry(key).or_default();
            }
        }
    }
    fn apply(&mut self, cues: Vec<Cue>) {
        for cue in cues {
            match cue {
                Cue::StopAll => {
                    self.voices.clear();
                    self.pending.clear();
                }
                Cue::Fade(channel, seconds) => {
                    for voice in self
                        .voices
                        .iter_mut()
                        .filter(|voice| voice.key.channel == channel)
                    {
                        voice.fade = Some((0.0, seconds, voice.gain));
                    }
                    for pending in self
                        .pending
                        .iter_mut()
                        .filter(|pending| pending.key.channel == channel)
                    {
                        pending.fade = Some(seconds);
                    }
                }
                Cue::Play(key) => {
                    if key.channel != Channel::Sound {
                        if let Some(voice) = self
                            .voices
                            .iter_mut()
                            .find(|voice| voice.key == key && !key.name.starts_with("user:"))
                        {
                            voice.fade = None;
                            voice.gain = 1.0;
                            #[cfg(windows)]
                            voice.sink.set_volume(1.0);
                            continue;
                        }
                        self.voices.retain(|voice| voice.key.channel != key.channel);
                        self.pending
                            .retain(|pending| pending.key.channel != key.channel);
                    }
                    self.pending.push(Pending { key, fade: None });
                }
            }
        }
        for pending in &self.pending {
            self.cache.entry(pending.key.clone()).or_default();
        }
    }
    fn prepare(&mut self, project: &Project, game: &Path) {
        for (key, asset) in &mut self.cache {
            if let Some(player) = &mut asset.player {
                if player.preparing() {
                    if let Err(error) = player.update() {
                        let message = format!("{}：{error:#}", key.name);
                        asset.error = Some(message.clone());
                        self.warnings.push(message);
                    }
                }
            }
        }
        if self.busy() {
            return;
        }
        let next = self
            .pending
            .iter()
            .map(|pending| &pending.key)
            .find(|key| {
                self.cache
                    .get(*key)
                    .is_some_and(|asset| asset.player.is_none() && asset.error.is_none())
            })
            .cloned()
            .or_else(|| {
                self.cache
                    .iter()
                    .find(|(_, asset)| asset.player.is_none() && asset.error.is_none())
                    .map(|(key, _)| key.clone())
            });
        let Some(key) = next else { return };
        let mut player = audio::Player::preparer();
        let result = if key.name.starts_with("user:") {
            player.play_content(project, &key.name)
        } else {
            (|| -> Result<()> {
                player.play_official(
                    game,
                    key.kind(),
                    &key.name,
                    &crate::components::extractor()?,
                    &crate::components::decoder()?,
                )
            })()
        };
        let asset = self.cache.get_mut(&key).unwrap();
        match result {
            Ok(()) => asset.player = Some(player),
            Err(error) => {
                let message = format!("{}：{error:#}", key.name);
                asset.error = Some(message.clone());
                self.warnings.push(message);
            }
        }
    }
    fn start_voice(&mut self, key: Key, path: &Path, fade: Option<f64>) -> Result<()> {
        #[cfg(windows)]
        {
            let mixer = {
                #[cfg(test)]
                if let Some(mixer) = &self.test_mixer {
                    return self.start_on_mixer(key, path, fade, &mixer.clone());
                }
                if self.output.is_none() {
                    let mut output = rodio::OutputStreamBuilder::open_default_stream()
                        .context("无法打开音频输出设备")?;
                    output.log_on_drop(false);
                    self.output = Some(output);
                }
                self.output.as_ref().unwrap().mixer().clone()
            };
            self.start_on_mixer(key, path, fade, &mixer)
        }
        #[cfg(not(windows))]
        {
            let _ = (key, path, fade);
            anyhow::bail!("连续音频预览目前支持 Windows");
        }
    }
    #[cfg(windows)]
    fn start_on_mixer(
        &mut self,
        key: Key,
        path: &Path,
        fade: Option<f64>,
        mixer: &rodio::mixer::Mixer,
    ) -> Result<()> {
        use std::io::Cursor;
        let bytes = std::fs::read(path).context("无法读取预览音频")?;
        let (sink, mut source) = rodio::Sink::new();
        sink.pause();
        if key.channel == Channel::Sound {
            sink.append(rodio::Decoder::try_from(Cursor::new(bytes)).context("无法解码预览音效")?);
        } else {
            sink.append(
                rodio::Decoder::new_looped(Cursor::new(bytes))
                    .context("无法解码预览音乐或环境音")?,
            );
        }
        // An empty Sink queue reports 48 kHz. Prime it while paused so the
        // mixer reads the clip's actual format without consuming audio.
        let _ = source.next();
        mixer.add(source);
        self.voices.push(Voice {
            key,
            gain: 1.0,
            fade: fade.map(|seconds| (0.0, seconds, 1.0)),
            sink,
        });
        Ok(())
    }
    pub fn update(&mut self, project: &Project, game: &Path, playing: bool) {
        self.prepare(project, game);
        let ready = self.pending.iter().all(|pending| {
            self.cache.get(&pending.key).is_some_and(|asset| {
                asset.error.is_some()
                    || asset
                        .player
                        .as_ref()
                        .and_then(audio::Player::prepared_file)
                        .is_some()
            })
        });
        if ready {
            for pending in std::mem::take(&mut self.pending) {
                let path = self
                    .cache
                    .get(&pending.key)
                    .filter(|asset| asset.error.is_none())
                    .and_then(|asset| asset.player.as_ref())
                    .and_then(audio::Player::prepared_file)
                    .map(Path::to_path_buf);
                if let Some(path) = path {
                    if let Err(error) = self.start_voice(pending.key.clone(), &path, pending.fade) {
                        let message = format!("{}：{error:#}", pending.key.name);
                        self.cache.get_mut(&pending.key).unwrap().error = Some(message.clone());
                        self.warnings.push(message);
                    }
                }
            }
        }
        let now = Instant::now();
        let advancing = playing && !self.buffering();
        let delta = self
            .last_tick
            .replace(now)
            .filter(|_| advancing && self.advancing)
            .map_or(0.0, |previous| now.duration_since(previous).as_secs_f64());
        self.advancing = advancing;
        #[cfg(windows)]
        self.voices.retain(|voice| !voice.sink.empty());
        for voice in &mut self.voices {
            if advancing {
                if let Some((elapsed, total, start)) = &mut voice.fade {
                    *elapsed += delta;
                    voice.gain = if *total <= 0.0 {
                        0.0
                    } else {
                        *start * (1.0 - (*elapsed / *total).min(1.0)) as f32
                    };
                }
            }
            #[cfg(windows)]
            {
                voice.sink.set_volume(voice.gain);
                if advancing {
                    voice.sink.play();
                } else {
                    voice.sink.pause();
                }
            }
        }
        self.voices.retain(|voice| voice.gain > 0.0);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn wav(sample: i16, samples: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples * 2).to_le_bytes());
        for _ in 0..samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
    fn project() -> Project {
        let mut project = Project::new();
        for (id, sample) in [("music", 4096), ("env", 2048), ("fx", 8192)] {
            let root = format!("assets/user/audio/{id}/");
            project.assets.insert(
                format!("{root}content.json"),
                serde_json::to_vec(&json!({
                "content_schema":1,"id":id,"type":"audio","files":{"main":"clip.wav"}}))
                .unwrap(),
            );
            project
                .assets
                .insert(format!("{root}clip.wav"), wav(sample, 800));
        }
        project
    }
    fn group() -> Value {
        json!({"id":"audio","type":"parallel","actions":[
            {"type":"music","name":"user:music"},
            {"type":"sound","kind":"env","name":"user:env"},
            {"type":"sound","name":"user:fx"}]})
    }
    fn render(source: &mut rodio::mixer::MixerSource, count: usize) -> f32 {
        let value = (0..count)
            .map(|_| source.next().unwrap_or(0.0))
            .last()
            .unwrap();
        value
    }
    fn update(timeline: &mut Timeline, project: &Project, playing: bool) {
        timeline.update(project, Path::new("unused-for-project-audio"), playing);
        assert!(timeline.warnings().is_empty(), "{:?}", timeline.warnings());
    }
    #[test]
    fn mixes_music_environment_and_overlapping_effects_with_pause_fade_and_cleanup() {
        let project = project();
        let original = project.clone();
        let (mixer, mut source) = rodio::mixer::mixer(1, 8000);
        let mut timeline = Timeline {
            test_mixer: Some(mixer),
            ..Default::default()
        };
        timeline.enter("main", &group());
        update(&mut timeline, &project, true);
        assert!(timeline.buffering());
        assert!(
            timeline.voices.is_empty(),
            "start a simultaneous group together"
        );
        update(&mut timeline, &project, true);
        update(&mut timeline, &project, true);
        assert!(!timeline.buffering());
        assert!((render(&mut source, 160) - 0.4375).abs() < 0.0001);
        timeline.enter(
            "main",
            &json!({"id":"second","type":"sound","name":"user:fx"}),
        );
        update(&mut timeline, &project, true);
        assert!((render(&mut source, 160) - 0.6875).abs() < 0.0001);
        timeline.pause();
        assert!(render(&mut source, 160).abs() < 0.0001);
        timeline.last_tick = Some(Instant::now() - Duration::from_secs(10));
        update(&mut timeline, &project, true);
        assert!((render(&mut source, 160) - 0.6875).abs() < 0.0001);
        assert!(
            (render(&mut source, 1000) - 0.1875).abs() < 0.0001,
            "effects finish and persistent tracks loop"
        );
        update(&mut timeline, &project, true);
        assert_eq!(timeline.voices.len(), 2);
        timeline.enter(
            "main",
            &json!({"id":"fade","type":"sound","kind":"env","op":"fadeout","seconds":1}),
        );
        timeline.last_tick = Some(Instant::now() - Duration::from_millis(500));
        update(&mut timeline, &project, true);
        assert!((render(&mut source, 160) - 0.15625).abs() < 0.001);
        timeline.pause();
        timeline.last_tick = Some(Instant::now() - Duration::from_secs(10));
        update(&mut timeline, &project, true);
        assert!(
            (render(&mut source, 160) - 0.15625).abs() < 0.001,
            "pause must not consume fade time"
        );
        timeline.last_tick = Some(Instant::now() - Duration::from_millis(550));
        update(&mut timeline, &project, true);
        assert!((render(&mut source, 160) - 0.125).abs() < 0.0001);
        let cache: Vec<_> = timeline
            .cache
            .values()
            .filter_map(|asset| {
                asset
                    .player
                    .as_ref()?
                    .prepared_file()
                    .map(Path::to_path_buf)
            })
            .collect();
        timeline.enter("main", &json!({"id":"stop","type":"music","op":"stop"}));
        update(&mut timeline, &project, true);
        assert!(render(&mut source, 160).abs() < 0.0001);
        assert!(timeline.voices.is_empty());
        timeline.stop();
        assert!(cache.iter().all(|path| !path.exists()));
        assert_eq!(project, original);
    }
    #[test]
    fn starting_in_the_middle_restores_only_persistent_tracks_on_the_preview_route() {
        let story = json!({"id":"main","start":"bg","nodes":[
            {"id":"bg","type":"music","name":"user:music","goto":"fx"},
            {"id":"unused","type":"music","name":"wrong-route"},
            {"id":"fx","type":"sound","name":"user:fx","goto":"env"},
            {"id":"env","type":"sound","kind":"env","name":"user:env","goto":"fade"},
            {"id":"fade","type":"sound","op":"fadeout","goto":"say"},
            {"id":"say","type":"say","text":"middle"}]});
        let mut timeline = Timeline::default();
        timeline.begin("main", &story, 5);
        assert_eq!(timeline.pending.len(), 1);
        assert_eq!(timeline.pending[0].key.name, "user:music");
        assert_eq!(crate::preview::playback_duration(&story["nodes"][4]), 1.0);
        assert_eq!(
            crate::preview::playback_duration(
                &json!({"type":"music","op":"fadeout","seconds":3.5})
            ),
            3.5
        );
    }
    #[test]
    fn a_missing_clip_reports_once_and_does_not_block_other_audio_or_playback() {
        let project = project();
        let (mixer, mut source) = rodio::mixer::mixer(1, 8000);
        let mut timeline = Timeline {
            test_mixer: Some(mixer),
            ..Default::default()
        };
        let node = json!({"id":"bad","type":"parallel","actions":[
            {"type":"music","name":"user:music"}, {"type":"sound","name":"user:missing"}]});
        timeline.enter("main", &node);
        for _ in 0..4 {
            timeline.update(&project, Path::new("unused"), true);
        }
        assert!(!timeline.buffering());
        assert_eq!(timeline.warnings().len(), 1);
        assert!((render(&mut source, 160) - 0.125).abs() < 0.0001);
        timeline.enter("main", &node);
        timeline.update(&project, Path::new("unused"), true);
        assert_eq!(timeline.warnings().len(), 1);
        timeline.finish();
        assert_eq!(
            timeline.warnings().len(),
            1,
            "keep errors visible at the end"
        );
    }
    #[test]
    #[ignore = "opens the default Windows audio output device"]
    fn native_output_starts_pauses_resumes_and_stops_all_three_tracks() {
        let mut project = project();
        // A quiet tone allows device smoke checks without playing loud test data.
        for (id, hz) in [("music", 220.0), ("env", 330.0), ("fx", 440.0)] {
            let mut bytes = wav(0, 8000);
            for (i, sample) in bytes[44..].chunks_exact_mut(2).enumerate() {
                let value =
                    ((i as f64 * hz * std::f64::consts::TAU / 8000.0).sin() * 300.0).round() as i16;
                sample.copy_from_slice(&value.to_le_bytes());
            }
            project
                .assets
                .insert(format!("assets/user/audio/{id}/clip.wav"), bytes);
        }
        let mut timeline = Timeline::default();
        timeline.enter("main", &group());
        for _ in 0..3 {
            update(&mut timeline, &project, true);
        }
        assert!(timeline.output.is_some());
        assert_eq!(timeline.voices.len(), 3);
        std::thread::sleep(Duration::from_millis(120));
        assert!(timeline
            .voices
            .iter()
            .all(|voice| voice.sink.get_pos() > Duration::ZERO));
        timeline.pause();
        assert!(timeline.voices.iter().all(|voice| voice.sink.is_paused()));
        update(&mut timeline, &project, true);
        assert!(timeline.voices.iter().all(|voice| !voice.sink.is_paused()));
        timeline.stop();
        assert!(timeline.output.is_none());
        assert!(timeline.voices.is_empty());
    }

    #[test]
    #[ignore = "requires the installed game and built audio components"]
    fn installed_official_audio_decodes_mixes_and_releases_its_cache() {
        let root = crate::components::root().unwrap().unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/embedded-components")
        });
        let game = Path::new("C:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal");
        let data: Value =
            serde_json::from_str(include_str!("../../../data/editor_data.json")).unwrap();
        let (mixer, mut source) = rodio::mixer::mixer(1, 8000);
        let mut timeline = Timeline {
            test_mixer: Some(mixer),
            ..Default::default()
        };
        let mut actions = vec![];
        let mut caches = vec![];
        for (channel, field) in [
            (Channel::Music, "music"),
            (Channel::Sound, "sounds"),
            (Channel::Env, "env_sounds"),
        ] {
            let key = Key {
                channel,
                name: data[field][0]["id"].as_str().unwrap().into(),
            };
            let mut player = audio::Player::preparer();
            player
                .play_official(
                    game,
                    key.kind(),
                    &key.name,
                    &root.join("asset-extractor/lom-asset-extractor.exe"),
                    &root.join("audio-decoder/vgmstream-cli.exe"),
                )
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(45);
            while player.preparing() {
                player.update().unwrap();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            let path = player.prepared_file().unwrap().to_path_buf();
            assert!(std::fs::metadata(&path).unwrap().len() > 44);
            caches.push(path);
            actions.push(
                json!({"type":if channel==Channel::Music {"music"} else {"sound"},
                "kind":key.kind(),"name":key.name}),
            );
            timeline.cache.insert(
                key,
                Asset {
                    player: Some(player),
                    error: None,
                },
            );
        }
        let project = project();
        timeline.enter(
            "main",
            &json!({"id":"mix","type":"parallel","actions":actions}),
        );
        update(&mut timeline, &project, true);
        assert_eq!(timeline.voices.len(), 3);
        assert!(
            (0..16000)
                .map(|_| source.next().unwrap_or(0.0).abs())
                .fold(0.0f32, f32::max)
                > 0.001
        );
        assert!(timeline
            .voices
            .iter()
            .all(|voice| voice.sink.get_pos() > Duration::ZERO));
        timeline.stop();
        assert!(caches.iter().all(|path| !path.exists()));
    }
}
