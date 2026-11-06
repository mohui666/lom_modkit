using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal sealed class LegacyResourceConfig
    {
        internal readonly Dictionary<string, string> Strings = new Dictionary<string, string>(StringComparer.Ordinal);
        internal readonly Dictionary<string, string> Voices = new Dictionary<string, string>(StringComparer.Ordinal);
        internal readonly Dictionary<string, LegacySprite> Sprites = new Dictionary<string, LegacySprite>(StringComparer.OrdinalIgnoreCase);

        internal static LegacyResourceConfig Parse(Dictionary<string, object> legacy)
        {
            if (legacy == null) return null;
            var result = new LegacyResourceConfig();
            object raw;
            if (legacy.TryGetValue("strings", out raw)) ReadStrings(raw, result.Strings);
            if (!legacy.TryGetValue("resources", out raw)) return result;
            var resources = Object(raw, "resources");
            if (resources.TryGetValue("voices", out raw))
            {
                ReadStrings(raw, result.Voices);
                foreach (string reference in result.Voices.Values) RequireUser(reference);
            }
            if (resources.TryGetValue("sprites", out raw))
            {
                foreach (var entry in Object(raw, "sprites"))
                {
                    var data = Object(entry.Value, entry.Key);
                    var sprite = new LegacySprite { Path = entry.Key };
                    object value;
                    if (!data.TryGetValue("frames", out value) || !(value is List<object>))
                        throw new FormatException("旧立绘缺少帧数组：" + entry.Key);
                    foreach (object frameRaw in (List<object>)value)
                    {
                        var frame = Object(frameRaw, "frame");
                        object image;
                        if (!frame.TryGetValue("image", out image) || !(image is string)) throw new FormatException("旧立绘帧缺少 image");
                        RequireUser((string)image);
                        float duration = Number(frame, "duration", 0);
                        if (duration < 0) throw new FormatException("旧立绘帧时长不能小于零");
                        sprite.Frames.Add(new LegacySpriteFrame { Image = (string)image, Duration = duration });
                    }
                    if (sprite.Frames.Count == 0) throw new FormatException("旧立绘没有帧：" + entry.Key);
                    if (data.TryGetValue("pivot", out value))
                    {
                        var pivot = Object(value, "pivot");
                        sprite.PivotX = Number(pivot, "x", 0);
                        sprite.PivotY = Number(pivot, "y", 0);
                    }
                    sprite.PixelsPerUnit = Number(data, "pixels_per_unit", 100);
                    if (sprite.PixelsPerUnit <= 0) throw new FormatException("旧立绘 pixels_per_unit 必须为正数");
                    sprite.Extrude = Convert.ToUInt32(data.TryGetValue("extrude", out value) ? value : 0);
                    sprite.SpriteType = Convert.ToInt32(data.TryGetValue("sprite_type", out value) ? value : 1);
                    if (sprite.SpriteType != 0 && sprite.SpriteType != 1) throw new FormatException("旧立绘 sprite_type 无效");
                    result.Sprites.Add(entry.Key, sprite);
                }
            }
            return result;
        }

        private static Dictionary<string, object> Object(object value, string name)
        {
            var result = value as Dictionary<string, object>;
            if (result == null) throw new FormatException("legacy." + name + " 必须为对象");
            return result;
        }
        private static void ReadStrings(object raw, Dictionary<string, string> target)
        {
            foreach (var entry in Object(raw, "strings"))
            {
                if (!(entry.Value is string)) throw new FormatException("旧本地化或配音映射必须为文本：" + entry.Key);
                target.Add(entry.Key, (string)entry.Value);
            }
        }
        private static float Number(Dictionary<string, object> data, string key, float fallback)
        {
            object value;
            float number = data.TryGetValue(key, out value) ? Convert.ToSingle(value) : fallback;
            if (float.IsNaN(number) || float.IsInfinity(number)) throw new FormatException("旧素材数值无效：" + key);
            return number;
        }
        private static void RequireUser(string value)
        {
            if (string.IsNullOrEmpty(value) || !value.StartsWith("user:", StringComparison.Ordinal))
                throw new FormatException("旧素材必须指向包内 user 素材");
        }
    }
    internal sealed class LegacySprite
    {
        internal string Path;
        internal float PivotX, PivotY, PixelsPerUnit = 100;
        internal uint Extrude;
        internal int SpriteType = 1;
        internal readonly List<LegacySpriteFrame> Frames = new List<LegacySpriteFrame>();
    }
    internal sealed class LegacySpriteFrame { internal string Image; internal float Duration; }
}
