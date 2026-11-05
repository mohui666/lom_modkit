//! Portrait registration in the game's 1920 × 1080, bottom-pivoted slot.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn layout(meta: &Value, portrait: &str) -> [f64; 3] {
    let value = &meta["portrait_layouts"][portrait];
    [
        value["x"].as_f64().unwrap_or(0.0),
        value["y"].as_f64().unwrap_or(0.0),
        value["scale"].as_f64().unwrap_or(100.0),
    ]
}

pub fn normalize(raw: &Value, portraits: &Value) -> Result<Value> {
    if raw.is_null() {
        return Ok(Value::Null);
    }
    let entries = raw
        .as_object()
        .context("portrait_layouts 必须是表情对齐配置")?;
    let mut result = serde_json::Map::new();
    for (key, entry) in entries {
        ensure!(
            portraits.get(key).is_some(),
            "立绘对齐引用了不存在的表情：{key}"
        );
        ensure!(entry.is_object(), "表情 {key} 的对齐配置必须是对象");
        let mut value =
            json!({"scale":crate::content::positive_scale(&entry["scale"], "表情缩放")?});
        for axis in ["x", "y"] {
            let number = if entry[axis].is_null() {
                0.0
            } else {
                entry[axis].as_f64().context("表情偏移必须是数字")?
            };
            ensure!(
                number.is_finite() && (number as f32).is_finite(),
                "表情偏移必须是游戏可表示的有限数字"
            );
            value[axis] = json!(number);
        }
        result.insert(key.clone(), value);
    }
    Ok(Value::Object(result))
}

pub fn image_rect(size: [u32; 2], transform: [f64; 3]) -> [f64; 4] {
    let factor = (1920.0 / size[0] as f64).min(1080.0 / size[1] as f64) * transform[2] / 100.0;
    let width = size[0] as f64 * factor;
    let height = size[1] as f64 * factor;
    let left = 960.0 + transform[0] - width / 2.0;
    let top = 1080.0 - transform[1] - 1080.0 * transform[2] / 200.0 - height / 2.0;
    [left, top, left + width, top + height]
}

pub fn visible_rect(bytes: &[u8], transform: [f64; 3]) -> Result<[f64; 4]> {
    let image = image::load_from_memory(bytes)?.to_rgba8();
    let (mut left, mut top, mut right, mut bottom) = (image.width(), image.height(), 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    ensure!(
        right > left && bottom > top,
        "这张立绘没有可用于对齐的可见内容"
    );
    let rect = image_rect([image.width(), image.height()], transform);
    let unit = (rect[2] - rect[0]) / image.width() as f64;
    Ok([
        rect[0] + left as f64 * unit,
        rect[1] + top as f64 * unit,
        rect[0] + right as f64 * unit,
        rect[1] + bottom as f64 * unit,
    ])
}

pub fn reference(meta: &Value) -> &str {
    meta["portrait_reference"].as_str().unwrap_or_else(|| {
        if meta["portraits"].get("normal").is_some() {
            "normal"
        } else {
            meta["portraits"]
                .as_object()
                .and_then(|p| p.keys().next())
                .map(String::as_str)
                .unwrap_or("")
        }
    })
}

pub fn auto_align(meta: &mut Value, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let reference = reference(meta).to_owned();
    let portraits = meta["portraits"]
        .as_object()
        .context("人物没有表情")?
        .clone();
    let bytes = |key: &str| -> Result<&[u8]> {
        let filename = portraits
            .get(key)
            .and_then(Value::as_str)
            .with_context(|| format!("基准或目标表情不存在：{key}"))?;
        files
            .get(filename)
            .map(Vec::as_slice)
            .with_context(|| format!("缺少表情图片：{filename}"))
    };
    let mut next = meta.clone();
    let mut anchor = visible_rect(bytes(&reference)?, layout(meta, &reference))?;
    if meta["portrait_layouts"].get(&reference).is_none() {
        let mut value = layout(meta, &reference);
        value[0] += 960.0 - (anchor[0] + anchor[2]) / 2.0;
        value[1] += anchor[3] - 1080.0;
        next["portrait_layouts"][&reference] = json!({"x":value[0],"y":value[1],"scale":value[2]});
        anchor = visible_rect(bytes(&reference)?, value)?;
    }
    for key in portraits.keys().filter(|key| **key != reference) {
        let mut value = layout(meta, key);
        let bounds = visible_rect(bytes(key)?, value)?;
        value[0] += (anchor[0] + anchor[2] - bounds[0] - bounds[2]) / 2.0;
        value[1] += bounds[3] - anchor[3];
        next["portrait_layouts"][key] = json!({"x":value[0],"y":value[1],"scale":value[2]});
    }
    *meta = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(width: u32, height: u32, body: [u32; 4]) -> Vec<u8> {
        let mut image = image::RgbaImage::new(width, height);
        for y in body[1]..body[3] {
            for x in body[0]..body[2] {
                image.put_pixel(x, y, image::Rgba([160, 80, 200, 255]));
            }
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }
    #[test]
    fn cropped_expressions_and_transparent_padding_share_a_visible_baseline() {
        let files = BTreeMap::from([
            ("normal.png".into(), png(100, 160, [30, 10, 80, 125])),
            ("happy.png".into(), png(200, 80, [60, 5, 170, 65])),
        ]);
        let mut meta = json!({"portraits":{"normal":"normal.png","happy":"happy.png"}});
        auto_align(&mut meta, &files).unwrap();
        let a = visible_rect(&files["normal.png"], layout(&meta, "normal")).unwrap();
        let b = visible_rect(&files["happy.png"], layout(&meta, "happy")).unwrap();
        assert!((a[3] - 1080.0).abs() < 0.001 && (b[3] - a[3]).abs() < 0.001);
        assert!((a[0] + a[2] - b[0] - b[2]).abs() < 0.001);
        assert_eq!(layout(&meta, "happy")[2], 100.0);
        let previous = meta.clone();
        auto_align(&mut meta, &files).unwrap();
        assert_eq!(meta, previous);
    }
    #[test]
    fn failed_auto_alignment_leaves_all_existing_calibration_intact() {
        let mut meta = json!({"portraits":{"normal":"normal.png","happy":"missing.png"},"portrait_layouts":{"normal":{"x":20,"y":-30,"scale":75}}});
        let before = meta.clone();
        let files = BTreeMap::from([("normal.png".into(), png(80, 100, [10, 5, 70, 90]))]);
        assert!(auto_align(&mut meta, &files).is_err());
        assert_eq!(meta, before);
        assert!(normalize(&json!({"missing":{}}), &meta["portraits"]).is_err());
    }
}
