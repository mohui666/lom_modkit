using System;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    internal static class ModSavePanel
    {
        internal static bool IsActive
        {
            get { return ModSaveBrowser.Current != null && ModSaveBrowser.Current.Saving; }
        }

        internal static bool TryOpen(SaveGamePanel panel)
        {
            SaveSystem saves = SaveSystem.Instance;
            if (panel == null || saves == null
                || ModPlayModeSession.Selected == ModPlayModeKind.Original) return false;
            bool addon = ModPlayModeSession.Selected == ModPlayModeKind.Addon;
            ModPackage package = null;
            if (addon)
            {
                if (!ModSaveSlotPolicy.IsAddonSlot(saves.CurrentSlot)) return false;
            }
            else
            {
                if (!ModCampaignState.Active
                    || !CampaignIdentity.OwnsSlot(ModCampaignState.ActiveCampaignId, saves.CurrentSlot))
                    return false;
                package = Plugin.FindCampaignPackage(ModCampaignState.ActiveCampaignId);
                if (package == null || package.Id != ModCampaignState.ActiveModId) return false;
            }
            try
            {
                if (ModNativeSaveBrowser.TryOpenSave(panel, package, addon)) return true;
                ModSaveBrowser.ShowSave(panel, package, addon);
                return true;
            }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogError("打开 MOD 保存页失败：" + ex);
                VanillaModCampaignPanel.Remove();
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(SaveGamePanel), "OnPanelOpen")]
    internal static class ModSaveGamePanelOpenPatch
    {
        private static bool Prefix(SaveGamePanel __instance)
        {
            return !ModSavePanel.TryOpen(__instance);
        }
    }
}
