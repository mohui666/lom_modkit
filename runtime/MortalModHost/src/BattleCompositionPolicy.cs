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

        // 原版 BattleLevel.NameKey 已实证存在的阵营；其余只能回退 BL_0000 安全基线。
        private static readonly HashSet<string> OfficialFactions =
            new HashSet<string>(StringComparer.Ordinal)
            {
                "000", "001", "002", "003", "004", "006", "008", "009", "010",
                "100", "200", "201", "300", "500"
            };

        internal static bool HasBattleLevel(string faction)
        {
            return !string.IsNullOrEmpty(faction) && OfficialFactions.Contains(faction);
        }

        internal static List<string> ParseFactions(string encoded)
        {
            List<FactionGroup> groups = ParseFactionGroups(encoded);
            var ids = new List<string>();
            for (int i = 0; i < groups.Count; i++) ids.Add(groups[i].Id);
            return ids;
        }

        internal struct FactionGroup
        {
            internal string Id;
            internal int People;
        }

        internal static List<FactionGroup> ParseFactionGroups(string encoded)
        {
            var result = new List<FactionGroup>();
            var seen = new HashSet<string>(StringComparer.Ordinal);
            if (string.IsNullOrEmpty(encoded)) return result;
            string[] rows = encoded.Split(',');
            for (int i = 0; i < rows.Length; i++)
            {
                string row = rows[i];
                if (string.IsNullOrEmpty(row)) continue;
                string id = row;
                int people = 1;
                int split = row.LastIndexOf(':');
                if (split > 0)
                {
                    id = row.Substring(0, split);
                    if (!int.TryParse(row.Substring(split + 1), out people) || people < 1 || people > 10000)
                        throw new InvalidOperationException("战役附加兵种人数无效：" + row);
                }
                if (!OfficialFactions.Contains(id))
                    throw new InvalidOperationException("不支持的战役附加兵种：" + id);
                if (!seen.Add(id))
                    throw new InvalidOperationException("战役附加兵种不能重复：" + id);
                result.Add(new FactionGroup { Id = id, People = people });
            }
            return result;
        }

        internal static int TotalPeople(IList<FactionGroup> groups, int namedCount)
        {
            int total = namedCount;
            if (groups != null)
            {
                for (int i = 0; i < groups.Count; i++) total += groups[i].People;
            }
            return total;
        }

        internal static bool HasExplicitPeople(string encoded)
        {
            return !string.IsNullOrEmpty(encoded) && encoded.IndexOf(':') >= 0;
        }

        /// <summary>
        /// 新脚本用 id:people；旧脚本只有 id 列表并另带 friend_people。
        /// 没有显式 :people 时，把旧总人数多出来的席位补给第一个阵营。
        /// </summary>
        internal static List<FactionGroup> ResolveSideGroups(
            string encoded, int namedCount, int legacyPeople)
        {
            List<FactionGroup> groups = ParseFactionGroups(encoded);
            int computed = TotalPeople(groups, namedCount);
            if (!HasExplicitPeople(encoded) && legacyPeople > computed && groups.Count > 0)
            {
                FactionGroup first = groups[0];
                first.People += legacyPeople - computed;
                groups[0] = first;
            }
            return groups;
        }

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
            if (id == "special4") return "special004";
            return id;
        }

        /// <summary>
        /// Addressables 实证：这些 id 拥有原版 Battle Boss/Npc Animator。
        /// special4/102/103/401 是 Boss overrideController，不一定出现在
        /// BattleLevelConfig 的 NpcPresets 里；special811 在 Npc 目录。
        /// </summary>
        internal static bool HasNpcPrefabAsset(string id)
        {
            if (!OfficialCharacters.Contains(id))
                throw new InvalidOperationException("不支持的官方战役人物：" + id);
            return id == "special4" || id == "special102" || id == "special103"
                || id == "special401" || id == "special811";
        }

        internal static bool TryOfficialBattleAnimatorAddress(string id, out string address)
        {
            address = null;
            if (id == "special4")
                address = "Assets/__Project/Animations/Battle/Boss/Special_004_樊嘯天/Enemy_Special004_Animator.overrideController";
            else if (id == "special102")
                address = "Assets/__Project/Animations/Battle/Boss/Special_102_南宮深/Enemy_Special102_Animator.overrideController";
            else if (id == "special103")
                address = "Assets/__Project/Animations/Battle/Boss/Special_103_南宮淺/Enemy_Special103_Animator.overrideController";
            else if (id == "special401")
                address = "Assets/__Project/Animations/Battle/Boss/Special_401_毛二壯/Enemy_Special401_Animator.overrideController";
            else if (id == "special811")
                address = "Assets/__Project/Animations/Battle/Npc/Special811/Special811_Animator.overrideController";
            return !string.IsNullOrEmpty(address);
        }

        /// <summary>
        /// 只接受 catalog 中已核对的角色资源段、Animator 名、官方中文名或
        /// 该角色自己的动画片段名。补零/不补零都认，避免 Special4 对不上 special004。
        /// </summary>
        internal static bool IsVerifiedAssetIdentity(string id, string candidate)
        {
            if (string.IsNullOrEmpty(candidate)) return false;
            string value = NormalizeAssetName(candidate);
            string[] tokens = IdentityTokens(id);
            string[] animationSuffixes =
            {
                "attack", "idle", "hurt", "run", "walk", "defence", "defense",
                "block", "dodge", "die", "skill", "ultimate", "stand"
            };
            for (int t = 0; t < tokens.Length; t++)
            {
                string token = tokens[t];
                if (value == token || value == token + "animator") return true;
                for (int i = 0; i < animationSuffixes.Length; i++)
                    if (value.StartsWith(token + animationSuffixes[i], StringComparison.Ordinal)
                        || value.StartsWith("enemy" + token + animationSuffixes[i],
                            StringComparison.Ordinal))
                        return true;
            }
            string[] aliases = DisplayAliases(id);
            for (int i = 0; i < aliases.Length; i++)
                if (candidate.IndexOf(aliases[i], StringComparison.Ordinal) >= 0)
                    return true;
            return false;
        }

        internal static string[] IdentityTokens(string id)
        {
            string padded = AssetToken(id);
            if (string.Equals(padded, id, StringComparison.Ordinal))
                return new[] { padded };
            return new[] { padded, id };
        }

        internal static string[] DisplayAliases(string id)
        {
            if (id == "special4") return new[] { "樊嘯天", "樊啸天" };
            if (id == "special102") return new[] { "南宮深", "南宫深" };
            if (id == "special103") return new[] { "南宮淺", "南宫浅" };
            if (id == "special401") return new[] { "毛二壯", "毛二壮" };
            return new string[0];
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
