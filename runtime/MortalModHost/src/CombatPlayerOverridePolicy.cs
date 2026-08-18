using System;

namespace MortalModHost
{
    /// <summary>
    /// 赵活 player_* 是官方 GameStat 基准值。血量由 SetPlayerStat 的
    /// _playerTotalHealth 换算，FinalValue 含被动加成。不写 SaveSystem。
    /// </summary>
    internal static class CombatPlayerOverridePolicy
    {
        internal const string Prefix = "player_";
        internal const string TalentsKey = "player_talents";

        internal static readonly string[] StatFields =
        {
            "max_health", "health", "max_stamina", "stamina",
            "stamina_power", "strength", "internal", "dexterity", "talking",
            "defence", "sword", "fist", "martial_weapon", "mental",
            "poison_resist", "paralyzed_resist",
            "disposition", "behaviour", "karma", "training",
        };

        internal static string Key(string field)
        {
            if (string.IsNullOrEmpty(field))
                throw new ArgumentException("赵活覆盖字段为空", nameof(field));
            if (field.StartsWith(Prefix, StringComparison.Ordinal)) return field;
            return Prefix + field;
        }

        internal static bool HasAny(Func<string, bool> hasConfig)
        {
            if (hasConfig == null) throw new ArgumentNullException(nameof(hasConfig));
            if (hasConfig(TalentsKey)) return true;
            for (int i = 0; i < StatFields.Length; i++)
                if (hasConfig(Key(StatFields[i]))) return true;
            return false;
        }

        internal static bool TouchesVitality(Func<string, bool> hasConfig)
        {
            if (hasConfig == null) throw new ArgumentNullException(nameof(hasConfig));
            return hasConfig(Key("max_health")) || hasConfig(Key("health"))
                || hasConfig(Key("max_stamina")) || hasConfig(Key("stamina"));
        }

        /// <summary>
        /// player_* 对应官方 GameStatType 名称。这些是基准值，
        /// SetPlayerStat 读 FinalValue（基准 + 被动加成）。
        /// </summary>
        internal static bool TryOfficialGameStatType(string configKey, out string officialType)
        {
            officialType = null;
            if (string.IsNullOrEmpty(configKey)) return false;
            string field = configKey;
            if (field.StartsWith(Prefix, StringComparison.Ordinal))
                field = field.Substring(Prefix.Length);
            switch (field)
            {
                case "strength": officialType = "體力"; return true;
                case "stamina_power": officialType = "內力"; return true;
                case "dexterity": officialType = "輕功"; return true;
                case "talking": officialType = "嘴力"; return true;
                case "defence": officialType = "防禦"; return true;
                case "karma": officialType = "道德"; return true;
                case "disposition": officialType = "性情"; return true;
                case "behaviour": officialType = "處世"; return true;
                case "training": officialType = "修養"; return true;
                case "martial_weapon": officialType = "武功暗器"; return true;
                case "internal": officialType = "陰陽內功"; return true;
                case "sword": officialType = "武功刀劍"; return true;
                case "fist": officialType = "武功拳掌"; return true;
                case "poison_resist": officialType = "抗毒"; return true;
                case "paralyzed_resist": officialType = "抗麻"; return true;
                case "mental": officialType = "心理衛生"; return true;
                default: return false;
            }
        }
    }
}
