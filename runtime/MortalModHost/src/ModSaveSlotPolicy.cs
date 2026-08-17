using System;

namespace MortalModHost
{
    /// <summary>不依赖 Unity/游戏程序集的 MOD 存档命名与原版槽保护规则。</summary>
    internal static class ModSaveSlotPolicy
    {
        internal static bool IsModSlot(string slot)
        {
            return !string.IsNullOrEmpty(slot)
                && slot.StartsWith("mod_campaign_", StringComparison.Ordinal);
        }

        internal static bool IsOfficialAutoSlot(string slot)
        {
            return slot == "auto" || slot == "auto_free" || slot == "auto_battle";
        }

        internal static string CampaignSlot(string campaignId)
        {
            return CampaignIdentity.SaveSlot(campaignId);
        }

        internal static string IsolatedAutoSlot(string modSlot, string officialAutoSlot)
        {
            if (!IsModSlot(modSlot))
                throw new ArgumentException("当前槽不是 MOD 隔离槽", nameof(modSlot));
            if (!IsOfficialAutoSlot(officialAutoSlot))
                throw new ArgumentException("不是受支持的原版自动槽", nameof(officialAutoSlot));
            return modSlot + "_" + officialAutoSlot;
        }

        internal static string ObserveOfficialSlot(string lastOfficialSlot, string candidate)
        {
            return IsModSlot(candidate) ? (lastOfficialSlot ?? "") : (candidate ?? "");
        }

        internal static bool ShouldHijackInGameLoad(bool titleScene, bool campaignActive, string currentSlot)
        {
            if (titleScene) return false;
            return campaignActive || IsModSlot(currentSlot);
        }

        internal static string PreferredInGameCampaignId(string activeCampaignId, string currentSlot)
        {
            if (!string.IsNullOrEmpty(activeCampaignId)) return activeCampaignId;
            string parsed;
            return CampaignIdentity.TryParseSlot(currentSlot, out parsed) ? parsed : "";
        }
    }
}
