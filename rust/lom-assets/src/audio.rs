use crate::{stories, unity::Reader};
use anyhow::{ensure, Context, Result};
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
const UNSUPPORTED: &str = "此音频事件使用了尚不支持的动态播放规则，请在游戏中确认";
fn require(ok: bool) -> Result<()> {
    ensure!(ok, "{UNSUPPORTED}");
    Ok(())
}
#[derive(Clone, Debug, Serialize)]
pub struct Parameters {
    parent: u32,
    values: BTreeMap<u8, f32>,
    ranges: BTreeMap<u8, (f32, f32)>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    pub media_id: u32,
    pub weight: i32,
    pub parameters: Vec<Parameters>,
}
fn base(r: &mut Reader<'_>) -> Result<Parameters> {
    for _ in 0..2 {
        require(r.u8()? <= 1 && r.u8()? == 0)?;
    }
    require(r.u8()? == 0)?;
    r.u32()?;
    let parent = r.u32()?;
    r.u8()?;
    let count = r.u8()? as usize;
    let ids = r.bytes(count)?.to_vec();
    require(
        ids.iter().all(|&v| v == 0 || v == 2) && ids.iter().collect::<BTreeSet<_>>().len() == count,
    )?;
    let mut values = BTreeMap::new();
    for key in ids {
        let value = r.f32()?;
        require(value.is_finite())?;
        values.insert(key, value);
    }
    let count = r.u8()? as usize;
    let ids = r.bytes(count)?.to_vec();
    require(
        ids.iter().all(|&v| v == 0 || v == 2) && ids.iter().collect::<BTreeSet<_>>().len() == count,
    )?;
    let mut ranges = BTreeMap::new();
    for key in ids {
        let low = r.f32()?;
        let high = r.f32()?;
        require(low.is_finite() && high.is_finite() && low <= high)?;
        ranges.insert(key, (low, high));
    }
    let positioning = r.u8()?;
    require(positioning == 0 || positioning == 3)?;
    if positioning == 3 {
        require(r.u8()? == 8)?;
    }
    require(r.u8()? == 0 && r.u32()? == 0)?;
    r.skip(5)?;
    require(r.u8()? == 0 && r.u8()? == 0 && r.u8()? == 0 && r.u16()? == 0)?;
    Ok(Parameters {
        parent,
        values,
        ranges,
    })
}
fn chunk_header(file: &mut File) -> Result<Option<([u8; 4], u32)>> {
    let mut bytes = [0; 8];
    let n = file.read(&mut bytes)?;
    if n == 0 {
        return Ok(None);
    }
    file.read_exact(&mut bytes[n..])
        .context("音频包区块头不完整")?;
    Ok(Some((
        bytes[..4].try_into()?,
        u32::from_le_bytes(bytes[4..].try_into()?),
    )))
}
pub fn choices(bank: &Path, event_id: u32, declared: &BTreeSet<u32>) -> Result<Vec<Choice>> {
    let mut file = File::open(bank)?;
    let mut version = None;
    let mut objects = BTreeMap::new();
    while let Some((tag, len)) = chunk_header(&mut file)? {
        if &tag != b"BKHD" && &tag != b"HIRC" {
            file.seek(SeekFrom::Current(len as i64))?;
            continue;
        }
        ensure!(len <= 64 * 1024 * 1024, "音频播放规则过大");
        let mut data = vec![0; len as usize];
        file.read_exact(&mut data)?;
        let mut r = Reader::new(&data);
        if &tag == b"BKHD" {
            version = Some(r.u32()?);
        } else {
            for _ in 0..r.count(1_000_000)? {
                let kind = r.u8()?;
                let len = r.u32()? as usize;
                require(len >= 4)?;
                let payload = r.bytes(len)?.to_vec();
                let id = Reader::new(&payload).u32()?;
                ensure!(
                    objects.insert(id, (kind, payload)).is_none(),
                    "音频包对象编号重复"
                );
            }
            r.end()?;
        }
    }
    require(version == Some(140))?;
    let node = |id: u32, expected: u8| -> Result<Reader<'_>> {
        let (kind, data) = objects.get(&id).context("音频事件引用的播放对象不存在")?;
        require(*kind == expected)?;
        let mut r = Reader::new(data);
        require(r.u32()? == id)?;
        Ok(r)
    };
    let mut event = node(event_id, 4)?;
    require(event.u8()? == 1)?;
    let action_id = event.u32()?;
    event.end()?;
    let mut action = node(action_id, 3)?;
    require(action.u16()? == 0x0403)?;
    let random_id = action.u32()?;
    require(action.bytes(3)? == [0, 0, 0] && action.u8()? == 4)?;
    action.u32()?;
    action.end()?;
    let mut container = node(random_id, 5)?;
    let mut inherited = vec![base(&mut container)?];
    require(container.u16()? == 1 && container.u16()? == 0 && container.u16()? == 0)?;
    container.skip(12)?;
    container.u16()?;
    let transition = container.u8()?;
    let random = container.u8()?;
    let mode = container.u8()?;
    let flags = container.u8()?;
    require(transition == 0 && random <= 1 && mode == 0 && flags & 8 == 0)?;
    let count = container.count(1_000_000)?;
    let children = (0..count)
        .map(|_| container.u32())
        .collect::<Result<BTreeSet<_>>>()?;
    let count = container.u16()?;
    let playlist = (0..count)
        .map(|_| Ok((container.u32()?, container.i32()?)))
        .collect::<Result<Vec<_>>>()?;
    container.end()?;
    require(
        !playlist.is_empty()
            && playlist.iter().map(|x| x.0).collect::<BTreeSet<_>>() == children
            && playlist.iter().all(|x| x.1 >= 0),
    )?;
    let mut parent = inherited[0].parent;
    let mut current = random_id;
    let mut visited = BTreeSet::from([current]);
    while parent != 0 {
        require(visited.insert(parent))?;
        let mut mixer = node(parent, 7)?;
        let parameters = base(&mut mixer)?;
        let count = mixer.count(1_000_000)?;
        let children = (0..count)
            .map(|_| mixer.u32())
            .collect::<Result<Vec<_>>>()?;
        require(children.contains(&current))?;
        mixer.end()?;
        current = parent;
        parent = parameters.parent;
        inherited.push(parameters);
    }
    let mut choices = vec![];
    for (id, weight) in playlist {
        let mut sound = node(id, 2)?;
        require(sound.u32()? & 15 == 1 && sound.u8()? == 0)?;
        let media_id = sound.u32()?;
        sound.u32()?;
        require(sound.u8()? == 0)?;
        let parameters = base(&mut sound)?;
        sound.end()?;
        require(parameters.parent == random_id && declared.contains(&media_id))?;
        let mut all = inherited.clone();
        all.push(parameters);
        choices.push(Choice {
            media_id,
            weight: if flags & 1 != 0 { weight } else { 1 },
            parameters: all,
        });
    }
    require(
        choices.iter().map(|x| x.media_id).collect::<BTreeSet<_>>() == *declared
            && choices.iter().any(|x| x.weight > 0),
    )?;
    Ok(choices)
}
pub fn extract_media(bank: &Path, id: u32, output: &Path) -> Result<()> {
    let mut file = File::open(bank)?;
    let mut index = None;
    while let Some((tag, len)) = chunk_header(&mut file)? {
        if &tag == b"DIDX" {
            ensure!(len % 12 == 0 && len <= 12_000_000, "音频包索引长度无效");
            let mut data = vec![0; len as usize];
            file.read_exact(&mut data)?;
            index = Some(data);
        } else if &tag == b"DATA" && index.is_some() {
            let index = index.unwrap();
            let mut r = Reader::new(&index);
            while r.at < index.len() {
                let ident = r.u32()?;
                let offset = r.u32()?;
                let size = r.u32()?;
                if ident != id {
                    continue;
                }
                ensure!(
                    size > 0 && (offset as u64 + size as u64) <= len as u64,
                    "音频包媒体范围无效"
                );
                file.seek(SeekFrom::Current(offset as i64))?;
                let mut data = vec![0; size as usize];
                file.read_exact(&mut data)?;
                ensure!(
                    data.starts_with(b"RIFF") || data.starts_with(b"RIFX"),
                    "音频包媒体格式不支持"
                );
                let mut dest = File::options().write(true).create_new(true).open(output)?;
                dest.write_all(&data)?;
                return Ok(());
            }
            anyhow::bail!("音频包中没有对应的媒体文件");
        } else {
            file.seek(SeekFrom::Current(len as i64))?;
        }
    }
    anyhow::bail!("音频包缺少媒体索引或数据")
}
pub fn extract(game: &Path, kind: &str, name: &str, output: &Path) -> Result<Value> {
    let game = game.canonicalize()?;
    let output = output
        .parent()
        .context("试听缓存路径无效")?
        .canonicalize()?
        .join(output.file_name().context("试听缓存文件名无效")?);
    ensure!(!output.starts_with(&game), "试听缓存必须保存在游戏目录之外");
    ensure!(
        !output.exists() && !output.with_extension("txtp").exists(),
        "试听输出文件已存在"
    );
    let key = stories::audio_key(&game, kind, name)?;
    let (bank_name, prefix) = match kind {
        "music" => ("Music", "Music_"),
        "sound" => ("SFX", "Sound_"),
        "env" => ("Env", "Env_"),
        _ => anyhow::bail!("试听音频类型无效"),
    };
    let folder = game.join("Mortal_Data/StreamingAssets/Audio/GeneratedSoundBanks/Windows");
    let text = fs::read_to_string(folder.join("SoundbanksInfo.xml"))?;
    let xml = roxmltree::Document::parse(&text)?;
    let bank = xml
        .descendants()
        .find(|n| {
            n.has_tag_name("SoundBank")
                && n.children()
                    .any(|n| n.has_tag_name("ShortName") && n.text() == Some(bank_name))
        })
        .context("没有找到音频包")?;
    let event_name = format!("{prefix}{key}");
    let event = bank
        .children()
        .find(|n| n.has_tag_name("IncludedEvents"))
        .and_then(|n| {
            n.children().find(|n| {
                n.has_tag_name("Event") && n.attribute("Name") == Some(event_name.as_str())
            })
        })
        .with_context(|| format!("没有找到音频事件：{event_name}"))?;
    let ids = event
        .children()
        .find(|n| n.has_tag_name("IncludedMemoryFiles"))
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("File"))
        .map(|n| Ok(n.attribute("Id").context("媒体缺少编号")?.parse()?))
        .collect::<Result<BTreeSet<u32>>>()?;
    let bank = folder.join(format!("{bank_name}.bnk"));
    if ids.len() == 1 {
        extract_media(&bank, *ids.first().unwrap(), &output)?;
        return Ok(json!({"name":name,"kind":kind,"event_name":event_name}));
    }
    let choices = choices(
        &bank,
        event.attribute("Id").context("事件缺少编号")?.parse()?,
        &ids,
    )?;
    let total: u64 = choices.iter().map(|x| x.weight as u64).sum();
    let mut rng = rand::rng();
    let mut pick = rng.random_range(0..total);
    let choice = choices
        .iter()
        .find(|x| {
            if pick < x.weight as u64 {
                true
            } else {
                pick -= x.weight as u64;
                false
            }
        })
        .context("音频随机权重无效")?;
    let (mut volume, mut pitch) = (0.0f64, 0.0f64);
    for parameters in &choice.parameters {
        volume += *parameters.values.get(&0).unwrap_or(&0.0) as f64;
        pitch += *parameters.values.get(&2).unwrap_or(&0.0) as f64;
        for (key, (low, high)) in &parameters.ranges {
            let value = rng.random_range(*low as f64..=*high as f64);
            if *key == 0 {
                volume += value;
            } else {
                pitch += value;
            }
        }
    }
    extract_media(&bank, choice.media_id, &output)?;
    write_config(&output, volume, pitch)?;
    Ok(
        json!({"name":name,"kind":kind,"event_name":event_name,"preview_mode":"random","variant_count":choices.len(),"media_id":choice.media_id,"volume_db":volume,"pitch_cents":pitch}),
    )
}
fn write_config(output: &Path, volume: f64, pitch: f64) -> Result<()> {
    let bytes = fs::read(output)?;
    require(bytes.starts_with(b"RIFF"))?;
    let mut r = Reader::at(&bytes, 12);
    let mut rate = None;
    while r.at < bytes.len() {
        let tag = r.bytes(4)?;
        let size = r.u32()? as usize;
        if tag == b"fmt " {
            require(size >= 16)?;
            r.skip(4)?;
            rate = Some(r.u32()?);
            break;
        }
        r.skip(size + (size & 1))?;
    }
    let rate = rate.context("音频没有采样率")?;
    let gain = 10f64.powf(volume / 20.0);
    let adjusted = (rate as f64 * 2f64.powf(pitch / 1200.0)).round_ties_even();
    require(gain.is_finite() && gain > 0.0 && adjusted > 0.0 && adjusted.is_finite())?;
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(output.with_extension("txtp"))?;
    writeln!(
        file,
        "{} #i #v {gain:.9} #h {adjusted:.0}",
        output.file_name().unwrap().to_string_lossy()
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    #[test]
    fn random_branches_preserve_weights_parent_gain_pitch_and_reject_dynamic_mixes() -> Result<()> {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/fixtures/wwise140.json"))?;
        let directory = tempfile::tempdir()?;
        let bank = directory.path().join("synthetic.bnk");
        for case in cases {
            let original = STANDARD.decode(case["data"].as_str().unwrap())?;
            fs::write(&bank, &original)?;
            let parsed = choices(&bank, 12, &BTreeSet::from([7, 8]));
            if case["expected"].is_null() {
                assert!(parsed.unwrap_err().to_string().contains("动态播放规则"));
                continue;
            }
            let parsed = parsed?;
            assert_eq!(serde_json::to_value(&parsed)?, case["expected"]);
            let output = directory.path().join("sample.wem");
            extract_media(&bank, parsed[0].media_id, &output)?;
            assert!(fs::read(&output)?.starts_with(b"RIFF"));
            write_config(&output, -3.0, 200.0)?;
            let config = fs::read_to_string(output.with_extension("txtp"))?;
            assert!(config.contains("#h 17959") && config.contains("#v 0.707945784"));
            assert!(
                extract_media(&bank, 7, &output).is_err(),
                "never overwrite an existing output"
            );
            assert!(choices(&bank, 12, &BTreeSet::from([7, 9])).is_err());
            assert_eq!(fs::read(&bank)?, original, "game bank stays unchanged");
        }
        Ok(())
    }
}
