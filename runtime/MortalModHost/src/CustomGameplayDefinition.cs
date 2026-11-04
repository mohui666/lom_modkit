using System;
using System.Collections.Generic;

namespace MortalModHost
{
    // Package-owned authoring data. No Unity objects or player state belong here.
    internal sealed class CustomGameplayDefinition
    {
        internal string Type, Description, Category, Trigger, UseIn;
        internal int Cost, Chance, Uses;
        internal readonly List<CustomMartialLevel> Levels = new List<CustomMartialLevel>();
        internal readonly List<CustomSupportAction> Effects = new List<CustomSupportAction>();
        internal readonly List<CustomConsumableEffect> ConsumableEffects = new List<CustomConsumableEffect>();
        internal CustomCombatSkillDefinition Skill;

        internal static CustomGameplayDefinition Parse(Dictionary<string, object> root, string type)
        {
            var data = new CustomGameplayDefinition { Type = type,
                Description = Text(root, "description", ""), Cost = Number(root, "cost", 0, 0, 999999) };
            if (type == "item")
            {
                data.Category = Text(root, "category", "misc");
                if (data.Category != "misc" && data.Category != "special" && data.Category != "consumable")
                    throw new FormatException("自定义物品 category 必须是 misc/special/consumable");
                if (data.Category == "consumable")
                {
                    data.UseIn = Text(root, "use_in", "field");
                    if (data.UseIn != "field" && data.UseIn != "combat")
                        throw new FormatException("消耗品 use_in 必须是 field/combat");
                    foreach (object row in Rows(root, "effects", 1, 16))
                    {
                        var effect = Object(row, "effects");
                        if (effect.ContainsKey("target")) throw new FormatException("主动消耗品固定作用于玩家，不能指定 target");
                        var value = new CustomConsumableEffect { Type = Text(effect, "type", null) };
                        if (data.UseIn == "field")
                        {
                            if (value.Type != "stat") throw new FormatException("养成消耗品只支持 stat 效果");
                            value.Stat = Text(effect, "stat", null);
                            if (string.IsNullOrWhiteSpace(value.Stat)) throw new FormatException("消耗品 stat 不能为空");
                            value.Value = Number(effect, "value", null, -999999, 999999);
                            if (value.Value == 0) throw new FormatException("消耗品属性变化不能为 0");
                        }
                        else
                        {
                            if (value.Type != "heal" && value.Type != "stamina")
                                throw new FormatException("决斗消耗品只支持 heal/stamina 效果");
                            value.Value = Number(effect, "value", null, 1, 999999);
                        }
                        data.ConsumableEffects.Add(value);
                    }
                }
            }
            else if (type == "martial")
            {
                data.Category = "book";
                foreach (object row in Rows(root, "levels", 1, 20))
                {
                    var level = Object(row, "levels");
                    var value = new CustomMartialLevel { Points = Number(level, "points", null, 1, 999999) };
                    ParseStats(level, "requirements", "min", value.Requirements);
                    ParseStats(level, "bonuses", "delta", value.Bonuses);
                    data.Levels.Add(value);
                }
            }
            else if (type == "support")
            {
                data.Trigger = Text(root, "trigger", "round_start");
                if (data.Trigger != "round_start" && data.Trigger != "after_action" && data.Trigger != "after_hit")
                    throw new FormatException("援助 trigger 必须是 round_start/after_action/after_hit");
                data.Chance = Number(root, "chance", 100, 0, 100);
                data.Uses = Number(root, "uses", 1, 1, 99);
                foreach (object row in Rows(root, "effects", 1, 16))
                {
                    var effect = Object(row, "effects");
                    string action = Text(effect, "type", null), target = Text(effect, "target", null);
                    if (action != "damage" && action != "heal" && action != "stamina")
                        throw new FormatException("援助效果 type 必须是 damage/heal/stamina");
                    if (target != "self" && target != "enemy")
                        throw new FormatException("援助效果 target 必须是 self/enemy");
                    data.Effects.Add(new CustomSupportAction { Type = action, Target = target,
                        Value = Number(effect, "value", null, 1, 999999) });
                }
            }
            else if (type == "skill") data.Skill = CustomCombatSkillDefinition.Parse(root);
            else throw new FormatException("不是自定义玩法内容：" + type);
            return data;
        }

        private static void ParseStats(Dictionary<string, object> root, string name, string field,
            List<CustomStatValue> target)
        {
            var seen = new HashSet<string>(StringComparer.Ordinal);
            foreach (object row in Rows(root, name, 0, 32))
            {
                var item = Object(row, name);
                string stat = Text(item, "stat", null);
                if (string.IsNullOrWhiteSpace(stat) || !seen.Add(stat))
                    throw new FormatException(name + " 属性不能为空或重复");
                target.Add(new CustomStatValue { Stat = stat,
                    Value = Number(item, field, null, -999999, 999999) });
            }
        }

        private static Dictionary<string, object> Object(object row, string name)
        {
            var data = row as Dictionary<string, object>;
            if (data == null) throw new FormatException(name + " 项必须是对象");
            return data;
        }

        private static IList<object> Rows(Dictionary<string, object> root, string name, int min, int max)
        {
            object value;
            if (!root.TryGetValue(name, out value))
            {
                if (min == 0) return new List<object>();
                throw new FormatException("缺少 " + name);
            }
            var rows = value as IList<object>;
            if (rows == null || rows.Count < min || rows.Count > max)
                throw new FormatException(name + " 必须是 " + min + "~" + max + " 项数组");
            return rows;
        }

        private static string Text(Dictionary<string, object> root, string name, string fallback)
        {
            object value;
            if (!root.TryGetValue(name, out value))
            {
                if (fallback != null) return fallback;
                throw new FormatException("缺少 " + name);
            }
            if (!(value is string)) throw new FormatException(name + " 必须是字符串");
            return (string)value;
        }

        private static int Number(Dictionary<string, object> root, string name, int? fallback, int min, int max)
        {
            object value;
            if (!root.TryGetValue(name, out value))
            {
                if (fallback.HasValue) return fallback.Value;
                throw new FormatException("缺少 " + name);
            }
            if (!(value is double)) throw new FormatException(name + " 必须是整数");
            double number = (double)value;
            if (double.IsNaN(number) || double.IsInfinity(number) || Math.Floor(number) != number || number < min || number > max)
                throw new FormatException(name + " 必须是 " + min + "~" + max + " 的整数");
            return (int)number;
        }
    }

    internal sealed class CustomMartialLevel
    {
        internal int Points;
        internal readonly List<CustomStatValue> Requirements = new List<CustomStatValue>();
        internal readonly List<CustomStatValue> Bonuses = new List<CustomStatValue>();
    }
    internal sealed class CustomStatValue { internal string Stat; internal int Value; }
    internal sealed class CustomSupportAction { internal string Type, Target; internal int Value; }
    internal sealed class CustomConsumableEffect { internal string Type, Stat; internal int Value; }
}
