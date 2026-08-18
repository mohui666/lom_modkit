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

        internal const int OfficialManualCount = 20;

        internal static string IsolatedAutoSlot(string modSlot, string officialAutoSlot)
        {
            string campaignId;
            if (!CampaignIdentity.TryParseSlot(modSlot, out campaignId))
                throw new ArgumentException("当前槽不是 MOD 隔离槽", nameof(modSlot));
            return IsolatedAutoSlotForCampaign(campaignId, officialAutoSlot);
        }

        internal static string IsolatedAutoSlotForCampaign(
            string campaignId, string officialAutoSlot)
        {
            if (!CampaignIdentity.IsValid(campaignId))
                throw new ArgumentException("campaign_id 无效", nameof(campaignId));
            if (!IsOfficialAutoSlot(officialAutoSlot))
                throw new ArgumentException("不是受支持的原版自动槽", nameof(officialAutoSlot));
            return CampaignIdentity.SaveSlot(campaignId) + "_" + officialAutoSlot;
        }

        internal static bool IsIsolatedAutoSlotForCampaign(string campaignId, string slot)
        {
            if (!CampaignIdentity.IsValid(campaignId) || string.IsNullOrEmpty(slot))
                return false;
            return string.Equals(slot, IsolatedAutoSlotForCampaign(campaignId, "auto"),
                    StringComparison.Ordinal)
                || string.Equals(slot, IsolatedAutoSlotForCampaign(campaignId, "auto_free"),
                    StringComparison.Ordinal)
                || string.Equals(slot, IsolatedAutoSlotForCampaign(campaignId, "auto_battle"),
                    StringComparison.Ordinal);
        }

        /// <summary>
        /// 原版右侧 001～020。001 沿用已有 mod_campaign_&lt;id&gt;，002～020 为
        /// _sNNN，避免和 campaign_id 或 _auto* 撞名。
        /// </summary>
        internal static string IsolatedManualSlot(string campaignId, int index)
        {
            if (index < 1 || index > OfficialManualCount)
                throw new ArgumentOutOfRangeException(nameof(index));
            string root = CampaignIdentity.SaveSlot(campaignId);
            if (index == 1) return root;
            return root + "_s" + index.ToString("000");
        }

        internal static string OfficialManualLabel(int index)
        {
            if (index < 1 || index > OfficialManualCount)
                throw new ArgumentOutOfRangeException(nameof(index));
            return index.ToString("000");
        }

        /// <summary>
        /// 原版 AutoSaveSlotPanel.OnTitleClick 传的是预制体 _slot（auto /
        /// auto_free / auto_battle）。标题页 CurrentSlot 仍是原版槽时，必须用
        /// 当前战役 id 拼隔离名，不能对 "001" 调 IsolatedAutoSlot。
        /// </summary>
        internal static string RedirectOfficialAutoSlot(
            string officialAutoSlot,
            string currentSlot,
            string activeCampaignId,
            string panelCampaignId)
        {
            if (IsModSlot(officialAutoSlot) || !IsOfficialAutoSlot(officialAutoSlot))
                return officialAutoSlot;
            // 标题页可能仍保留上一个战役的 CurrentSlot；面板中玩家刚选定的
            // campaign 才是这次点击的所有者，其次才是活动战役，再退回当前槽。
            if (!string.IsNullOrEmpty(panelCampaignId)
                && CampaignIdentity.IsValid(panelCampaignId))
                return IsolatedAutoSlotForCampaign(panelCampaignId, officialAutoSlot);
            string parsed;
            if (IsModSlot(currentSlot))
            {
                // 已知活动战役优先于从槽名反推；这也避免 campaign_id 自身
                // 以 _s002/_auto 结尾时的解析歧义。
                if (!string.IsNullOrEmpty(activeCampaignId)
                    && CampaignIdentity.IsValid(activeCampaignId)
                    && CampaignIdentity.OwnsSlot(activeCampaignId, currentSlot))
                    return IsolatedAutoSlotForCampaign(activeCampaignId, officialAutoSlot);
                if (CampaignIdentity.TryParseSlot(currentSlot, out parsed))
                    return IsolatedAutoSlotForCampaign(parsed, officialAutoSlot);
            }
            // 当前是原版槽且没有明确的 MOD 面板选择时，不能因为上一场
            // MOD 的运行态尚未清理，就把官方 auto* 改读到 MOD 文件。
            return officialAutoSlot;
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
