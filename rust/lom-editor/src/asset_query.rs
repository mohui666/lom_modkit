//! Image-backed asset discovery. Geometry describes pixels/Transforms, never anatomy.
use anyhow::{ensure, Context, Result};
use lom_core::{load_json, project::Project};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub(crate) const OPTIONAL_FIELDS: &[&str] = &[
    "library",
    "project",
    "game",
    "kind",
    "query",
    "id",
    "portrait",
    "limit",
    "offset",
    "thumbnail_size",
    "cache_dir",
    "position",
];

fn user_root() -> PathBuf {
    if let Some(root) = std::env::var_os("APPDATA") {
        return PathBuf::from(root).join("lom_modkit");
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/lom_modkit")
    } else {
        home.join(".local/share/lom_modkit")
    }
}

pub(crate) fn resolve_library(request: &Value, base: &Path) -> Result<PathBuf> {
    let selected = if let Some(path) = request.get("library") {
        Some(base.join(path.as_str().context("library 必须是路径字符串")?))
    } else {
        let root = user_root();
        let prefs = root.join("rust/preferences.json");
        let legacy = root.join("settings.json");
        let prefs = if prefs.is_file() {
            load_json(prefs)?
        } else {
            json!({})
        };
        let path = if prefs.get("preview_library_dir").is_some() {
            prefs["preview_library_dir"].as_str().map(PathBuf::from)
        } else if legacy.is_file() {
            load_json(legacy)?["preview_library_dir"]
                .as_str()
                .map(PathBuf::from)
        } else {
            None
        };
        path
    };
    let root = selected.unwrap_or_else(|| {
        std::env::var_os("LOM_MODKIT_ROOT")
            .map(|root| PathBuf::from(root).join("data"))
            .unwrap_or_else(|| user_root().join("game-previews"))
    });
    let root = std::path::absolute(root)?;
    Ok(
        if !root.join("preview_map.json").is_file() && root.join("data/preview_map.json").is_file()
        {
            root.join("data")
        } else {
            root
        },
    )
}

fn modified(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|t| t.as_millis() as u64)
}

fn output_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    // Canonical Windows paths use a verbatim prefix internally. Return ordinary
    // absolute paths so image viewers and Markdown clients can open the result.
    if let Some(path) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}

fn status(root: &Path, mapping: Option<&Value>, request: &Value, base: &Path) -> Result<Value> {
    let index = root.join("preview_map.json");
    let mut value = json!({"path":root,"index":index,"index_modified_unix_ms":modified(&index),
        "state":if mapping.is_some(){"available"}else{"missing"},"freshness":"unknown",
        "freshness_reason":"未提供可比较的当前游戏素材来源信息",
        "metadata_schema":mapping.and_then(|m|m.get("image_metadata_schema")),
        "extracted_unix_ms":mapping.and_then(|m|m.get("extracted_unix_ms")),
        "missing_capabilities":[],"format_outdated":false});
    let mut missing = Vec::new();
    if let Some(map) = mapping {
        if map["portrait_canvas"] != "sprite_rect" {
            value["format_outdated"] = json!(true);
            missing.push("restored_sprite_canvas");
        }
        if map["image_metadata_schema"] != 1 {
            missing.push("source_sprite_geometry");
        }
        if map["characters"]
            .as_object()
            .into_iter()
            .flatten()
            .any(|(_, c)| {
                c["portraits"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .any(|(p, _)| {
                        !matches!(
                            c["portrait_metadata"][p]["body_framing"].as_str(),
                            Some("half_body" | "full_body")
                        )
                    })
            })
        {
            missing.push("body_framing_annotations");
        }
        if !map["moods"].is_object() {
            missing.push("mood_animation");
        }
        if !map["pictures"].is_object() {
            missing.push("picture_library");
        }
        if let Some(game) = request.get("game") {
            let current = base
                .join(game.as_str().context("game 必须是路径字符串")?)
                .join("Mortal_Data/StreamingAssets/aa/catalog.json");
            let metadata = std::fs::metadata(&current)
                .with_context(|| format!("无法读取游戏素材目录：{}", current.display()))?;
            let source = &map["source_catalog"];
            if let (Some(bytes), Some(stamp)) = (
                source["bytes"].as_u64(),
                source["modified_unix_ms"].as_u64(),
            ) {
                let now = json!({"bytes":metadata.len(),"modified_unix_ms":modified(&current)});
                value["source_catalog_comparison"] =
                    json!({"recorded":source,"current":now,"path":current});
                if metadata.len() != bytes || now["modified_unix_ms"] != stamp {
                    value["freshness"] = json!("outdated");
                    value["freshness_reason"] =
                        json!("当前游戏素材目录的大小或修改时间与提取记录不同，需要刷新预览库");
                } else {
                    value["freshness_reason"] = json!("所记录的素材目录大小与修改时间相同；未校验其他游戏资源，不能保证全部素材最新");
                }
            }
        }
    } else {
        missing.extend([
            "images",
            "source_sprite_geometry",
            "mood_animation",
            "picture_library",
        ]);
    }
    value["missing_capabilities"] = json!(missing);
    Ok(value)
}

/// Lightweight: read the mapping and its source stamps; do not open every image.
pub(crate) fn library_status(request: &Value, base: &Path) -> Result<Value> {
    let root = resolve_library(request, base)?;
    if !root.join("preview_map.json").is_file() {
        return status(&root, None, request, base);
    }
    let map = match load_json(root.join("preview_map.json")) {
        Ok(map) if map["characters"].is_object() && map["views"].is_object() => map,
        result => {
            let mut value = status(&root, None, request, base)?;
            value["state"] = json!("invalid");
            value["error"] = json!(match result {
                Err(error) => format!("{error:#}"),
                _ => "预览索引缺少 characters/views 对象".into(),
            });
            return Ok(value);
        }
    };
    status(&root, Some(&map), request, base)
}

#[derive(Clone)]
struct Item {
    kind: &'static str,
    id: String,
    name: String,
    category: Option<String>,
    portrait: Option<String>,
    path: Option<String>,
    source: &'static str,
    meta: Value,
}

fn official(map: &Value) -> Vec<Item> {
    let catalog = lom_core::validate::editor_data();
    let pictures: BTreeMap<_, _> = crate::forms::picture_catalog()["pictures"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["key"].as_str().map(|key| (key, row)))
        .collect();
    let mut items = Vec::new();
    let mut ids = BTreeSet::new();
    for row in catalog["characters"].as_array().into_iter().flatten() {
        if let Some(id) = row["id"].as_str() {
            ids.insert(id.to_owned());
        }
    }
    ids.extend(
        map["characters"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| id.clone()),
    );
    for id in ids {
        let row = catalog["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["id"] == id);
        let name = row
            .and_then(|r| r["name"].as_str())
            .or_else(|| map["characters"][&id]["name"].as_str())
            .unwrap_or(&id);
        let name = crate::i18n::term("characters", &id, name);
        let mut portraits = BTreeSet::new();
        portraits.extend(
            row.into_iter()
                .flat_map(|r| r["portraits"].as_array())
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        portraits.extend(
            map["characters"][&id]["portraits"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(p, _)| p.clone()),
        );
        for portrait in portraits {
            items.push(Item {
                kind: "character",
                id: id.clone(),
                name: name.clone(),
                category: None,
                path: map["characters"][&id]["portraits"][&portrait]
                    .as_str()
                    .map(str::to_owned),
                meta: map["characters"][&id]["portrait_metadata"][&portrait].clone(),
                portrait: Some(portrait),
                source: "official",
            });
        }
    }
    for (kind, group, metadata) in [
        ("view", "views", "view_metadata"),
        ("picture", "pictures", "picture_metadata"),
    ] {
        let mut ids = BTreeSet::new();
        ids.extend(
            map[group]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(id, _)| id.clone()),
        );
        if kind == "view" {
            ids.extend(
                catalog[group]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r["id"].as_str())
                    .map(str::to_owned),
            );
        }
        if kind == "picture" {
            ids.extend(pictures.keys().map(|key| (*key).to_owned()));
        }
        for id in ids {
            let picture = pictures.get(id.as_str()).copied();
            let name = catalog[group]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["id"] == id)
                .and_then(|r| r["name"].as_str())
                .or_else(|| picture.and_then(|p| p["name"].as_str()))
                .unwrap_or(&id);
            items.push(Item {
                kind,
                id: id.clone(),
                name: if kind == "view" {
                    crate::i18n::term("views", &id, name)
                } else {
                    name.into()
                },
                path: map[group][&id].as_str().map(str::to_owned),
                category: if kind == "picture" {
                    picture
                        .and_then(|p| p["category"].as_str())
                        .map(str::to_owned)
                } else {
                    None
                },
                portrait: None,
                source: "official",
                meta: map[metadata][&id].clone(),
            });
        }
    }
    items
}

fn project_items(project: &Project) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for (key, bytes) in project
        .assets
        .iter()
        .filter(|(key, _)| key.ends_with("/content.json"))
    {
        let meta: Value =
            serde_json::from_slice(bytes).with_context(|| format!("素材描述无效：{key}"))?;
        let Some(id) = meta["id"].as_str() else {
            continue;
        };
        let name = meta["name"].as_str().unwrap_or(id);
        let Some(parent) = Path::new(key).parent() else {
            continue;
        };
        let paths: Vec<_> = if meta["type"] == "character" {
            meta["portraits"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(p, f)| f.as_str().map(|f| (Some(p.clone()), f)))
                .collect()
        } else if meta["type"] == "image" {
            meta["files"]["main"]
                .as_str()
                .map(|f| vec![(None, f)])
                .unwrap_or_default()
        } else {
            continue;
        };
        for (portrait, path) in paths {
            let path = parent.join(path).to_string_lossy().replace('\\', "/");
            lom_core::package::canonical_archive_name(&path)?;
            items.push(Item {
                kind: if portrait.is_some() {
                    "character"
                } else if meta["image_kind"] == "background" {
                    "view"
                } else {
                    "picture"
                },
                id: format!("user:{id}"),
                name: name.into(),
                category: meta["type"].as_str().map(str::to_owned),
                portrait,
                path: Some(path),
                source: "project",
                meta: meta.clone(),
            });
        }
    }
    Ok(items)
}

fn framing(meta: &Value) -> Value {
    let value = meta["body_framing"]
        .as_str()
        .filter(|v| matches!(*v, "half_body" | "full_body"));
    json!({"value":value.unwrap_or("unknown"),"source":if value.is_some(){"explicit_metadata"}else{"unannotated"},
        "visual_review_required":value.is_none(),"note":"半身／全身只能由明确标注或看图确认，舞台锚点和画布尺寸都不是脚底证据"})
}

fn slug(value: &str) -> String {
    value
        .chars()
        .take(100)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn bounds(image: &image::RgbaImage) -> Value {
    let (mut x, mut y, mut right, mut bottom) = (image.width(), image.height(), 0, 0);
    for (px, py, pixel) in image.enumerate_pixels() {
        if pixel[3] > 0 {
            x = x.min(px);
            y = y.min(py);
            right = right.max(px + 1);
            bottom = bottom.max(py + 1);
        }
    }
    if right == 0 {
        Value::Null
    } else {
        json!([x, y, right - x, bottom - y])
    }
}

fn image_details(
    item: &Item,
    root: &Path,
    project: Option<&Project>,
    cache: &Path,
    size: u32,
) -> Result<Value> {
    let mut result = json!({"available":false,"image":null,"thumbnail":null,"canvas":null,"visible_bounds":null,"visible_bounds_source":"preview_image_pixels",
        "original_canvas":null,"sprite_pivot":null,"crop":null,"framing":framing(&item.meta),
        "coordinate_space":"pixel rectangles: [left,top,width,height]; visible alpha > 0", "geometry_source":"unknown"});
    let Some(relative) = &item.path else {
        result["image_error"] = json!("预览索引没有对应图片，请提取或补充预览素材库");
        return Ok(result);
    };
    let (source, bytes, stamp) = if item.source == "project" {
        let project = project.context("缺少工程素材")?;
        let Some(bytes) = project.assets.get(relative) else {
            result["image_error"] = json!("工程缺少图片字节");
            return Ok(result);
        };
        let project_path = project.source.as_ref().context("工程缺少来源路径")?;
        let image_path = if project_path.is_dir() {
            project_path.join(relative)
        } else {
            project_path.parent().unwrap_or(project_path).join(relative)
        };
        let stamp = modified(&image_path).or_else(|| modified(project_path));
        (
            format!("{}::{relative}", project_path.display()),
            Some(bytes),
            json!({"bytes":bytes.len(),"modified_unix_ms":stamp}),
        )
    } else {
        lom_core::package::canonical_archive_name(relative)?;
        let file = root.join(relative);
        if !file.is_file() {
            result["image_error"] = json!(format!("预览图片缺失：{}", file.display()));
            return Ok(result);
        }
        let file = lom_core::package::resolve_confined_file(root, &file)?;
        let metadata = std::fs::metadata(&file)?;
        (
            file.to_string_lossy().into_owned(),
            None,
            json!({"bytes":metadata.len(),"modified_unix_ms":modified(&file)}),
        )
    };
    std::fs::create_dir_all(cache)?;
    // Cache identity, not an integrity check: two libraries/projects may reuse
    // a character ID, and their returned preview paths must remain independent.
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut identity);
    let cache_name = format!(
        "{}-{:016x}",
        slug(&format!(
            "{}--{}--{}",
            item.kind,
            item.id,
            item.portrait.as_deref().unwrap_or("image")
        )),
        identity.finish()
    );
    let destination = cache.join(cache_name);
    std::fs::create_dir_all(&destination)?;
    let destination = destination.canonicalize()?;
    ensure!(
        destination.starts_with(cache.canonicalize()?),
        "缩略图缓存路径超出目录"
    );
    let metadata_file = destination.join("metadata.json");
    let thumbnail = destination.join(format!("thumbnail-{size}.png"));
    let extension = Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "jpg" | "jpeg"))
        .unwrap_or_else(|| "png".into());
    let original = destination.join(format!("original.{extension}"));
    let signature = json!({"source":source,"stamp":stamp,"size":size});
    let cached = if metadata_file.is_file() {
        Some(load_json(&metadata_file)?)
    } else {
        None
    };
    let measured = if let Some(cached) = cached.filter(|m| {
        m["input"] == signature && thumbnail.is_file() && (bytes.is_none() || original.is_file())
    }) {
        cached["measured"].clone()
    } else {
        let data = if let Some(bytes) = bytes {
            bytes.clone()
        } else {
            std::fs::read(&source)?
        };
        let image = image::load_from_memory(&data)
            .context("无法解码预览图片")?
            .into_rgba8();
        let measured =
            json!({"canvas":[image.width(),image.height()],"visible_bounds":bounds(&image)});
        if bytes.is_some() {
            lom_core::project::atomic_write(&original, &data)?;
        }
        let small = image::DynamicImage::ImageRgba8(image).thumbnail(size, size);
        let mut output = std::io::Cursor::new(Vec::new());
        small.write_to(&mut output, image::ImageFormat::Png)?;
        lom_core::project::atomic_write(&thumbnail, &output.into_inner())?;
        lom_core::project::atomic_write(
            &metadata_file,
            &lom_core::stable_json(&json!({"input":signature,"measured":measured}))?,
        )?;
        measured
    };
    result["available"] = json!(true);
    result["image"] = if bytes.is_some() {
        json!(output_path(&original))
    } else {
        json!(output_path(Path::new(&source)))
    };
    result["thumbnail"] = json!(output_path(&thumbnail));
    result["canvas"] = measured["canvas"].clone();
    result["visible_bounds"] = measured["visible_bounds"].clone();
    if item.meta["original_canvas"].is_array() {
        result["original_canvas"] = item.meta["original_canvas"].clone();
        result["sprite_pivot"] = item.meta["sprite_pivot"].clone();
        result["crop"] = item.meta["crop"].clone();
        result["geometry_source"] =
            json!(item.meta["source"].as_str().unwrap_or("explicit_metadata"));
    } else if item.source == "official" && item.kind == "character" {
        static GEOMETRY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        let geometry = GEOMETRY.get_or_init(|| {
            serde_json::from_str(include_str!("../../../data/portrait_geometry.json")).unwrap()
        });
        let known = &geometry[&item.id][item.portrait.as_deref().unwrap_or("")];
        if known["canvas"].is_array() {
            result["original_canvas"] = known["canvas"].clone();
            result["crop"] = json!({"legacy_image_rect":known["image"],"source":"bundled_geometry","checked_against_current_game":false});
            result["geometry_source"] = json!("bundled_geometry_unverified_for_current_game");
        }
    } else if item.source == "project" {
        result["original_canvas"] = measured["canvas"].clone();
        result["geometry_source"] = json!("project_image_pixels");
    }
    Ok(result)
}

pub(crate) fn query(request: &Value, base: &Path) -> Result<Value> {
    let kind = request["kind"].as_str().unwrap_or("all");
    ensure!(
        ["all", "character", "view", "picture"].contains(&kind),
        "kind 必须是 all/character/view/picture"
    );
    let number = |key: &str, default: u64, max: u64| -> Result<u64> {
        match request.get(key) {
            None => Ok(default),
            Some(v) => {
                let n = v
                    .as_u64()
                    .with_context(|| format!("{key} 必须是非负整数"))?;
                ensure!(n <= max, "{key} 不能超过 {max}");
                Ok(n)
            }
        }
    };
    let limit = number("limit", 20, 100)? as usize;
    ensure!(limit > 0, "limit 必须大于 0");
    let offset = number("offset", 0, usize::MAX as u64)? as usize;
    let size = number("thumbnail_size", 256, 1024)? as u32;
    ensure!(size > 0, "thumbnail_size 必须大于 0");
    for key in ["query", "id", "portrait", "position"] {
        ensure!(
            request.get(key).is_none_or(Value::is_string),
            "{key} 必须是字符串"
        );
    }
    let root = resolve_library(request, base)?;
    let mapping = if root.join("preview_map.json").is_file() {
        load_json(root.join("preview_map.json"))?
    } else {
        json!({})
    };
    let project = request
        .get("project")
        .map(|p| Project::open(&base.join(p.as_str().context("project 必须是路径字符串")?)))
        .transpose()?;
    let mut items = official(&mapping);
    if let Some(p) = &project {
        items.extend(project_items(p)?);
    }
    let keywords: Vec<_> = request["query"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    items.retain(|item| {
        (kind == "all" || kind == item.kind)
            && request["id"].as_str().is_none_or(|id| id == item.id)
            && request["portrait"]
                .as_str()
                .is_none_or(|p| item.portrait.as_deref() == Some(p))
            && {
                let text = format!(
                    "{} {} {} {} {}",
                    item.id,
                    item.name,
                    item.portrait.as_deref().unwrap_or(""),
                    item.category.as_deref().unwrap_or(""),
                    crate::forms::picture_category(item.category.as_deref().unwrap_or(""))
                )
                .to_lowercase();
                keywords.iter().all(|k| text.contains(k))
            }
    });
    let total = items.len();
    let cache = if let Some(value) = request.get("cache_dir") {
        base.join(value.as_str().context("cache_dir 必须是路径字符串")?)
    } else {
        user_root().join("rust/asset-query-cache")
    };
    let mut output = Vec::new();
    for item in items.into_iter().skip(offset).take(limit) {
        let mut value = json!({"kind":item.kind,"id":item.id,"name":item.name,"portrait":item.portrait,"category":item.category,"source":item.source});
        let details =
            image_details(&item, &root, project.as_ref(), &cache, size).with_context(|| {
                format!(
                    "素材 {} / {}",
                    item.id,
                    item.portrait.as_deref().unwrap_or("image")
                )
            })?;
        if item.source == "project" {
            value["project_layout"] = json!({"portrait_layout":item.meta["portrait_layout"],"scale":item.meta["scale"],"art_facing":item.meta["art_facing"],"extends":item.meta["extends"]});
        }
        value
            .as_object_mut()
            .unwrap()
            .extend(details.as_object().unwrap().clone());
        output.push(value);
    }
    let mut response = json!({"ok":true,"items":output,"total":total,"offset":offset,"limit":limit,
        "library":library_status(request,base)?,"game_tested":false,"visual_review_performed":false});
    if let Some(position) = request["position"].as_str() {
        let positions: Value =
            serde_json::from_str(include_str!("../../../data/stage_positions.json"))?;
        let anchor = positions["positions"]
            .get(position)
            .context("未知舞台站位 ID")?;
        response["stage_transform_anchor"] = json!({"id":position,"normalized_from_top_left":[anchor["x"],anchor["feet"]],"reference":positions["reference"],
            "legacy_y_field":"feet","meaning":"舞台 Transform 锚点，不能视为图片中可见的脚底；需结合原画布、透明边界和实际画面确认"});
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn library() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        let mut pixels = image::RgbaImage::new(20, 30);
        for y in 10..25 {
            for x in 3..12 {
                pixels.put_pixel(x, y, image::Rgba([90, 80, 70, 255]));
            }
        }
        pixels.save(root.path().join("assets/normal.png")).unwrap();
        std::fs::write(root.path().join("preview_map.json"),serde_json::to_vec(&json!({"portrait_canvas":"sprite_rect","characters":{"fixture":{"name":"测试人物","first":"normal","portraits":{"normal":"assets/normal.png"}}},"views":{}})).unwrap()).unwrap();
        root
    }
    #[test]
    fn selected_image_reports_transparency_and_never_infers_anatomy_from_anchor() {
        let root = library();
        let cache = tempfile::tempdir().unwrap();
        let request = json!({"library":root.path(),"cache_dir":cache.path(),"query":"测试","kind":"character","position":"L1"});
        let result = query(&request, Path::new(".")).unwrap();
        let item = &result["items"][0];
        assert_eq!(result["total"], 1);
        assert_eq!(item["canvas"], json!([20, 30]));
        assert_eq!(item["visible_bounds"], json!([3, 10, 9, 15]));
        assert_eq!(item["framing"]["value"], "unknown");
        assert_eq!(item["framing"]["visual_review_required"], true);
        assert_eq!(result["visual_review_performed"], false);
        assert_eq!(result["stage_transform_anchor"]["legacy_y_field"], "feet");
        let thumb = Path::new(item["thumbnail"].as_str().unwrap());
        let stamp = modified(thumb);
        let again = query(&request, Path::new(".")).unwrap();
        assert_eq!(again["items"][0]["thumbnail"], item["thumbnail"]);
        assert_eq!(modified(thumb), stamp);
    }
    #[test]
    fn library_status_is_lightweight_and_freshness_is_evidence_based() {
        let root = library();
        let request = json!({"library":root.path()});
        let status = library_status(&request, Path::new(".")).unwrap();
        assert_eq!(status["state"], "available");
        assert_eq!(status["freshness"], "unknown");
        assert_eq!(status["format_outdated"], false);
        assert!(status["missing_capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("source_sprite_geometry")));
        std::fs::remove_file(root.path().join("assets/normal.png")).unwrap();
        assert_eq!(
            library_status(&request, Path::new(".")).unwrap()["state"],
            "available"
        );
        let result = query(
            &json!({"library":root.path(),"id":"fixture"}),
            Path::new("."),
        )
        .unwrap();
        assert_eq!(result["items"][0]["available"], false);
    }
    #[test]
    fn catalog_changes_are_reported_without_claiming_equal_stamps_prove_freshness() {
        let root = library();
        let game = tempfile::tempdir().unwrap();
        let directory = game.path().join("Mortal_Data/StreamingAssets/aa");
        std::fs::create_dir_all(&directory).unwrap();
        let catalog = directory.join("catalog.json");
        std::fs::write(&catalog, b"current").unwrap();
        let path = root.path().join("preview_map.json");
        let mut map = load_json(&path).unwrap();
        map["source_catalog"] = json!({"bytes":1,"modified_unix_ms":0});
        std::fs::write(&path, serde_json::to_vec(&map).unwrap()).unwrap();
        let request = json!({"library":root.path(),"game":game.path()});
        assert_eq!(
            library_status(&request, Path::new(".")).unwrap()["freshness"],
            "outdated"
        );
        map["source_catalog"] = json!({"bytes":7,"modified_unix_ms":modified(&catalog)});
        std::fs::write(&path, serde_json::to_vec(&map).unwrap()).unwrap();
        assert_eq!(
            library_status(&request, Path::new(".")).unwrap()["freshness"],
            "unknown"
        );
    }
    #[test]
    fn matching_ids_in_different_libraries_have_independent_preview_paths() {
        let first = library();
        let second = library();
        let cache = tempfile::tempdir().unwrap();
        let lookup = |library: &Path| {
            query(
                &json!({"library":library,"cache_dir":cache.path(),"id":"fixture"}),
                Path::new("."),
            )
            .unwrap()
        };
        let a = lookup(first.path());
        let b = lookup(second.path());
        assert_ne!(a["items"][0]["thumbnail"], b["items"][0]["thumbnail"]);
        assert!(Path::new(a["items"][0]["thumbnail"].as_str().unwrap()).is_file());
    }
    #[test]
    fn project_jpeg_is_exported_with_its_format_and_source_stays_unchanged() {
        let root = library();
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::new();
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(12, 18)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = jpeg.into_inner();
        let path = "assets/user/image/fixture.photo/photo.jpg";
        project.assets.insert(path.into(), jpeg.clone());
        project.assets.insert("assets/user/image/fixture.photo/content.json".into(),
            serde_json::to_vec(&json!({"schema":1,"content_schema":1,"id":"fixture.photo","name":"测试照片","type":"image","files":{"main":"photo.jpg"}})).unwrap());
        let source = directory.path().join("project");
        project.save_to(&source).unwrap();
        let result = query(&json!({"library":root.path(),"project":source,"id":"user:fixture.photo","cache_dir":directory.path().join("cache")}),Path::new(".")).unwrap();
        let image = Path::new(result["items"][0]["image"].as_str().unwrap());
        assert_eq!(image.extension().unwrap(), "jpg");
        assert_eq!(std::fs::read(image).unwrap(), jpeg);
        assert_eq!(std::fs::read(source.join(path)).unwrap(), jpeg);
        assert_eq!(result["items"][0]["original_canvas"], json!([12, 18]));
    }
}
