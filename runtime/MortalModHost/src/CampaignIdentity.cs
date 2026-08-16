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
            return IsValid(campaignId)
                && string.Equals(slot, SaveSlot(campaignId), StringComparison.Ordinal);
        }
    }
}
