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

        /// <summary>按 Addressables 的 Combat 目录段精确匹配，不扫描整条 key 的子串。</summary>
        internal static bool MatchesOfficialAssetKey(string characterId, string addressKey)
        {
            string token = NormalizeAscii(OfficialAssetToken(characterId));
            string[] parts = (addressKey ?? "").Replace('\\', '/').Split('/');
            for (int i = 0; i + 1 < parts.Length; i++)
            {
                if (!string.Equals(parts[i], "Combat", StringComparison.OrdinalIgnoreCase)) continue;
                return string.Equals(NormalizeAscii(parts[i + 1]), token, StringComparison.Ordinal);
            }
            return false;
        }

        /// <summary>
        /// 原版人物必须以同一个 Combat 目录同时提供四种状态。只核对待机图会允许
        /// 一个被错误拼装的 CombatEnemyAvatar 把其他人物的攻击/受伤图带进来。
        /// </summary>
        internal static bool MatchesOfficialAvatarKeys(
            string characterId, string normalKey, string attackKey,
            string hurtKey, string defenceKey)
        {
            return MatchesOfficialAssetKey(characterId, normalKey)
                && MatchesOfficialAssetKey(characterId, attackKey)
                && MatchesOfficialAssetKey(characterId, hurtKey)
                && MatchesOfficialAssetKey(characterId, defenceKey);
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

        private static string NormalizeAscii(string value)
        {
            var chars = new List<char>();
            foreach (char c in (value ?? "").ToLowerInvariant())
                if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) chars.Add(c);
            return new string(chars.ToArray());
        }
    }
}
