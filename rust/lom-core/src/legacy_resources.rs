//! Import-time conversion of the old ModSupport portrait and voice directories.
use anyhow::{ensure, Context, Result};
use image::{AnimationDecoder, ImageFormat};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::Path,
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
    io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resources {
    pub handled_files: BTreeSet<String>,
    assets: BTreeMap<String, Vec<u8>>,
    sprites: BTreeMap<String, Value>,
    pub voices: BTreeMap<String, String>,
}

impl Resources {
    pub fn read(root: &Path, files: &[std::path::PathBuf]) -> Result<Self> {
        let mut result = Self::default();
        let mut portrait_names = BTreeSet::new();
        let mut table_images = BTreeSet::new();
        for path in files {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if relative.to_ascii_lowercase().starts_with("datatable/")
                && path
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case("json"))
            {
                let data: Value = serde_json::from_str(&super::read_text(path)?)
                    .with_context(|| format!("DataTable JSON 无效：{relative}"))?;
                collect_image_paths(&data, &mut table_images);
            }
        }
        for path in files {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let lower = relative.to_ascii_lowercase();
            let extension = path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let name = path
                .file_stem()
                .and_then(|v| v.to_str())
                .context("旧素材文件名无效")?;
            if (lower.starts_with("portraits/") || table_images.contains(&lower))
                && ["png", "jpg", "gif"].contains(&extension.as_str())
            {
                ensure!(
                    !lower.starts_with("portraits/") || portrait_names.insert(name.to_owned()),
                    "旧立绘存在同名资源，无法确定优先文件：{name}"
                );
                let index = result.sprites.len();
                let bytes = fs::read(path)?;
                let mut frames = Vec::new();
                if extension == "gif" {
                    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
                    for (frame_index, frame) in decoder.into_frames().enumerate() {
                        let frame = frame.with_context(|| format!("GIF 解码失败：{relative}"))?;
                        let (numerator, denominator) = frame.delay().numer_denom_ms();
                        let duration = f64::from(numerator) / f64::from(denominator) / 1000.0;
                        let mut png = Cursor::new(Vec::new());
                        frame.into_buffer().write_to(&mut png, ImageFormat::Png)?;
                        let id = format!("legacy.p{index:04}_f{frame_index:04}");
                        result.add_content("image", &id, "frame.png", png.into_inner(), name)?;
                        frames.push(json!({"image":format!("user:{id}"),"duration":duration}));
                    }
                    ensure!(!frames.is_empty(), "GIF 没有可播放帧：{relative}");
                } else {
                    // Validate the image while preserving its original encoded pixels.
                    image::load_from_memory(&bytes)
                        .with_context(|| format!("立绘解码失败：{relative}"))?;
                    let id = format!("legacy.p{index:04}_f0000");
                    result.add_content("image", &id, &format!("image.{extension}"), bytes, name)?;
                    frames.push(json!({"image":format!("user:{id}"),"duration":0}));
                }
                let mut config = json!({"frames":frames,"pivot":{"x":0,"y":0},"pixels_per_unit":100,"extrude":0,"sprite_type":1});
                let sidecar = path.with_extension("json");
                // The original GIF loader uses SpriteDesc defaults and ignores sidecars.
                if extension != "gif" && sidecar.is_file() {
                    let data: Value = serde_json::from_str(&super::read_text(&sidecar)?)
                        .with_context(|| format!("旧 SpriteDesc 无效：{}", sidecar.display()))?;
                    if let Some(pivot) = data.get("pivot") {
                        for axis in ["x", "y"] {
                            ensure!(
                                pivot[axis].as_f64().is_some_and(f64::is_finite),
                                "SpriteDesc pivot.{axis} 无效"
                            );
                        }
                        config["pivot"] = pivot.clone();
                    }
                    if let Some(value) = data.get("pixelsPerUnit") {
                        ensure!(
                            value.as_f64().is_some_and(|v| v.is_finite()
                                && v > 0.0
                                && (v as f32).is_finite()),
                            "SpriteDesc pixelsPerUnit 必须为正数"
                        );
                        config["pixels_per_unit"] = value.clone();
                    }
                    if let Some(value) = data.get("extrude") {
                        ensure!(
                            value.as_u64().is_some_and(|v| v <= u32::MAX as u64),
                            "SpriteDesc extrude 无效"
                        );
                        config["extrude"] = value.clone();
                    }
                    if let Some(value) = data.get("spriteType") {
                        config["sprite_type"] = match value.as_str() {
                            Some("FullRect") => json!(0),
                            Some("Tight") => json!(1),
                            _ => {
                                ensure!(value == 0 || value == 1, "SpriteDesc spriteType 无效");
                                value.clone()
                            }
                        };
                    }
                    result.handled_files.insert(
                        sidecar
                            .strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
                if extension == "gif" && sidecar.is_file() {
                    result.handled_files.insert(
                        sidecar
                            .strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
                result.sprites.insert(relative.clone(), config);
                result.handled_files.insert(relative);
            } else if lower.starts_with("voice/") && extension == "mp3" {
                ensure!(
                    !result.voices.contains_key(name),
                    "旧配音存在同名资源：{name}"
                );
                let id = format!("legacy.v{:04}", result.voices.len());
                result.add_content(
                    "audio",
                    &id,
                    "voice.wav",
                    decode_mp3(fs::read(path)?)
                        .with_context(|| format!("MP3 解码失败：{relative}"))?,
                    name,
                )?;
                result.voices.insert(name.into(), format!("user:{id}"));
                result.handled_files.insert(relative);
            }
        }
        Ok(result)
    }

    fn add_content(
        &mut self,
        kind: &str,
        id: &str,
        file: &str,
        bytes: Vec<u8>,
        name: &str,
    ) -> Result<()> {
        ensure!(
            bytes.len()
                <= if kind == "image" {
                    8 * 1024 * 1024
                } else {
                    20 * 1024 * 1024
                },
            "转换后的旧素材 {name} 超过包内素材大小限制"
        );
        let mut metadata = json!({"schema":1,"content_schema":1,"id":id,"type":kind,"name":name,"files":{"main":file}});
        if kind == "audio" {
            metadata["audio_kind"] = json!("sound");
        }
        let metadata = crate::content::normalize_content_metadata(&metadata)?;
        let directory = format!("assets/user/{kind}/{id}");
        self.assets.insert(
            format!("{directory}/content.json"),
            serde_json::to_vec_pretty(&crate::content::content_metadata_payload(&metadata))?,
        );
        self.assets.insert(format!("{directory}/{file}"), bytes);
        Ok(())
    }

    pub fn apply(
        &self,
        project: &mut crate::project::Project,
        strings: &BTreeMap<String, String>,
    ) -> Result<()> {
        if !project.manifest["legacy"].is_object() {
            project.manifest["legacy"] = json!({"schema":1});
        }
        project.manifest["legacy"]["strings"] = json!(strings);
        project.manifest["legacy"]["resources"] =
            json!({"sprites":self.sprites,"voices":self.voices});
        project.assets.extend(self.assets.clone());
        Ok(())
    }
}

fn collect_image_paths(value: &Value, paths: &mut BTreeSet<String>) {
    match value {
        Value::String(path) => {
            let path = path.replace('\\', "/").to_ascii_lowercase();
            let mut relative = path.as_str();
            while let Some(rest) = relative.strip_prefix("./") {
                relative = rest;
            }
            if [".png", ".jpg", ".gif"]
                .iter()
                .any(|extension| path.ends_with(extension))
            {
                paths.insert(relative.to_owned());
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_image_paths(value, paths);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_image_paths(value, paths);
            }
        }
        _ => {}
    }
}

fn decode_mp3(bytes: Vec<u8>) -> Result<Vec<u8>> {
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let mut format = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?
        .format;
    let track = format.default_track().context("MP3 没有音轨")?;
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut pcm = Vec::new();
    let mut specification = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(error) => return Err(error.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let audio = decoder.decode(&packet)?;
        let spec = *audio.spec();
        ensure!(
            spec.channels.count() <= 2 && spec.channels.count() > 0,
            "旧配音仅支持单声道或立体声 MP3"
        );
        ensure!(
            specification.is_none_or(|previous| previous == spec),
            "MP3 中途改变采样格式"
        );
        specification = Some(spec);
        let mut samples = SampleBuffer::<i16>::new(audio.capacity() as u64, spec);
        samples.copy_interleaved_ref(audio);
        for sample in samples.samples() {
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
        ensure!(
            pcm.len() <= 20 * 1024 * 1024 - 44,
            "转换后的 WAV 超过 20 MB"
        );
    }
    let spec = specification.context("MP3 没有音频帧")?;
    ensure!(!pcm.is_empty(), "MP3 没有采样数据");
    let channels = spec.channels.count() as u16;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&spec.rate.to_le_bytes());
    wav.extend_from_slice(&(spec.rate * u32::from(channels) * 2).to_le_bytes());
    wav.extend_from_slice(&(channels * 2).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    Ok(wav)
}

/// Resources referenced solely by legacy runtime metadata still belong in the package.
pub fn references(manifest: &Value) -> Vec<(&str, &str)> {
    let mut result = Vec::new();
    if let Some(sprites) = manifest["legacy"]["resources"]["sprites"].as_object() {
        for sprite in sprites.values() {
            if let Some(frames) = sprite["frames"].as_array() {
                for frame in frames {
                    if let Some(id) = frame["image"]
                        .as_str()
                        .and_then(|v| v.strip_prefix("user:"))
                    {
                        result.push(("image", id));
                    }
                }
            }
        }
    }
    if let Some(voices) = manifest["legacy"]["resources"]["voices"].as_object() {
        for voice in voices.values() {
            if let Some(id) = voice.as_str().and_then(|v| v.strip_prefix("user:")) {
                result.push(("audio", id));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portraits_gif_delays_and_mp3_survive_package_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for folder in ["story", "Portraits", "Voice"] {
            fs::create_dir(root.join(folder)).unwrap();
        }
        fs::write(root.join("story/demo.lua"),"setsaydialog(saydialogs.narrative)\nsetcharacter(narrative)\nsay(luamanager.GetStoryText(\"D_TEST\"))\n").unwrap();
        fs::write(root.join("CustomStory.csv"), "demo,旧资源测试,测试\n").unwrap();
        fs::write(
            root.join("StringTable.csv"),
            "Story/D_TEST,配音测试\nCharacter/player,新名字\n",
        )
        .unwrap();
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for (color, delay) in [([255, 0, 0, 255], 80), ([0, 255, 0, 255], 120)] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(4, 6, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        fs::write(root.join("Portraits/player_normal.gif"), &bytes).unwrap();
        let png = image::RgbaImage::from_pixel(4, 6, image::Rgba([0, 0, 255, 255]));
        png.save(root.join("Portraits/player_sad.png")).unwrap();
        fs::create_dir_all(root.join("DataTable/Book")).unwrap();
        fs::create_dir(root.join("Sprite")).unwrap();
        png.save(root.join("Sprite/player_normal.png")).unwrap();
        fs::write(
            root.join("DataTable/Book/test.json"),
            r#"{"_avatar":"./sprite\\player_normal.PNG"}"#,
        )
        .unwrap();
        image::RgbImage::from_pixel(4, 6, image::Rgb([50, 70, 90]))
            .save(root.join("Portraits/player_happy.jpg"))
            .unwrap();
        let desc = json!({"pivot":{"x":0.25,"y":0.75},"pixelsPerUnit":75,"extrude":2,"spriteType":"FullRect"});
        fs::write(root.join("Portraits/player_sad.json"), desc.to_string()).unwrap();
        fs::write(root.join("Portraits/player_happy.json"), desc.to_string()).unwrap();
        fs::write(
            root.join("Voice/D_TEST.mp3"),
            include_bytes!("../tests/fixtures/legacy-tone.mp3"),
        )
        .unwrap();
        let project = super::super::read(root).unwrap().into_project().unwrap();
        let resources = &project.manifest["legacy"]["resources"];
        assert!(resources["sprites"]["Sprite/player_normal.png"].is_object());
        assert_eq!(
            resources["sprites"]["Portraits/player_normal.gif"]["frames"][0]["duration"],
            0.08
        );
        assert_eq!(
            resources["sprites"]["Portraits/player_normal.gif"]["frames"][1]["duration"],
            0.12
        );
        for filename in ["player_sad.png", "player_happy.jpg"] {
            let config = &resources["sprites"][format!("Portraits/{filename}")];
            assert_eq!(config["pivot"], desc["pivot"]);
            assert_eq!(config["pixels_per_unit"], 75);
            assert_eq!(config["extrude"], 2);
            assert_eq!(config["sprite_type"], 0);
        }
        assert_eq!(
            project.manifest["legacy"]["strings"]["Character/player"],
            "新名字"
        );
        let say = project.stories["demo"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["type"] == "say")
            .unwrap();
        assert_eq!(say["text"], "配音测试");
        assert_eq!(say["voice"], resources["voices"]["D_TEST"]);
        let wav = project
            .assets
            .iter()
            .find(|(name, _)| name.ends_with(".wav"))
            .unwrap()
            .1;
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1);
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 22050);
        assert!(wav.len() > 22050 / 5 * 2);
        let output = tempfile::tempdir().unwrap();
        let package = output.path().join("resource-test.lommod");
        project.export(&package).unwrap();
        let reopened = crate::project::Project::open(&package).unwrap();
        for (path, data) in &project.assets {
            let restored = reopened.assets.get(path).expect("missing packaged asset");
            if path.ends_with("content.json") {
                assert_eq!(
                    serde_json::from_slice::<Value>(restored).unwrap(),
                    serde_json::from_slice::<Value>(data).unwrap()
                );
            } else {
                assert_eq!(restored, data, "changed packaged asset {path}");
            }
        }
        assert_eq!(
            fs::read(root.join("Portraits/player_normal.gif")).unwrap(),
            bytes
        );
    }
}
