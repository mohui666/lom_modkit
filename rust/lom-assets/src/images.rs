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

struct SpriteIndex<'a> {
    // A present key with no object is ambiguous, even if a later file also has the key.
    addresses: BTreeMap<String, Option<StudioObject<'a>>>,
    names: BTreeMap<String, Option<StudioObject<'a>>>,
}

impl<'a> SpriteIndex<'a> {
    fn new(studio: &'a Studio) -> Self {
        let mut addresses = BTreeMap::new();
        let mut names = BTreeMap::new();
        for object in studio.objects().filter(|object| object.class_id() == 213) {
            if let Some(address) = object.container() {
                addresses
                    .entry(address.replace('\\', "/").to_lowercase())
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(object));
            }
            if let Some(name) = object.name() {
                names
                    .entry(name.to_owned())
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(object));
            }
        }
        Self { addresses, names }
    }

    fn get(&self, address: &str) -> Result<StudioObject<'a>> {
        let normalized = address.replace('\\', "/").to_lowercase();
        if let Some(object) = self.addresses.get(&normalized) {
            return object.with_context(|| format!("素材地址不唯一：{address}"));
        }
        ensure!(!normalized.contains('/'), "素材包内找不到地址：{address}");
        let name = Path::new(address)
            .file_stem()
            .and_then(|s| s.to_str())
            .context("素材名称无效")?;
        self.names
            .get(name)
            .with_context(|| format!("找不到资源 {name}"))?
            .with_context(|| format!("资源名称不唯一：{name}"))
    }
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

/// Source Sprite geometry is distinct from the downscaled PNG and from the
/// Story stage's Transform. None of these values classify the drawn anatomy.
fn source_geometry(
    studio: &Studio,
    object: StudioObject<'_>,
    restored_canvas: bool,
) -> Result<Value> {
    let sprite = object.read_sprite(Default::default())?;
    let data = &sprite.render_data;
    let mut rect = [
        data.texture_rect.x,
        data.texture_rect.y,
        data.texture_rect.width,
        data.texture_rect.height,
    ];
    let mut offset = [data.texture_rect_offset.x, data.texture_rect_offset.y];
    let mut rotation = data.settings.packing_rotation;
    if let Some(key) = sprite.render_data_key {
        let atlas = if !sprite.sprite_atlas.is_null() {
            Some(unity::resolve(
                studio,
                object,
                unity_rs_core::serialized::ObjectReference {
                    file_id: sprite.sprite_atlas.file_id,
                    path_id: sprite.sprite_atlas.path_id,
                },
            )?)
        } else {
            studio.objects().find(|o| {
                o.class_id() == 687078895
                    && o.name()
                        .is_some_and(|name| sprite.atlas_tags.iter().any(|tag| tag == name))
            })
        };
        if let Some(atlas) = atlas {
            let atlas = atlas.read_sprite_atlas(Default::default())?;
            let data = atlas
                .render_data(&key.into())
                .context("图集缺少精灵几何数据")?;
            rect = [
                data.texture_rect.x,
                data.texture_rect.y,
                data.texture_rect.width,
                data.texture_rect.height,
            ];
            offset = [data.texture_rect_offset.x, data.texture_rect_offset.y];
            rotation = data.settings.packing_rotation();
        }
    }
    Ok(
        json!({"original_canvas":[sprite.rect.width,sprite.rect.height],
        "sprite_pivot":{"normalized_bottom_left":[sprite.pivot.x,sprite.pivot.y],"pixels_per_unit":sprite.pixels_to_units},
        "crop":{"canvas_restored":restored_canvas,"atlas_rect":rect,"texture_rect_offset":offset,
            "offset_coordinate_space":"bottom_left_original_canvas","packing_rotation":rotation},
        "source":"Unity.Sprite","body_framing":"unknown"}),
    )
}

fn target_geometry<'a>(map: &'a Value, group: &str, id: &str, key: &str) -> &'a Value {
    if group == "character" {
        &map["characters"][id]["portrait_metadata"][key]
    } else if group == "views" {
        &map["view_metadata"][key]
    } else if group == "combat_backgrounds" {
        &map["combat_background_metadata"][key]
    } else {
        &map["picture_metadata"][key]
    }
}

fn set_geometry(map: &mut Value, group: &str, id: &str, key: &str, geometry: &Value) {
    let mut geometry = geometry.clone();
    if let Some(framing) = target_geometry(map, group, id, key)["body_framing"]
        .as_str()
        .filter(|value| matches!(*value, "half_body" | "full_body"))
    {
        geometry["body_framing"] = json!(framing);
    }
    if group == "character" {
        map["characters"][id]["portrait_metadata"][key] = geometry;
    } else if group == "views" {
        map["view_metadata"][key] = geometry;
    } else if group == "combat_backgrounds" {
        map["combat_background_metadata"][key] = geometry;
    } else {
        map["picture_metadata"][key] = geometry;
    }
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
/// Reuse only explicitly mapped files. Missing files are extracted again; unsafe
/// paths are errors even when the referenced file no longer exists.
pub(crate) fn cached_file(root: Option<&Path>, value: &Value) -> Result<Option<String>> {
    let Some(root) = root else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let name = value.as_str().context("已有图片路径格式不正确")?;
    lom_core::package::canonical_archive_name(name)?;
    ensure!(name.starts_with("assets/"), "已有图片路径超出素材目录");
    let path = root.join(name);
    let mut ancestor = path.as_path();
    while !ancestor.try_exists()? {
        ancestor = ancestor.parent().context("已有图片路径无效")?;
    }
    ensure!(
        ancestor.canonicalize()?.starts_with(root.canonicalize()?),
        "已有图片路径指向素材库之外：{name}"
    );
    if !path.try_exists()? {
        return Ok(None);
    }
    lom_core::package::resolve_confined_file(root, &path)?;
    Ok(Some(name.into()))
}

fn target<'a>(map: &'a Value, group: &str, id: &str, key: &str) -> &'a Value {
    if group == "character" {
        &map["characters"][id]["portraits"][key]
    } else {
        &map[group][key]
    }
}

fn set_target(map: &mut Value, group: &str, id: &str, key: &str, relative: &str) {
    if group == "character" {
        map["characters"][id]["portraits"][key] = json!(relative);
    } else {
        map[group][key] = json!(relative);
    }
}

fn reuse_targets(
    map: &mut Value,
    previous: &Value,
    existing: Option<&Path>,
    targets: &[(String, String, String)],
) -> Result<bool> {
    let retained = targets
        .iter()
        .map(|(group, id, key)| cached_file(existing, target(previous, group, id, key)))
        .collect::<Result<Vec<_>>>()?;
    let Some(shared) = retained.iter().flatten().next() else {
        return Ok(false);
    };
    for ((group, id, key), old) in targets.iter().zip(&retained) {
        set_target(map, group, id, key, old.as_deref().unwrap_or(shared));
    }
    Ok(true)
}

pub fn extract(
    game: &Path,
    output: &Path,
    max_size: u32,
    existing: Option<&Path>,
) -> Result<Value> {
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
    let existing = existing.map(Path::canonicalize).transpose()?;
    let previous = if let Some(root) = existing.as_deref() {
        ensure!(root != output, "增量输出必须使用独立的空目录");
        let mapping =
            lom_core::package::resolve_confined_file(root, &root.join("preview_map.json"))?;
        let map = lom_core::load_json(mapping)?;
        ensure!(
            map["characters"].is_object() && map["views"].is_object(),
            "已有素材库映射不完整"
        );
        ensure!(
            map["portrait_canvas"] == "sprite_rect",
            "已有立绘库使用旧画布格式，请使用“重新完整提取”；原素材库保持不变"
        );
        for character in map["characters"].as_object().unwrap().values() {
            let portraits = character["portraits"]
                .as_object()
                .context("已有立绘映射不完整")?;
            ensure!(
                character["first"]
                    .as_str()
                    .is_some_and(|first| portraits.contains_key(first)),
                "已有素材库人物缺少默认立绘映射"
            );
        }
        map
    } else {
        json!({})
    };
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
    let mut map = previous.clone();
    map["portrait_canvas"] = json!("sprite_rect");
    map["image_metadata_schema"] = json!(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    map["last_extraction_unix_ms"] = json!(now);
    let catalog_path = aa.join("catalog.json");
    let catalog_metadata = fs::metadata(&catalog_path)?;
    let source_catalog = json!({"bytes":catalog_metadata.len(),"modified_unix_ms":catalog_metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64});
    if existing.is_none() {
        map["extracted_unix_ms"] = json!(now);
        map["source_catalog"] = source_catalog;
    } else {
        // Supplementing missing entries does not refresh all retained pixels.
        // Keep the original snapshot's stamp so stale libraries stay detectable.
        map["supplemented_catalog"] = source_catalog;
    }
    for group in [
        "characters",
        "views",
        "combat_backgrounds",
        "pictures",
        "moods",
    ] {
        if map.get(group).is_none() {
            map[group] = json!({});
        }
        ensure!(map[group].is_object(), "已有素材库 {group} 映射必须是对象");
    }
    if map.get("picture_catalog").is_none() {
        map["picture_catalog"] = json!([]);
    }
    let catalog = map["picture_catalog"]
        .as_array_mut()
        .context("插图目录格式不正确")?;
    for picture in &pictures {
        if !catalog.iter().any(|old| old["key"] == picture["key"]) {
            catalog.push(picture.clone());
        }
    }
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
        if map["characters"].get(&id).is_none() {
            map["characters"][&id] = json!({"name":c["name"],"mood_position":c["mood_position"],"first":first,"portraits":{}});
        }
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
    let mut ordered_failures = vec![];
    let mut succeeded = 0;
    let mut reused = 0;
    let total = tasks.len();
    let mut completed = 0;
    let mut report_progress = |failed| {
        completed += 1;
        if completed == 1 || completed % 10 == 0 || completed == total {
            crate::emit(
                "progress",
                json!({"message":"正在导出立绘、表情、背景和插图","done":completed,"total":total,"failed":failed}),
            );
        }
    };
    let mut by_bundle: BTreeMap<&str, Vec<ImageTask<'_>>> = BTreeMap::new();
    for (i, ((address, portrait), (relative, targets))) in tasks.into_iter().enumerate() {
        let reused_images = reuse_targets(&mut map, &previous, existing.as_deref(), &targets)?;
        if reused_images {
            reused += 1;
            if targets
                .iter()
                .all(|(group, id, key)| target_geometry(&map, group, id, key).is_object())
            {
                report_progress(ordered_failures.len());
                continue;
            }
        }
        let Some(entry) = addresses.get(&address) else {
            ordered_failures.push((
                i,
                json!({"address":address,"error":"素材目录中没有对应的素材包"}),
            ));
            report_progress(ordered_failures.len());
            continue;
        };
        by_bundle.entry(&entry.bundle).or_default().push(ImageTask {
            index: i,
            address,
            internal: &entry.internal,
            portrait,
            relative,
            targets,
            reused: reused_images,
        });
    }
    // Keep one bundle open at a time. Its object metadata is indexed once,
    // including for tasks that only need geometry for an existing image.
    for (bundle, tasks) in by_bundle {
        let studio = (|| -> Result<Studio> {
            let path = bundles.join(bundle).canonicalize()?;
            ensure!(path.starts_with(&bundles), "素材包路径超出游戏资源目录");
            Ok(Studio::open(path)?)
        })();
        let indexed = studio
            .as_ref()
            .map(|studio| (studio, SpriteIndex::new(studio)));
        for task in tasks {
            let result = (|| -> Result<()> {
                let (studio, sprites) = indexed
                    .as_ref()
                    .map_err(|error| anyhow::anyhow!("{error:#}"))?;
                let object = sprites.get(task.internal)?;
                let geometry = source_geometry(studio, object, task.portrait)?;
                if !task.reused {
                    let image = decode(studio, object, task.portrait)?;
                    save(&image, &output.join(&task.relative), Some(max_size))?;
                }
                for (group, id, key) in &task.targets {
                    set_geometry(&mut map, group, id, key, &geometry);
                    if !task.reused {
                        set_target(&mut map, group, id, key, &task.relative);
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => succeeded += usize::from(!task.reused),
                Err(error) => ordered_failures.push((
                    task.index,
                    json!({"address":task.address,"error":format!("{error:#}")}),
                )),
            }
            report_progress(ordered_failures.len());
        }
    }
    ordered_failures.sort_unstable_by_key(|(index, _)| *index);
    let mut failures: Vec<_> = ordered_failures
        .into_iter()
        .map(|(_, error)| error)
        .collect();
    let (combat_exported, combat_reused, combat_failures) = extract_combat_backgrounds(
        &data,
        &output,
        max_size,
        existing.as_deref(),
        &previous,
        &mut map,
    )?;
    succeeded += combat_exported;
    reused += combat_reused;
    failures.extend(combat_failures);
    let inventory = unity::open(&data.join("globalgamemanagers.assets"))?;
    let icons = inventory_pictures(
        &inventory,
        &output,
        max_size,
        existing.as_deref(),
        &previous["pictures"],
    )?;
    reused += icons.reused;
    succeeded += icons.images.len() - icons.reused;
    for key in &icons.unavailable {
        // A previous mapping may reference an image that no longer exists.
        map["pictures"].as_object_mut().unwrap().remove(key);
    }
    for (key, relative) in &icons.images {
        map["pictures"][key] = relative.clone();
        let catalog = map["picture_catalog"].as_array_mut().unwrap();
        if !catalog.iter().any(|row| row["key"] == *key) {
            catalog.push(json!({"key":key,"category":if key.starts_with("item:book:") {"Book"} else {"Items"}}));
        }
    }
    ensure!(
        succeeded + reused > 0,
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
        if cached_file(existing.as_deref(), &previous["views"][name])?.is_some() {
            continue;
        }
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
    let moods =
        crate::moods::extract_cached(&studio, &output, existing.as_deref(), &previous["moods"])?;
    map["moods"]
        .as_object_mut()
        .unwrap()
        .extend(moods.as_object().unwrap().clone());
    if cached_file(existing.as_deref(), &previous["event_book"])?.is_none() {
        let book = unity::unique(&studio, 213, "pic_book_1")?;
        save(
            &decode(&studio, book, false)?,
            &output.join("assets/ui/event_book.png"),
            None,
        )?;
        map["event_book"] = json!("assets/ui/event_book.png");
    }
    let mut report = json!({"exported":succeeded,"reused":reused,"incremental":existing.is_some(),"failed":failures.len(),"failures":failures,"resource_platform":bundles.file_name().unwrap().to_string_lossy(),"unavailable_characters":unavailable,
        "characters":map["characters"].as_object().unwrap().len(),"views":map["views"].as_object().unwrap().len(),"combat_backgrounds":map["combat_backgrounds"].as_object().unwrap().len(),"pictures":map["pictures"].as_object().unwrap().len(),"moods":map["moods"].as_object().unwrap().len(),"game_tested":false,"engine":"rust"});
    if !icons.unavailable.is_empty() {
        report["missing_item_icons"] = json!(icons.unavailable);
    }
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

/// CombatLevel.BackImage references are ordinary serialized Sprite objects,
/// not StoryView Addressables. Keep this map separate from scene backgrounds.
fn extract_combat_backgrounds(
    data: &Path,
    output: &Path,
    max_size: u32,
    existing: Option<&Path>,
    previous: &Value,
    map: &mut Value,
) -> Result<(usize, usize, Vec<Value>)> {
    let metadata: Value =
        serde_json::from_str(include_str!("../../../data/ref/combat_backgrounds.json"))?;
    let rows = metadata["backgrounds"]
        .as_array()
        .context("决斗背景目录缺少 backgrounds")?;
    if map.get("combat_backgrounds").is_none() {
        map["combat_backgrounds"] = json!({});
    }
    let mut by_file = BTreeMap::<&str, Vec<&Value>>::new();
    for row in rows {
        by_file
            .entry(row["asset_file"].as_str().context("决斗背景缺少资源文件")?)
            .or_default()
            .push(row);
    }
    let (mut exported, mut reused) = (0, 0);
    let mut failures = Vec::new();
    for (file, rows) in by_file {
        // A retained library can be supplemented without opening this asset
        // when every referenced image and its geometry are already present.
        let mut needed = false;
        for row in &rows {
            let id = row["id"].as_str().context("决斗背景缺少 id")?;
            needed |= cached_file(existing, &previous["combat_backgrounds"][id])?.is_none()
                || !previous["combat_background_metadata"][id].is_object();
        }
        let studio = if needed {
            Some(unity::open(&data.join(file)))
        } else {
            None
        };
        for row in rows {
            let id = row["id"].as_str().context("决斗背景缺少 id")?;
            let name = row["sprite"].as_str().context("决斗背景缺少 Sprite 名")?;
            let result = (|| -> Result<bool> {
                let targets = vec![("combat_backgrounds".into(), String::new(), id.to_owned())];
                let retained = reuse_targets(map, previous, existing, &targets)?;
                if retained && map["combat_background_metadata"][id].is_object() {
                    return Ok(true);
                }
                let studio = studio
                    .as_ref()
                    .context("缺少待提取的决斗背景资源")?
                    .as_ref()
                    .map_err(|error| anyhow::anyhow!("{error:#}"))?;
                let sprites: Vec<_> = studio
                    .objects()
                    .filter(|object| {
                        object.class_id() == 213
                            && object.name() == Some(name)
                            && unity::basename(object.source_path()) == file
                    })
                    .collect();
                ensure!(
                    sprites.len() == 1,
                    "决斗背景 Sprite 缺失或不唯一：{file} / {name}"
                );
                let geometry = source_geometry(studio, sprites[0], false)?;
                set_geometry(map, "combat_backgrounds", "", id, &geometry);
                if !retained {
                    let relative = format!("assets/combat_backgrounds/{name}.png");
                    save(
                        &decode(studio, sprites[0], false)?,
                        &output.join(&relative),
                        Some(max_size),
                    )?;
                    set_target(map, "combat_backgrounds", "", id, &relative);
                }
                Ok(retained)
            })();
            match result {
                Ok(true) => reused += 1,
                Ok(false) => exported += 1,
                Err(error) => {
                    failures.push(json!({"id":id,"asset_file":file,"error":format!("{error:#}")}))
                }
            }
        }
    }
    Ok((exported, reused, failures))
}
struct InventoryPictures {
    images: serde_json::Map<String, Value>,
    reused: usize,
    unavailable: Vec<String>,
}

fn inventory_pictures(
    studio: &Studio,
    output: &Path,
    max_size: u32,
    existing: Option<&Path>,
    previous: &Value,
) -> Result<InventoryPictures> {
    let catalog: Value = serde_json::from_str(include_str!("../../../data/picture_keys.json"))?;
    let requested: std::collections::BTreeSet<_> = catalog["pictures"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["key"].as_str().filter(|key| key.starts_with("item:")))
        .collect();
    let mut database = None;
    for object in studio.objects().filter(|object| {
        object.class_id() == 114
            && object
                .name()
                .is_some_and(|name| name.contains("ItemDatabase"))
    }) {
        let kind = unity::script_type(studio, object)?;
        if kind == ("Mortal.Core".to_owned(), "ItemDatabase".to_owned()) {
            ensure!(database.is_none(), "原版物品数据库不唯一");
            database = Some(object);
        }
    }
    let database = database.context("缺少原版 ItemDatabase")?;
    let bytes = unity::raw(database)?;
    let mut reader = Reader::at(&bytes, 28);
    reader.string()?;
    let mut images = serde_json::Map::new();
    let mut files: BTreeMap<(usize, i64), String> = BTreeMap::new();
    let mut records = vec![];
    for category in ["book", "misc", "special", "consumable"] {
        let collection = unity::resolve(studio, database, reader.ptr()?)?;
        let bytes = unity::raw(collection)?;
        let mut items = Reader::at(&bytes, 28);
        items.string()?;
        for _ in 0..items.count(10000)? {
            let item = unity::resolve(studio, collection, items.ptr()?)?;
            let bytes = unity::raw(item)?;
            let mut fields = Reader::at(&bytes, 28);
            fields.string()?;
            fields.string()?;
            let id = fields.string()?;
            let key = format!("item:{category}:{id}");
            if !requested.contains(key.as_str()) {
                continue;
            }
            let pointer = fields.ptr()?;
            if pointer.is_null() {
                continue;
            }
            let icon = unity::resolve(studio, item, pointer)
                .with_context(|| format!("无法读取原版物品图标：{key}"))?;
            let identity = (icon.file_index(), pointer.path_id);
            if let Some(relative) = cached_file(existing, &previous[&key])? {
                files.entry(identity).or_insert(relative.clone());
                images.insert(key.clone(), json!(relative));
            }
            records.push((
                key,
                identity,
                icon,
                format!("assets/pictures/item_{category}_{id}.png"),
            ));
        }
        items.end()?;
    }
    // Seed all cached aliases before decoding: an earlier missing key may share
    // its Sprite with a later key whose PNG is already in the library.
    let cached_identities: std::collections::BTreeSet<_> = files.keys().copied().collect();
    let mut reused = 0;
    for (key, identity, icon, generated) in records {
        if cached_identities.contains(&identity) {
            reused += 1;
        }
        if images.contains_key(&key) {
            continue;
        }
        let relative = if let Some(relative) = files.get(&identity) {
            relative.clone()
        } else {
            save(
                &decode(studio, icon, false)
                    .with_context(|| format!("无法解码原版物品图标：{key}"))?,
                &output.join(&generated),
                Some(max_size),
            )?;
            files.insert(identity, generated.clone());
            generated
        };
        images.insert(key, json!(relative));
    }
    let (kept, unavailable) = retain_catalog_icons(&mut images, requested, existing, previous)?;
    Ok(InventoryPictures {
        images,
        reused: reused + kept,
        unavailable,
    })
}

fn retain_catalog_icons<'a>(
    images: &mut serde_json::Map<String, Value>,
    requested: impl IntoIterator<Item = &'a str>,
    existing: Option<&Path>,
    previous: &Value,
) -> Result<(usize, Vec<String>)> {
    let mut reused = 0;
    let mut unavailable = Vec::new();
    for key in requested {
        if images.contains_key(key) {
            continue;
        }
        // The shipped catalog spans game versions. An absent optional entry
        // cannot prevent the independent portrait library from being loaded.
        if let Some(relative) = cached_file(existing, &previous[key])? {
            images.insert(key.to_owned(), json!(relative));
            reused += 1;
        } else {
            unavailable.push(key.to_owned());
        }
    }
    Ok((reused, unavailable))
}

type ImageTarget = (String, String, String);
type Tasks = BTreeMap<(String, bool), (String, Vec<ImageTarget>)>;

struct ImageTask<'a> {
    index: usize,
    address: String,
    internal: &'a str,
    portrait: bool,
    relative: String,
    targets: Vec<ImageTarget>,
    reused: bool,
}

fn add_task(tasks: &mut Tasks, address: &str, portrait: bool, folder: &str, target: ImageTarget) {
    let digest = format!("{:x}", Sha256::digest(address.as_bytes()));
    tasks
        .entry((address.into(), portrait))
        .or_insert_with(|| (format!("assets/{folder}/{digest}.png"), vec![]))
        .1
        .push(target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads installed CombatLevel Sprite assets; LOM_GAME_DIR and LOM_ASSET_OUTPUT required"]
    fn native_combat_backgrounds_from_installed_game() -> Result<()> {
        let game = std::env::var("LOM_GAME_DIR")?;
        let output = std::env::var("LOM_ASSET_OUTPUT")?;
        let mut map = json!({});
        let (exported, reused, failures) = extract_combat_backgrounds(
            &Path::new(&game).join("Mortal_Data"),
            Path::new(&output),
            1024,
            None,
            &json!({}),
            &mut map,
        )?;
        ensure!(
            exported == 28 && reused == 0 && failures.is_empty(),
            "{failures:?}"
        );
        ensure!(
            map["combat_backgrounds"]["combat:back_combat_backmountain_1"].is_string(),
            "missing requested duel background"
        );
        fs::write(
            Path::new(&output).join("combat-backgrounds.json"),
            serde_json::to_vec_pretty(&map)?,
        )?;
        let (exported, reused, failures) = extract_combat_backgrounds(
            &Path::new(&game).join("Mortal_Data"),
            Path::new(&output),
            1024,
            Some(Path::new(&output)),
            &map.clone(),
            &mut map,
        )?;
        ensure!(
            exported == 0 && reused == 28 && failures.is_empty(),
            "cached duel background reuse failed"
        );
        Ok(())
    }

    #[test]
    fn cached_aliases_keep_their_paths_and_new_aliases_share_the_png() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        fs::write(root.path().join("assets/normal.png"), b"original normal").unwrap();
        fs::write(root.path().join("assets/alias.png"), b"original alias").unwrap();
        let previous = json!({"characters":{"player":{"first":"normal","portraits":{"normal":"assets/normal.png","alias":"assets/alias.png","missing":"assets/missing.png"}}}});
        let mut map = previous.clone();
        let targets = ["normal", "alias", "missing", "new"]
            .map(|key| ("character".into(), "player".into(), key.into()));
        assert!(reuse_targets(&mut map, &previous, Some(root.path()), &targets).unwrap());
        let portraits = &map["characters"]["player"]["portraits"];
        assert_eq!(portraits["normal"], "assets/normal.png");
        assert_eq!(portraits["alias"], "assets/alias.png");
        assert_eq!(portraits["missing"], "assets/normal.png");
        assert_eq!(portraits["new"], "assets/normal.png");
        assert_eq!(
            fs::read(root.path().join("assets/normal.png")).unwrap(),
            b"original normal"
        );
        assert_eq!(fs::read_dir(root.path().join("assets")).unwrap().count(), 2);
    }

    #[test]
    fn missing_cached_images_are_extracted_again_and_unsafe_paths_are_errors() {
        let root = tempfile::tempdir().unwrap();
        assert!(cached_file(Some(root.path()), &json!("assets/missing.png"))
            .unwrap()
            .is_none());
        for unsafe_name in [
            "../outside.png",
            "assets/../outside.png",
            "C:/outside.png",
            "outside.png",
        ] {
            assert!(
                cached_file(Some(root.path()), &json!(unsafe_name)).is_err(),
                "{unsafe_name}"
            );
        }
        fs::create_dir(root.path().join("assets")).unwrap();
        assert!(cached_file(Some(root.path()), &json!("assets")).is_err());
    }

    #[test]
    fn unavailable_optional_book_icon_does_not_block_other_assets() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        fs::write(root.path().join("assets/kept.png"), b"cached").unwrap();
        let previous = json!({
            "item:book:6015":"assets/no-longer-present.png",
            "item:book:6016":"assets/kept.png"
        });
        let mut images = serde_json::Map::from_iter([(
            "item:book:6014".to_owned(),
            json!("assets/exported.png"),
        )]);
        let (reused, missing) = retain_catalog_icons(
            &mut images,
            ["item:book:6014", "item:book:6015", "item:book:6016"],
            Some(root.path()),
            &previous,
        )
        .unwrap();
        assert_eq!(reused, 1);
        assert_eq!(missing, ["item:book:6015"]);
        assert_eq!(images["item:book:6016"], "assets/kept.png");
        assert_eq!(images.len(), 2);
    }
}
