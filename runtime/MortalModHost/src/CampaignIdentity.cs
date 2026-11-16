using System;

namespace MortalModHost
{
    /// <summary>
    /// 自定义战役的稳定身份与存档命名规则。campaign_id 是存档身份，不能用可能随
    /// 重命名/重新打包而变化的包文件名，也不能静默回退 manifest.id。
    /// </summary>
    internal static class CampaignIdentity
    {
        internal const int MaxLength = 64;

        internal static bool IsValid(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > MaxLength) return false;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')
                    || c == '_' || c == '-')) return false;
            }
            return true;
        }

        internal static string SaveSlot(string campaignId)
        {
            if (!IsValid(campaignId))
                throw new ArgumentException(
                    "campaign_id 必须匹配 [a-z0-9_-]{1,64}", nameof(campaignId));
            // v3 使用全新命名空间；绝不探测旧版 mod_<manifest.id>，即使作者恰好
            // 让 campaign_id == id 也不会误读没有稳定身份的旧存档。
            return "mod_campaign_" + campaignId;
        }

        internal static bool OwnsSlot(string campaignId, string slot)
        {
            if (!IsValid(campaignId) || string.IsNullOrEmpty(slot)) return false;

            // 先按已知 campaign_id 精确匹配。仅靠 TryParseSlot 无法区分
            // campaign_id="chapter_s002" 的主槽和普通 campaign_id 的 002 槽。
            const string prefix = "mod_campaign_";
            int end = prefix.Length + campaignId.Length;
            if (slot.Length < end || !slot.StartsWith(prefix, StringComparison.Ordinal)
                || string.CompareOrdinal(slot, prefix.Length, campaignId, 0, campaignId.Length) != 0)
                return false;
            int suffixLength = slot.Length - end;
            if (suffixLength == 0) return true;
            if (suffixLength == 5 && string.CompareOrdinal(slot, end, "_auto", 0, 5) == 0
                || suffixLength == 10 && string.CompareOrdinal(slot, end, "_auto_free", 0, 10) == 0
                || suffixLength == 12 && string.CompareOrdinal(slot, end, "_auto_battle", 0, 12) == 0)
                return true;
            // Only _s002 through _s020 are generated manual slots. Do not use
            // TryParseSlot here: its historical parser also accepts numeric signs.
            return suffixLength == 5 && slot[end] == '_' && slot[end + 1] == 's'
                && slot[end + 2] == '0'
                && (slot[end + 3] == '0' && slot[end + 4] >= '2' && slot[end + 4] <= '9'
                    || slot[end + 3] == '1' && slot[end + 4] >= '0' && slot[end + 4] <= '9'
                    || slot[end + 3] == '2' && slot[end + 4] == '0');
        }

        internal static bool TryParseSlot(string slot, out string campaignId)
        {
            campaignId = "";
            const string prefix = "mod_campaign_";
            if (string.IsNullOrEmpty(slot)
                || !slot.StartsWith(prefix, StringComparison.Ordinal))
                return false;
            string id = slot.Substring(prefix.Length);
            if (id.EndsWith("_auto_battle", StringComparison.Ordinal))
                id = id.Substring(0, id.Length - "_auto_battle".Length);
            else if (id.EndsWith("_auto_free", StringComparison.Ordinal))
                id = id.Substring(0, id.Length - "_auto_free".Length);
            else if (id.EndsWith("_auto", StringComparison.Ordinal))
                id = id.Substring(0, id.Length - "_auto".Length);
            if (id.Length >= 5)
            {
                int mark = id.LastIndexOf("_s", StringComparison.Ordinal);
                if (mark > 0 && mark == id.Length - 5)
                {
                    int index;
                    if (int.TryParse(id.Substring(mark + 2), out index)
                        && index >= 2 && index <= 20)
                        id = id.Substring(0, mark);
                }
            }
            if (!IsValid(id)) return false;
            campaignId = id;
            return true;
        }
    }
}
