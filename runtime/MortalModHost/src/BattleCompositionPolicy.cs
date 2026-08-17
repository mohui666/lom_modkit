using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>Battle v3 三方总人数/官方具名角色的纯校验与解析规则。</summary>
    internal static class BattleCompositionPolicy
    {
        private static readonly HashSet<string> OfficialCharacters =
            new HashSet<string>(StringComparer.Ordinal)
            {
                "brother1", "brother2", "brother4", "girl4", "girl9", "sister1",
                "special3", "special4", "special102", "special103", "special401", "special811"
            };

        internal static List<string> ParseCharacters(string encoded, int total)
        {
            if (total < 0 || total > 10000)
                throw new ArgumentOutOfRangeException(nameof(total), "战役总人数必须在 0..10000");
            var result = new List<string>();
            var seen = new HashSet<string>(StringComparer.Ordinal);
            if (!string.IsNullOrEmpty(encoded))
            {
                string[] rows = encoded.Split(',');
                for (int i = 0; i < rows.Length; i++)
                {
                    string id = rows[i];
                    if (!OfficialCharacters.Contains(id))
                        throw new InvalidOperationException("不支持的官方战役人物：" + id);
                    if (!seen.Add(id))
                        throw new InvalidOperationException("战役具名人物不能重复：" + id);
                    result.Add(id);
                }
            }
            if (result.Count > total)
                throw new InvalidOperationException("具名角色数量不能超过该方总人数");
            return result;
        }

        internal static string AssetToken(string id)
        {
            if (!OfficialCharacters.Contains(id))
                throw new InvalidOperationException("不支持的官方战役人物：" + id);
            if (id == "girl4") return "girl004";
            if (id == "girl9") return "girl009";
            if (id == "sister1") return "sister001";
            if (id == "special3") return "special003";
            return id;
        }

        /// <summary>
        /// 只接受 catalog 中已核对的角色资源段、Animator 名或该角色自己的动画片段名。
        /// 不做任意位置的 contains，避免 special3/brother3 一类相似 ID 串错资源。
        /// </summary>
        internal static bool IsVerifiedAssetIdentity(string id, string candidate)
        {
            string token = AssetToken(id);
            string value = NormalizeAssetName(candidate);
            if (value == token || value == token + "animator") return true;
            string[] animationSuffixes =
            {
                "attack", "idle", "hurt", "run", "walk", "defence", "defense",
                "block", "dodge", "die", "skill", "ultimate", "stand"
            };
            for (int i = 0; i < animationSuffixes.Length; i++)
                if (value.StartsWith(token + animationSuffixes[i], StringComparison.Ordinal))
                    return true;
            return false;
        }

        private static string NormalizeAssetName(string value)
        {
            var chars = new List<char>();
            foreach (char c in (value ?? "").ToLowerInvariant())
                if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) chars.Add(c);
            return new string(chars.ToArray());
        }
    }
}
