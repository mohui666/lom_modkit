use crate::unity::{self, Reader};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{Rgba, RgbaImage};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use unity_rs_core::studio::{Studio, StudioObject};

#[derive(Clone)]
pub struct Address {
    pub bundle: String,
    pub internal: String,
}
pub fn catalog(path: &Path) -> Result<BTreeMap<String, Address>> {
    let data: Value = serde_json::from_slice(&fs::read(path)?)?;
    let decode = |name: &str| -> Result<Vec<u8>> {
        Ok(STANDARD.decode(data[name].as_str().context("素材目录缺少字段")?)?)
    };
    let kd = decode("m_KeyDataString")?;
    let bd = decode("m_BucketDataString")?;
    let ed = decode("m_EntryDataString")?;
    let mut r = Reader::new(&bd);
    let mut buckets = vec![];
    for _ in 0..r.count(2_000_000)? {
        let offset = r.u32()? as usize;
        let count = r.count(2_000_000)?;
        let entries = (0..count).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
        buckets.push((offset, entries));
    }
    let keys = buckets
        .iter()
        .map(|(offset, _)| -> Result<Option<String>> {
            let mut r = Reader::at(&kd, *offset);
            let kind = r.u8()?;
            if kind > 1 {
                return Ok(None);
            }
            let len = r.count(kd.len())?;
            let bytes = r.bytes(len)?;
            Ok(Some(if kind == 0 {
                String::from_utf8(bytes.to_vec())?
            } else {
                ensure!(bytes.len() % 2 == 0, "素材目录 UTF16 长度无效");
                String::from_utf16(
                    &bytes
                        .chunks_exact(2)
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect::<Vec<_>>(),
                )?
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut r = Reader::new(&ed);
    let mut entries = vec![];
    for _ in 0..r.count(2_000_000)? {
        let mut entry = [0; 7];
        for item in &mut entry {
            *item = r.i32()?;
        }
        entries.push(entry);
    }
    let mut result = BTreeMap::new();
    for (i, (_, items)) in buckets.iter().enumerate() {
        let Some(key) = &keys[i] else { continue };
        for index in items {
            let entry = entries.get(*index as usize).context("素材目录索引越界")?;
            if entry[2] < 0 {
                continue;
            }
            if let Some(Some(bundle)) = keys.get(entry[2] as usize) {
                if bundle.ends_with(".bundle") {
                    let internal = data["m_InternalIds"]
                        .get(entry[0] as usize)
                        .and_then(Value::as_str)
                        .context("素材内部地址无效")?;
                    result.entry(key.clone()).or_insert(Address {
                        bundle: bundle.clone(),
                        internal: internal.into(),
                    });
                }
            }
        }
    }
    Ok(result)
}

pub fn sprite<'a>(studio: &'a Studio, address: &str) -> Result<StudioObject<'a>> {
    let normalized = address.replace('\\', "/").to_lowercase();
    let exact: Vec<_> = studio
        .objects()
        .filter(|o| {
            o.class_id() == 213
                && o.container()
                    .is_some_and(|x| x.replace('\\', "/").to_lowercase() == normalized)
        })
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    ensure!(exact.is_empty(), "素材地址不唯一：{address}");
    ensure!(!normalized.contains('/'), "素材包内找不到地址：{address}");
    let name = Path::new(address)
        .file_stem()
        .and_then(|s| s.to_str())
        .context("素材名称无效")?;
    unity::unique(studio, 213, name)
}
pub fn decode(studio: &Studio, object: StudioObject<'_>, canvas: bool) -> Result<RgbaImage> {
    let metadata = object.read_sprite(Default::default())?;
    let decoded = object.decode_sprite(Default::default(), Default::default())?;
    let image = RgbaImage::from_raw(decoded.width, decoded.height, decoded.pixels)
        .context("图片像素长度无效")?;
    if !canvas {
        return Ok(image);
    }
    let mut offset = metadata.render_data.texture_rect_offset;
    if let Some(key) = metadata.render_data_key {
        let atlas = if !metadata.sprite_atlas.is_null() {
            Some(unity::resolve(
                studio,
                object,
                unity_rs_core::serialized::ObjectReference {
                    file_id: metadata.sprite_atlas.file_id,
                    path_id: metadata.sprite_atlas.path_id,
                },
            )?)
        } else {
            studio.objects().find(|o| {
                o.class_id() == 687078895
                    && o.name()
                        .is_some_and(|name| metadata.atlas_tags.iter().any(|x| x == name))
            })
        };
        if let Some(atlas) = atlas {
            let atlas = atlas.read_sprite_atlas(Default::default())?;
            let data = atlas.render_data(&key.into()).context("图集缺少精灵数据")?;
            offset.x = data.texture_rect_offset.x;
            offset.y = data.texture_rect_offset.y;
        }
    }
    let (w, h) = (
        metadata.rect.width.round_ties_even(),
        metadata.rect.height.round_ties_even(),
    );
    ensure!(
        w > 0.0 && h > 0.0 && w * h <= 128.0 * 1024.0 * 1024.0,
        "图片画布尺寸无效"
    );
    let mut canvas = RgbaImage::new(w as u32, h as u32);
    // Replace RGBA bytes, without alpha blending a second time.
    image::imageops::replace(
        &mut canvas,
        &image,
        offset.x.round_ties_even() as i64,
        h as i64 - offset.y.round_ties_even() as i64 - image.height() as i64,
    );
    Ok(canvas)
}
pub fn save(image: &RgbaImage, path: &Path, max_size: Option<u32>) -> Result<()> {
    fs::create_dir_all(path.parent().context("图片路径无效")?)?;
    let resized = max_size
        .filter(|&max| image.width() > max || image.height() > max)
        .map(|max| {
            let factor = max as f64 / image.width().max(image.height()) as f64;
            image::imageops::resize(
                image,
                (image.width() as f64 * factor).round().max(1.0) as u32,
                (image.height() as f64 * factor).round().max(1.0) as u32,
                image::imageops::FilterType::Lanczos3,
            )
        });
    let temp = path.with_extension("tmp");
    resized
        .as_ref()
        .unwrap_or(image)
        .save_with_format(&temp, image::ImageFormat::Png)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn character(studio: &Studio, object: StudioObject<'_>) -> Result<(String, Value)> {
    let bytes = unity::raw(object)?;
    let mut r = Reader::at(&bytes, 28);
    let name = r.string()?;
    let mood = [r.f32()?, r.f32()?];
    let id = unity::mapping(studio, object, r.ptr()?)?;
    let count = r.count(127)?;
    ensure!(count > 0, "人物没有立绘");
    let mut portraits = serde_json::Map::new();
    let mut first = String::new();
    for i in 0..count {
        let mapping = r.ptr()?;
        let address = r.string()?;
        if i == 0 {
            first = address.clone();
        }
        let emotion = unity::mapping(studio, object, mapping)?;
        portraits.entry(emotion).or_insert(Value::String(address));
    }
    ensure!(!id.is_empty(), "人物缺少编号");
    Ok((
        id,
        json!({"name":name,"mood_position":mood,"first_address":first,"addresses":portraits}),
    ))
}
fn addressable(
    studio: &Studio,
    owner: StudioObject<'_>,
    ptr: unity_rs_core::serialized::ObjectReference,
) -> Result<(String, String)> {
    let bytes = unity::raw(unity::resolve(studio, owner, ptr)?)?;
    let mut r = Reader::at(&bytes, 28);
    r.string()?;
    let key = r.string()?;
    let address = r.string()?;
    ensure!(!key.is_empty() && !address.is_empty(), "背景条目为空");
    Ok((key, address))
}
pub fn extract(game: &Path, output: &Path, max_size: u32) -> Result<Value> {
    let game = game.canonicalize()?;
    let output = output.canonicalize()?;
    ensure!(
        !output.starts_with(&game),
        "请选择游戏文件夹之外的素材缓存目录"
    );
    ensure!(
        fs::read_dir(&output)?.next().is_none(),
        "提取目录必须为空，已有素材不会被覆盖"
    );
    ensure!(
        (128..=1600).contains(&max_size),
        "预览尺寸应在 128 至 1600 之间"
    );
    let data = game.join("Mortal_Data");
    let aa = data.join("StreamingAssets/aa");
    let bundles = ["StandaloneWindows64", "StandaloneWindows"]
        .into_iter()
        .map(|x| aa.join(x))
        .find(|p| p.is_dir())
        .context("游戏目录缺少图片素材包")?
        .canonicalize()?;
    crate::emit(
        "progress",
        json!({"message":"正在读取人物、表情、背景和插图目录","done":0,"total":0}),
    );
    let addresses = catalog(&aa.join("catalog.json"))?;
    let studio = unity::open(&data.join("sharedassets2.assets"))?;
    let mut characters = serde_json::Map::new();
    let mut views = BTreeMap::new();
    for obj in studio.objects().filter(|o| {
        o.class_id() == 114 && unity::basename(o.source_path()) == "sharedassets2.assets"
    }) {
        let name = obj.name().unwrap_or("");
        if name == "StoryCharacterConfig" {
            let bytes = unity::raw(obj)?;
            let mut r = Reader::at(&bytes, 28);
            r.string()?;
            for _ in 0..r.count(10000)? {
                let entry = unity::resolve(&studio, obj, r.ptr()?)?;
                let (id, value) = character(&studio, entry)?;
                characters.entry(id).or_insert(value);
            }
        } else if name == "唐門_主角_自戀" {
            let (id, value) = character(&studio, obj)?;
            ensure!(id == "player", "自恋主角配置编号已改变");
            characters.insert("player_beautified".into(), value);
        } else if name == "_StoryViewData" {
            let bytes = unity::raw(obj)?;
            let mut r = Reader::at(&bytes, 28);
            r.string()?;
            for _ in 0..r.count(10000)? {
                let pointer = r.ptr()?;
                if pointer.is_null() {
                    continue;
                }
                let (k, v) = addressable(&studio, obj, pointer)?;
                views.insert(k, v);
            }
        }
    }
    ensure!(
        !characters.is_empty() && !views.is_empty(),
        "没有读到人物或背景目录，请核对当前游戏版本"
    );
    let pictures: Vec<_> = addresses
        .iter()
        .filter(|(key, _)| key.starts_with("pic_") && key.len() > 4)
        .map(|(k, a)| {
            let path = a.internal.replace('\\', "/");
            let category = path.rsplit('/').nth(1).unwrap_or("");
            json!({"key":&k[4..],"category":category})
        })
        .collect();
    let mut map = json!({"portrait_canvas":"sprite_rect","characters":{},"views":{},"pictures":{},"picture_catalog":pictures});
    let mut tasks = BTreeMap::<(String, bool), (String, Vec<(String, String, String)>)>::new();
    for (id, c) in characters {
        let mut portraits = c["addresses"]
            .as_object()
            .context("人物立绘表无效")?
            .clone();
        let first = portraits
            .iter()
            .find(|(_, v)| **v == c["first_address"])
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| "first".into());
        portraits
            .entry(first.clone())
            .or_insert(c["first_address"].clone());
        map["characters"][&id] = json!({"name":c["name"],"mood_position":c["mood_position"],"first":first,"portraits":{}});
        for (emotion, address) in portraits {
            add_task(
                &mut tasks,
                address.as_str().context("立绘地址无效")?,
                true,
                "portraits",
                ("character".into(), id.clone(), emotion),
            );
        }
    }
    for (key, address) in views {
        add_task(
            &mut tasks,
            &address,
            false,
            "views",
            ("views".into(), String::new(), key),
        );
    }
    for picture in &pictures {
        let key = picture["key"].as_str().unwrap();
        add_task(
            &mut tasks,
            &format!("pic_{key}"),
            false,
            "pictures",
            ("pictures".into(), String::new(), key.into()),
        );
    }
    let mut failures = vec![];
    let mut succeeded = 0;
    let total = tasks.len();
    let mut cache: Vec<(String, Studio)> = vec![];
    for (i, ((address, portrait), (relative, targets))) in tasks.into_iter().enumerate() {
        let result = (|| -> Result<()> {
            let entry = addresses
                .get(&address)
                .context("素材目录中没有对应的素材包")?;
            if !cache.iter().any(|(name, _)| *name == entry.bundle) {
                let path = bundles.join(&entry.bundle).canonicalize()?;
                ensure!(path.starts_with(&bundles), "素材包路径超出游戏资源目录");
                if cache.len() == 2 {
                    cache.remove(0);
                }
                cache.push((entry.bundle.clone(), Studio::open(path)?));
            }
            let (_, environment) = cache
                .iter()
                .find(|(name, _)| *name == entry.bundle)
                .unwrap();
            let image = decode(environment, sprite(environment, &entry.internal)?, portrait)?;
            save(&image, &output.join(&relative), Some(max_size))?;
            for (group, id, key) in targets {
                if group == "character" {
                    map["characters"][&id]["portraits"][&key] = json!(relative);
                } else {
                    map[&group][&key] = json!(relative);
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => succeeded += 1,
            Err(error) => failures.push(json!({"address":address,"error":format!("{error:#}")})),
        }
        if i == 0 || (i + 1) % 10 == 0 || i + 1 == total {
            crate::emit(
                "progress",
                json!({"message":"正在导出立绘、表情、背景和插图","done":i+1,"total":total,"failed":failures.len()}),
            );
        }
    }
    drop(cache);
    ensure!(
        succeeded > 0,
        "未能导出任何游戏图片：{}",
        failures
            .iter()
            .take(3)
            .map(|x| x["error"].as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("；")
    );
    let unavailable: Vec<_> = map["characters"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, c)| c["portraits"].get(c["first"].as_str().unwrap()).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in &unavailable {
        map["characters"].as_object_mut().unwrap().remove(id);
    }
    for (name, color) in [("black", 0), ("white", 255)] {
        let relative = format!("assets/views/{name}.png");
        save(
            &RgbaImage::from_pixel(1920, 1080, Rgba([color, color, color, 255])),
            &output.join(&relative),
            None,
        )?;
        map["views"][name] = json!(relative);
    }
    crate::emit(
        "progress",
        json!({"message":"正在导出气泡动画与人物气泡位置","done":0,"total":0}),
    );
    map["moods"] = crate::moods::extract(&studio, &output)?;
    let book = unity::unique(&studio, 213, "pic_book_1")?;
    save(
        &decode(&studio, book, false)?,
        &output.join("assets/ui/event_book.png"),
        None,
    )?;
    map["event_book"] = json!("assets/ui/event_book.png");
    let report = json!({"exported":succeeded,"failed":failures.len(),"failures":failures,"resource_platform":bundles.file_name().unwrap().to_string_lossy(),"unavailable_characters":unavailable,
        "characters":map["characters"].as_object().unwrap().len(),"views":map["views"].as_object().unwrap().len(),"pictures":map["pictures"].as_object().unwrap().len(),"moods":map["moods"].as_object().unwrap().len(),"game_tested":false,"engine":"rust"});
    fs::write(
        output.join("preview_map.json"),
        serde_json::to_vec_pretty(&map)?,
    )?;
    fs::write(
        output.join("extract_report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
type Tasks = BTreeMap<(String, bool), (String, Vec<(String, String, String)>)>;
fn add_task(
    tasks: &mut Tasks,
    address: &str,
    portrait: bool,
    folder: &str,
    target: (String, String, String),
) {
    let digest = format!("{:x}", Sha256::digest(address.as_bytes()));
    tasks
        .entry((address.into(), portrait))
        .or_insert_with(|| (format!("assets/{folder}/{digest}.png"), vec![]))
        .1
        .push(target);
}
