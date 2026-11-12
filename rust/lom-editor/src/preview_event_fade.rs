use serde_json::{json, Value};

/// The final simulated state has removed the event book. Keep its previous
/// frame, image and caption together only while the authored fade is playing.
pub(crate) fn fading_event_picture(before: &Value, action: &Value, elapsed: f64) -> Option<Value> {
    if action["type"] != "cg" || action["kind"] != "event" || action["action"] != "hide" {
        return None;
    }
    let seconds = action["fade_out"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)?;
    let progress = (elapsed.max(0.0) / seconds).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return None;
    }
    let mut picture = before["official_pictures"]["event"].as_object()?.clone();
    let opacity = picture
        .get("preview_opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    picture.insert("preview_opacity".into(), json!(opacity * (1.0 - progress)));
    Some(Value::Object(picture))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_frame_image_and_caption_remain_together_until_fade_finishes() {
        let before = json!({"official_pictures":{"event":{
            "type":"cg","kind":"event","action":"show","key":"user:letter",
            "event_title":"来信","event_text":"正文","fade_in":0.5
        }}});
        let original = before.clone();
        let action = json!({"type":"cg","kind":"event","action":"hide","fade_out":2.0});
        let start = fading_event_picture(&before, &action, 0.0).unwrap();
        assert_eq!(start["preview_opacity"], 1.0);
        let half = fading_event_picture(&before, &action, 1.0).unwrap();
        assert_eq!(half["preview_opacity"], 0.5);
        assert_eq!(half["key"], "user:letter");
        assert_eq!(half["event_title"], "来信");
        assert_eq!(half["event_text"], "正文");
        assert!(fading_event_picture(&before, &action, 2.0).is_none());
        assert_eq!(before, original);
    }

    #[test]
    fn legacy_zero_and_absent_events_do_not_create_a_fading_picture() {
        let before = json!({"official_pictures":{"event":{"kind":"event","key":"letter"}}});
        for action in [
            json!({"type":"cg","kind":"event","action":"hide"}),
            json!({"type":"cg","kind":"event","action":"hide","fade_out":0}),
            json!({"type":"cg","kind":"picture","action":"hide","fade_out":1}),
        ] {
            assert!(fading_event_picture(&before, &action, 0.0).is_none());
        }
        assert!(fading_event_picture(
            &json!({"official_pictures":{}}),
            &json!({"type":"cg","kind":"event","action":"hide","fade_out":1}),
            0.5
        )
        .is_none());
    }
}
