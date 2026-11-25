using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using BepInEx;
using BepInEx.Configuration;
using BepInEx.Logging;
using BepInEx.Unity.Mono.Bootstrap;
using Fungus;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    internal static class CoffeeCompanionCompatibility
    {
        internal const string Guid = "LOMAssistant";
        private const string LuaSuffix = ".LuaScript";
        private const string ModSaveMessage = "当前是 MOD 进度，请使用游戏菜单里的 MOD 存读档。咖啡伴侣的随时存档用于纯原版。";
        private static readonly Version ReviewedDiagnosticsVersion = new Version(3, 8, 2, 0);
        private static readonly HashSet<Type> ReviewedSharedPatches = new HashSet<Type>
        {
            typeof(IsolatedUniverseSavePatch), typeof(ModSaveGamePanelOpenPatch),
            typeof(NativeSaveBrowserDeleteConfirmPatch), typeof(IsolatedStoryAutoSavePatch),
            typeof(IsolatedFreeAutoSavePatch), typeof(IsolatedBattleAutoSavePatch),
            typeof(CustomPictureShowPatch), typeof(CustomBigPictureShowPatch),
            typeof(PlayerAbilityValues.AbilityLevelText), typeof(CustomDiceLiteralTextPatch),
            typeof(GameplayMusic), typeof(CombatBackgroundOverridePatch), typeof(ScenarioReturnPatch)
        };

        private static Type _saveBrowser;
        private static FieldInfo _browserRoot, _savedSlot, _savedRootScript, _savedCurrentScript;
        private static MethodInfo _instrumentEvents, _notice;
        private static Assembly _companionAssembly;

        internal static bool IsLoaded { get { return _companionAssembly != null; } }
        private static bool ModMode { get { return ModPlayModeSession.Selected != ModPlayModeKind.Original; } }

        internal static void Install(Harmony harmony, ManualLogSource log)
        {
            _companionAssembly = null;
            if (UnityChainloader.Instance == null
                || !UnityChainloader.Instance.Plugins.TryGetValue(Guid, out PluginInfo plugin)
                || plugin.Instance == null) return;
            Assembly assembly = _companionAssembly = plugin.Instance.GetType().Assembly;

            Type detector = assembly.GetType("LOMAssistant.ConflictDetector");
            if (detector != null && assembly.GetName().Version == ReviewedDiagnosticsVersion)
                PatchPrefix(harmony, log, detector, "Intrudes", nameof(CheckSharedPatches),
                    typeof(Patches), typeof(string), typeof(string));

            // Only wrap the companion's own methods. The vanilla panels and Lua
            // prefixes stay available to it in all other circumstances.
            _saveBrowser = assembly.GetType("LOMAssistant.Features.Extras.SaveBrowser");
            if (_saveBrowser != null)
            {
                _browserRoot = AccessTools.Field(_saveBrowser, "_root");
                PatchPrefix(harmony, log, _saveBrowser, "Open", nameof(OpenSaveBrowser),
                    typeof(MonoBehaviour), typeof(bool));
                PatchPrefix(harmony, log, _saveBrowser, "Refresh", nameof(UpdateSaveBrowser));
                PatchPrefix(harmony, log, _saveBrowser, "Reselect", nameof(UpdateSaveBrowser));
            }

            Type anytime = assembly.GetType("LOMAssistant.Features.AnytimeSave.AnytimeSave");
            Type snapshot = assembly.GetType("LOMAssistant.Features.AnytimeSave.SaveFile");
            Type events = assembly.GetType("LOMAssistant.EventPicker");
            if (anytime == null || snapshot == null || events == null) return;
            _savedSlot = AccessTools.Field(snapshot, "nativeSlot");
            _savedRootScript = AccessTools.Field(snapshot, "rootScript");
            _savedCurrentScript = AccessTools.Field(snapshot, "currentScript");
            _notice = AccessTools.Method(anytime, "Notice", new[] { typeof(string), typeof(float) });
            _instrumentEvents = AccessTools.Method(events, "Instrument", new[] { typeof(string), typeof(string) });

            PatchPrefix(harmony, log, anytime, "CanSave", nameof(CanSave),
                typeof(string).MakeByRefType());
            if (_notice != null && _savedSlot != null && _savedRootScript != null && _savedCurrentScript != null)
                PatchPrefix(harmony, log, anytime, "LoadFrom", nameof(LoadSnapshot), snapshot);
            if (_instrumentEvents != null)
                PatchPrefix(harmony, log, anytime, "PrepareScript", nameof(PrepareScript),
                    typeof(LuaEnvironment), typeof(string), typeof(string));
        }

        private static void PatchPrefix(Harmony harmony, ManualLogSource log, Type owner,
            string method, string prefix, params Type[] parameters)
        {
            MethodInfo target = AccessTools.Method(owner, method, parameters);
            if (target == null)
            {
                log.LogWarning("咖啡伴侣兼容接口缺失：" + owner.Name + "." + method);
                return;
            }
            harmony.Patch(target, prefix: new HarmonyMethod(typeof(CoffeeCompanionCompatibility), prefix));
        }

        // ConfigElement.Value persists immediately. Only restore keys named in
        // the recorded warning from the previous Host, and only once.
        internal static void RecoverCoffeeHotkeys(ConfigFile hostConfig, ManualLogSource log, ISet<KeyCode> hostKeys)
        {
            const string warningPrefix = "检测到「MortalModHost」的快捷键和本 Mod 撞了，本 Mod 的";
            var done = hostConfig.Bind("Migration", "CoffeeHotkeyRecoveryCompleted", false,
                "咖啡伴侣联用时，仅一次恢复其冲突提示明确记录的旧默认快捷键。");
            if (done.Value) return;

            string path = Path.Combine(Path.GetDirectoryName(_companionAssembly.Location), "StoryReview", "compat-warnings");
            string[] warnings;
            try { warnings = File.Exists(path) ? File.ReadAllLines(path) : Array.Empty<string>(); }
            catch (Exception error) when (error is IOException || error is UnauthorizedAccessException)
            {
                log.LogWarning("无法读取咖啡伴侣的旧快捷键冲突记录：" + error.Message);
                return;
            }
            var yielded = new HashSet<string>(StringComparer.Ordinal);
            foreach (string line in warnings)
            {
                if (!line.StartsWith(warningPrefix, StringComparison.Ordinal)) continue;
                int end = line.IndexOf("热键已让给它", warningPrefix.Length, StringComparison.Ordinal);
                if (end >= 0) yielded.UnionWith(line.Substring(warningPrefix.Length, end - warningPrefix.Length).Split('、'));
            }
            Type config = _companionAssembly.GetType("LOMAssistant.Config.ConfigManager");
            if (config != null)
            {
                RestoreCoffeeKey(config, yielded, hostKeys, "PanelKey", "设置页", KeyCode.F8, log);
                RestoreCoffeeKey(config, yielded, hostKeys, "SaveNoteKey", "存档备注", KeyCode.F8, log);
                RestoreCoffeeKey(config, yielded, hostKeys, "AnytimeLoadKey", "随时存档：读取最新存档", KeyCode.F7, log);
            }
            done.Value = true;
        }

        private static void RestoreCoffeeKey(Type config, ISet<string> yielded, ISet<KeyCode> hostKeys,
            string field, string label, KeyCode key, ManualLogSource log)
        {
            if (!yielded.Contains(label) || hostKeys.Contains(key)) return;
            object entry = AccessTools.Field(config, field)?.GetValue(null);
            if (entry == null) return;
            PropertyInfo value = AccessTools.Property(entry.GetType(), "Value");
            if (value?.GetValue(entry, null) is KeyCode current && current == KeyCode.None)
            {
                value.SetValue(entry, key, null);
                log.LogInfo("已恢复咖啡伴侣快捷键：" + label + " " + key);
            }
        }

        private static bool CheckSharedPatches(Patches info, string owner, string ours, ref bool __result)
        {
            if (ours != Guid || (owner != Plugin.GUID && owner != Plugin.GUID + ".steamrestart")) return true;
            // Compare the actual patch owners, so newly added or third-party
            // hooks still go through Coffee's conflict detector.
            var patches = info.Prefixes.Concat(info.Postfixes).Concat(info.Transpilers).Concat(info.Finalizers)
                .Where(patch => patch.owner == owner);
            bool seen = false;
            foreach (Patch patch in patches)
            {
                MethodInfo method = patch.PatchMethod;
                if (method == null || (!ReviewedSharedPatches.Contains(method.DeclaringType)
                    && !(owner == Plugin.GUID + ".steamrestart" && method.DeclaringType == typeof(Plugin)
                        && method.Name == "SkipSteamRestart"))) return true;
                seen = true;
            }
            if (!seen) return true;
            __result = false;
            return false;
        }

        internal static Action SuspendSaveBrowser(MonoBehaviour panel)
        {
            if (_saveBrowser == null || panel == null) return null;
            Behaviour browser = panel.GetComponent(_saveBrowser) as Behaviour;
            if (browser == null) return null;
            RectTransform root = _browserRoot?.GetValue(browser) as RectTransform;
            bool wasEnabled = browser.enabled;
            bool wasVisible = root != null && root.gameObject.activeSelf;
            browser.enabled = false;
            if (root != null) root.gameObject.SetActive(false);
            return delegate
            {
                if (browser != null) browser.enabled = wasEnabled;
                if (root != null) root.gameObject.SetActive(wasVisible);
            };
        }

        private static bool OpenSaveBrowser(ref bool ____opening)
        {
            if (!ModMode) return true;
            ____opening = false;
            return false;
        }

        private static bool UpdateSaveBrowser(Behaviour __instance) { return __instance.enabled; }

        private static bool CanSave(ref string reason, ref bool __result)
        {
            if (!ModMode && !ModSaveSlotPolicy.IsModSlot(SaveSystem.Instance?.CurrentSlot)) return true;
            reason = ModSaveMessage;
            __result = false;
            return false;
        }

        private static bool LoadSnapshot(object __instance, object __0)
        {
            if (__0 == null) return true;
            bool modSnapshot = ModSaveSlotPolicy.IsModSlot((string)_savedSlot.GetValue(__0))
                || IsModScript((string)_savedRootScript.GetValue(__0))
                || IsModScript((string)_savedCurrentScript.GetValue(__0));
            if (!modSnapshot && !ModMode) return true;
            string message = modSnapshot
                ? "这个随时存档引用了 MOD 进度，请从对应玩法的普通 MOD 存档继续。"
                : "请返回标题后，在纯原版中读取咖啡伴侣的随时存档。";
            _notice.Invoke(__instance, new object[] { message, 8f });
            return false;
        }

        private static bool IsModScript(string name)
        {
            return name != null && name.StartsWith("MOD_", StringComparison.Ordinal);
        }

        private static bool PrepareScript(string code, string name, ref string __result)
        {
            if (!ModMode) return true;
            // The event picker can still wrap these calls. A Coffee replay cannot
            // reconstruct the Host's MOD package identity or campaign state.
            __result = name != null && name.EndsWith(LuaSuffix, StringComparison.Ordinal) && name != "tmp.LuaScript"
                ? (string)_instrumentEvents.Invoke(null, new object[] { code, name.Substring(0, name.Length - LuaSuffix.Length) })
                : code;
            return false;
        }
    }
}
