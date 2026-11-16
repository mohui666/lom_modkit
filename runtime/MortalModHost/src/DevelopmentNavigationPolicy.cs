using System;

namespace MortalModHost
{
    internal static class DevelopmentNavigationPolicy
    {
        internal static void ValidatePreview(ModPackage package, PreviewRequest request)
        {
            if (!request.DeveloperMode) return;
            if (!request.FreshCampaign || request.ModId != "lom_modkit_preview"
                || !ModDisclosurePolicy.IsDevelopmentPreviewPackage(package)
                || package.Campaign == null || !package.Campaign.NewGame
                || package.Campaign.DisableOfficialEvents)
                throw new InvalidOperationException("开发测试必须使用允许原版事件的编辑器临时隔离战役");
        }

        internal static bool AllowsContext(ModPlayModeKind mode, string slot,
            string previewSlot, string previewCampaign, string activeMod, string activeCampaign,
            bool officialEventsEnabled)
        {
            if (mode == ModPlayModeKind.Addon) return ModSaveSlotPolicy.IsAddonSlot(slot);
            return mode == ModPlayModeKind.Campaign && officialEventsEnabled
                && !string.IsNullOrEmpty(previewSlot) && slot == previewSlot
                && activeMod == "lom_modkit_preview" && activeCampaign == previewCampaign
                && CampaignIdentity.OwnsSlot(previewCampaign, slot);
        }

        internal static bool IsDate(int year, int month, int stage)
        {
            return year >= 1 && year <= 9 && month >= 1 && month <= 12 && stage >= 1 && stage <= 3;
        }
    }
}
