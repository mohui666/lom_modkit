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
            if (id == "special3") return "special003";
            if (id == "special4") return "special004";
            return id;
        }
    }
}
