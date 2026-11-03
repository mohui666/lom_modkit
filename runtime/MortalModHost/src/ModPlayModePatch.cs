using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    // These four methods are the live TitleManager's original-game entry points.
    // The two MOD browsers call SaveSystem directly and never pass through them.
    [HarmonyPatch]
    internal static class OriginalGameModePatch
    {
        private static IEnumerable<MethodBase> TargetMethods()
        {
            foreach (string name in new[] { "OpenSlot", "NewGame", "LoadGame", "ContinueGame" })
                yield return AccessTools.Method(typeof(TitleManager), name);
        }

        private static void Prefix()
        {
            if (Plugin.Instance != null) Plugin.Instance.EnterOriginalMode();
        }
    }
}
