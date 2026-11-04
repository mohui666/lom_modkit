using System;
using System.Collections.Generic;
using System.Linq;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>MOD 存读档入口。原版模式始终使用游戏自己的面板。</summary>
    internal static class VanillaModCampaignPanel
    {
        internal static bool IsActive { get { return ModSaveBrowser.Current != null || ModNativeSaveBrowser.IsActive; } }
        internal static string SelectedCampaignId
        {
            get { return ModNativeSaveBrowser.IsActive ? ModNativeSaveBrowser.SelectedCampaignId
                : ModSaveBrowser.Current != null ? ModSaveBrowser.Current.CampaignId : null; }
        }

        internal static bool Open(IList<ModPackage> packages,
            Action<ModPackage, string> startCampaign, Action<ModPackage, string> loadCampaign,
            Action<ModPackage, string> loadAutoCampaign, string recentCampaignId,
            Action<string> campaignSelected, Action<string> logInfo, Action<string> logWarning)
        {
            return OpenBrowser(packages, startCampaign, loadCampaign, loadAutoCampaign,
                recentCampaignId, "", campaignSelected, true, null, null, logWarning);
        }

        internal static bool OpenExisting(CommonPanel host, LoadGamePanel panel, IList<ModPackage> packages,
            Action<ModPackage, string> startCampaign, Action<ModPackage, string> loadCampaign,
            Action<ModPackage, string> loadAutoCampaign, string recentCampaignId, string preferredCampaignId,
            Action<string> campaignSelected, Action<string> logInfo, Action<string> logWarning)
        {
            return OpenBrowser(packages, startCampaign, loadCampaign, loadAutoCampaign,
                recentCampaignId, preferredCampaignId, campaignSelected, false, host, panel, logWarning);
        }

        private static bool OpenBrowser(IList<ModPackage> packages,
            Action<ModPackage, string> start, Action<ModPackage, string> load,
            Action<ModPackage, string> auto, string recent, string preferred, Action<string> selected,
            bool title, CommonPanel host, LoadGamePanel owner, Action<string> warning)
        {
            if (ModPlayModeSession.Selected != ModPlayModeKind.Campaign) return false;
            try
            {
                ModPackage package = packages != null
                    ? packages.FirstOrDefault(x => x.CampaignId == preferred) : null;
                if (!title && package != null && ModNativeSaveBrowser.TryOpenExisting(
                    host, owner, package, false, start, load, auto)) return true;
                ModSaveBrowser.ShowCampaigns(packages, start, load, auto, recent, preferred,
                    selected, title, host, owner);
                return true;
            }
            catch (Exception ex)
            {
                if (warning != null) warning("打开 MOD 存档页失败：" + ex.Message);
                Remove();
                return false;
            }
        }

        internal static bool OpenAddonMenu()
        {
            if (ModPlayModeSession.Selected != ModPlayModeKind.Addon) return false;
            if (ModNativeSaveBrowser.TryOpenTitle(null, true)) return true;
            ModSaveBrowser.ShowAddon(false, true, null, null);
            return true;
        }

        internal static bool OpenAddonSaves(bool save)
        {
            if (ModPlayModeSession.Selected != ModPlayModeKind.Addon) return false;
            MonoBehaviour panel = save
                ? (MonoBehaviour)Resources.FindObjectsOfTypeAll<SaveGamePanel>()
                    .FirstOrDefault(x => x.gameObject.activeInHierarchy)
                : Resources.FindObjectsOfTypeAll<LoadGamePanel>()
                    .FirstOrDefault(x => x.gameObject.activeInHierarchy);
            if (panel == null) return false;
            if (!save && ModNativeSaveBrowser.TryOpenExisting(
                panel.GetComponentInParent<CommonPanel>(), panel as LoadGamePanel, null,
                true, null, null, null)) return true;
            if (save && ModNativeSaveBrowser.TryOpenSave(panel as SaveGamePanel, null, true)) return true;
            ModSaveBrowser.ShowAddon(save, false, panel.GetComponentInParent<CommonPanel>(), panel);
            return true;
        }

        internal static void Maintain() { }
        internal static void Remove()
        {
            ModNativeSaveBrowser.Close();
            if (ModSaveBrowser.Current != null) ModSaveBrowser.Current.Close();
        }
    }

    [HarmonyPatch(typeof(MenuPanel), "LoadButtonClick")]
    internal static class InGameLoadMenuPatch
    {
        private static void Postfix(MenuPanel __instance)
        {
            if (__instance == null || VanillaModCampaignPanel.IsActive || Plugin.Instance == null) return;
            if (ModPlayModeSession.Selected == ModPlayModeKind.Addon)
                VanillaModCampaignPanel.OpenAddonSaves(false);
            else if (ModPlayModeSession.Selected == ModPlayModeKind.Campaign)
                Plugin.Instance.TryOpenInGameModLoadMenu(__instance);
        }
    }

    [HarmonyPatch(typeof(Mortal.Battle.PausePanel), "LoadButtonClick")]
    internal static class BattlePauseLoadMenuPatch
    {
        private static void Postfix(Mortal.Battle.PausePanel __instance)
        {
            if (__instance == null || VanillaModCampaignPanel.IsActive || Plugin.Instance == null) return;
            if (ModPlayModeSession.Selected == ModPlayModeKind.Addon)
                VanillaModCampaignPanel.OpenAddonSaves(false);
            else if (ModPlayModeSession.Selected == ModPlayModeKind.Campaign)
                Plugin.Instance.TryOpenInGameModLoadMenu(__instance);
        }
    }
}
