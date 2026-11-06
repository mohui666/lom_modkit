using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using BepInEx;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;

namespace MortalModHost
{
    // Read-only evidence for returns whose addresses belong to the loaded save.
    // A new/load reset starts a session, so an older save's return is never current.
    internal static class RouteRuntimeCapture
    {
        private static string _session = Guid.NewGuid().ToString("N");
        private static readonly List<object> Transitions = new List<object>();

        internal static bool Tracks(string script)
        {
            return string.Equals(script, "S0019_01", StringComparison.OrdinalIgnoreCase)
                || string.Equals(script, "S2504_01", StringComparison.OrdinalIgnoreCase)
                || string.Equals(script, "S8500_01_06", StringComparison.OrdinalIgnoreCase)
                || string.Equals(script, "talk_girl5_check", StringComparison.OrdinalIgnoreCase);
        }

        internal static void Reset()
        {
            _session = Guid.NewGuid().ToString("N");
            Transitions.Clear();
            Publish("", "", "", "inactive");
        }

        internal static void Capture(bool enteringStory = false)
        {
            PlayerStatManagerData state = PlayerStatManagerData.Instance;
            if (state == null) { Publish("", "", "", "inactive"); return; }
            string script = state.CurrentStoryScript;
            bool active = (enteringStory || state.CurrentScene == "Story") && VanillaStoryPatch.ValidTarget(script);
            Publish(active ? script : "", active ? state.GetTempScript(1) : "",
                active ? state.CurrentTravelScript : "", active ? "story" : "inactive");
        }

        internal static void Observe(string from, string target)
        {
            if (!Tracks(from) || !VanillaStoryPatch.ValidTarget(target)) return;
            Transitions.Add(new Dictionary<string, object> {
                {"from", from}, {"target", target}, {"captured_utc", Now()}
            });
            if (Transitions.Count > 32) Transitions.RemoveAt(0);
            Capture();
        }

        private static string Now() { return DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture); }

        private static void Publish(string script, string temp, string travel, string status)
        {
            var snapshot = new Dictionary<string, object> {
                {"schema", 1}, {"game_dir", Paths.GameRootPath}, {"session", _session},
                {"captured_utc", Now()}, {"status", status}, {"script", script ?? ""},
                {"temp_script1", temp ?? ""}, {"travel_script", travel ?? ""},
                {"transitions", Transitions}
            };
            string root = VanillaBookmarkStore.Root;
            string path = Path.Combine(root, "route-runtime.json");
            string temporary = path + ".tmp";
            try
            {
                Directory.CreateDirectory(root);
                using (var writer = new StreamWriter(temporary, false, new UTF8Encoding(false)))
                    writer.Write(ScenarioState.ToJson(snapshot));
                if (File.Exists(path)) File.Replace(temporary, path, null);
                else File.Move(temporary, path);
            }
            catch (Exception error)
            {
                // Observing a route must never interrupt the game's own jump.
                if (LuaManagerPatch.Log != null) LuaManagerPatch.Log.LogWarning("路线运行快照写入失败：" + error.Message);
            }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "Init")]
    internal static class RouteRuntimeEnterPatch
    {
        private static void Prefix() { RouteRuntimeCapture.Capture(true); }
    }

    [HarmonyPatch(typeof(LuaManager), "SetNextScript")]
    internal static class RouteRuntimeNextPatch
    {
        private static void Prefix(LuaManager __instance, out string __state) { __state = __instance.ScriptName; }
        private static void Postfix(LuaManager __instance, string script, string __state)
        {
            if (string.Equals(__instance.ScriptName, script, StringComparison.OrdinalIgnoreCase))
                RouteRuntimeCapture.Observe(__state, script);
        }
    }

    [HarmonyPatch(typeof(PlayerStatManagerData), "SetTempScript")]
    internal static class RouteRuntimeTempPatch
    {
        private static void Postfix(PlayerStatManagerData __instance)
        {
            if (RouteRuntimeCapture.Tracks(__instance.CurrentStoryScript)) RouteRuntimeCapture.Capture();
        }
    }

    [HarmonyPatch(typeof(PlayerStatManagerData), "SetTravelScript")]
    internal static class RouteRuntimeTravelPatch
    {
        private static void Postfix(PlayerStatManagerData __instance)
        {
            if (RouteRuntimeCapture.Tracks(__instance.CurrentStoryScript)) RouteRuntimeCapture.Capture();
        }
    }

    [HarmonyPatch(typeof(PlayerStatManagerData), "ResetData")]
    internal static class RouteRuntimeResetPatch
    {
        private static void Postfix() { RouteRuntimeCapture.Reset(); }
    }

    [HarmonyPatch(typeof(PlayerStatManagerData), "SetCurrentScene")]
    internal static class RouteRuntimeScenePatch
    {
        private static void Postfix(string scene)
        {
            if (scene != "Story") RouteRuntimeCapture.Capture();
        }
    }
}
