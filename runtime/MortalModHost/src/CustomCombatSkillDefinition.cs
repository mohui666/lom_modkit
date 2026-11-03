using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>Package-owned active duel skill data; contains no Unity objects.</summary>
    internal sealed class CustomCombatSkillDefinition
    {
        internal string Action;
        internal int Power, StaminaCost, Accuracy, Cooldown, AiChance;
        internal readonly List<CustomSupportAction> Effects = new List<CustomSupportAction>();
        internal CustomCombatAnimationDefinition Animation;

        internal static CustomCombatSkillDefinition Parse(Dictionary<string, object> root)
        {
            var skill = new CustomCombatSkillDefinition
            {
                Action = Text(root, "action", "attack"),
                Power = Number(root, "power", 0, 0, 999999),
                StaminaCost = Number(root, "stamina_cost", 0, 0, 9999),
                Accuracy = Number(root, "accuracy", 100, 0, 100),
                Cooldown = Number(root, "cooldown", 0, 0, 99),
                AiChance = Number(root, "ai_chance", 100, 0, 100)
            };
            if (skill.Action != "attack" && skill.Action != "weapon" && skill.Action != "ultimate")
                throw new FormatException("招式 action 必须是 attack/weapon/ultimate");
            object raw;
            if (root.TryGetValue("effects", out raw))
            {
                var effects = raw as IList<object>;
                if (effects == null || effects.Count > 16) throw new FormatException("招式effects必须是至多16项数组");
                foreach (object row in effects)
                {
                    var effect = row as Dictionary<string, object>;
                    if (effect == null) throw new FormatException("招式effects项必须是对象");
                    string type = Text(effect, "type", null), target = Text(effect, "target", null);
                    if (type != "damage" && type != "heal" && type != "stamina")
                        throw new FormatException("招式效果必须是damage/heal/stamina");
                    if (target != "self" && target != "enemy") throw new FormatException("招式效果target必须是self/enemy");
                    skill.Effects.Add(new CustomSupportAction { Type = type, Target = target,
                        Value = Number(effect, "value", null, 1, 999999) });
                }
            }
            if (root.TryGetValue("animation", out raw))
            {
                var animation = raw as Dictionary<string, object>;
                if (animation == null) throw new FormatException("招式animation必须是对象或省略");
                object framesRaw;
                var frames = animation.TryGetValue("frames", out framesRaw) ? framesRaw as IList<object> : null;
                if (frames == null || frames.Count < 1 || frames.Count > 120)
                    throw new FormatException("招式动画frames必须是1~120个PNG帧");
                var clip = new CustomCombatAnimationDefinition
                {
                    Fps = Number(animation, "fps", 12, 1, 60),
                    HitFrame = Number(animation, "hit_frame", 0, 0, frames.Count - 1),
                    Scale = Number(animation, "scale", 100, 50, 200)
                };
                foreach (object frame in frames)
                {
                    string name = frame as string;
                    if (string.IsNullOrWhiteSpace(name) || name.Contains("..") || name.IndexOfAny(new[] { '/', '\\', ':' }) >= 0
                        || !name.EndsWith(".png", StringComparison.OrdinalIgnoreCase))
                        throw new FormatException("招式动画帧必须是同目录PNG文件名");
                    clip.Frames.Add(name);
                }
                skill.Animation = clip;
            }
            return skill;
        }

        private static string Text(Dictionary<string, object> root, string key, string fallback)
        {
            object raw;
            if (!root.TryGetValue(key, out raw))
            {
                if (fallback != null) return fallback;
                throw new FormatException("缺少招式字段：" + key);
            }
            string value = raw as string;
            if (value == null) throw new FormatException("招式字段必须是字符串：" + key);
            return value;
        }

        private static int Number(Dictionary<string, object> root, string key, int? fallback, int min, int max)
        {
            object raw;
            if (!root.TryGetValue(key, out raw))
            {
                if (fallback.HasValue) return fallback.Value;
                throw new FormatException("缺少招式字段：" + key);
            }
            if (!(raw is double) && !(raw is int) && !(raw is long))
                throw new FormatException("招式字段必须是整数：" + key);
            double number = Convert.ToDouble(raw);
            if (double.IsNaN(number) || double.IsInfinity(number) || number != Math.Truncate(number) || number < min || number > max)
                throw new FormatException("招式字段越界：" + key + "（" + min + "~" + max + "）");
            return (int)number;
        }
    }

    internal sealed class CustomCombatAnimationDefinition
    {
        internal readonly List<string> Frames = new List<string>();
        internal int Fps, HitFrame, Scale;
    }
}
