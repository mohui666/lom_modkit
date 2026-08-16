using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>Combat v3 角色引用及自定义四帧回退规则（无 Unity 依赖）。</summary>
    internal static class CombatCharacterPolicy
    {
        internal static bool IsUserCharacter(string value)
        {
            return value != null && value.StartsWith("user:", StringComparison.Ordinal)
                && value.Length > 5;
        }

        internal static string OfficialAssetToken(string characterId)
        {
            if (string.IsNullOrEmpty(characterId) || IsUserCharacter(characterId))
                throw new ArgumentException("必须提供官方人物 id", nameof(characterId));
            if (characterId == "special3") return "special003";
            if (characterId == "special4") return "special004";
            return characterId;
        }

        internal static Dictionary<string, string> ResolveFrames(
            IDictionary<string, string> combatFrames, string normalPortrait)
        {
            string idle = Read(combatFrames, "idle");
            if (string.IsNullOrEmpty(idle)) idle = normalPortrait;
            if (string.IsNullOrEmpty(idle))
                throw new InvalidOperationException("自定义决斗角色缺少 idle 和 portraits.normal");
            return new Dictionary<string, string>(StringComparer.Ordinal)
            {
                { "idle", idle },
                { "attack", Read(combatFrames, "attack") ?? idle },
                { "hurt", Read(combatFrames, "hurt") ?? idle },
                { "defence", Read(combatFrames, "defence") ?? idle }
            };
        }

        private static string Read(IDictionary<string, string> source, string key)
        {
            string value;
            return source != null && source.TryGetValue(key, out value)
                && !string.IsNullOrEmpty(value) ? value : null;
        }
    }
}
