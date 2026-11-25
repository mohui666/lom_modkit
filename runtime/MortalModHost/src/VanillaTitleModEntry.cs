using System.Reflection;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    // Keep the game's button, localization and navigation. Other plugins can add
    // title buttons without having the original start button replaced under them.
    [HarmonyPatch(typeof(CommonPanel), nameof(CommonPanel.Show), new[] { typeof(bool), typeof(bool) })]
    internal static class VanillaTitleModEntry
    {
        private static readonly FieldInfo TitleSlotPanel = AccessTools.Field(typeof(TitleManager), "_slotPanel");
        private static bool _openingOriginal;

        internal static void OpenOriginalGame()
        {
            TitleManager title = TitleManager.Instance;
            if (title == null) return;
            _openingOriginal = true;
            try { title.OpenSlot(); }
            finally { _openingOriginal = false; }
        }

        private static bool Prefix(CommonPanel __instance, bool show, bool invokeEvent)
        {
            // The StartGame button's persistent UnityEvent calls CommonPanel.Show
            // on this exact panel. Host-driven and in-game panel opens pass through.
            if (!show || !invokeEvent || _openingOriginal || ModNativeSaveBrowser.OwnsPanel(__instance))
                return true;
            TitleManager title = TitleManager.Instance;
            if (title == null || !ReferenceEquals(TitleSlotPanel?.GetValue(title), __instance)) return true;
            Plugin plugin = Plugin.Instance;
            return plugin == null || !plugin.TryOpenPlayModePicker();
        }
    }
}
