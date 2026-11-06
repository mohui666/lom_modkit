using System;
using System.Collections.Generic;
using System.Linq;
using Fungus;
using HarmonyLib;
using MoonSharp.Interpreter;
using Mortal.Story;

namespace MortalModHost
{
    // Resource/table add-ons apply to the game's existing script. They do not
    // need an exported copy of that script or a fabricated vanilla override.
    internal static class LegacyOriginalContext
    {
        private static ModPackage _package;
        private static LuaEnvironment _environment;
        private static Script _interpreter;

        internal static bool HasContent(ModPackage package)
        {
            return LegacyExtensionsConfig.HasOriginalContent(package);
        }

        private static List<ModPackage> Eligible()
        {
            return Plugin.LoadedMods == null ? new List<ModPackage>()
                : Plugin.LoadedMods.Where(package => ModPlayModeSession.Allows(package) && HasContent(package)).ToList();
        }

        // Background portraits in Free use the same selection as story playback.
        // Ambiguity is reported at the story entry, before any package is applied.
        internal static ModPackage ActivePackage
        {
            get { var packages = Eligible(); return packages.Count == 1 ? packages[0] : null; }
        }

        internal static void Enter(LuaManager manager)
        {
            var packages = Eligible();
            if (packages.Count > 1)
                throw new InvalidOperationException("当前旧 MOD 资源／数据表会话只支持一个来源，请只启用一个附加包："
                    + string.Join("、", packages.Select(candidate => candidate.Name + " (" + candidate.Id + ")").ToArray()));
            ModPackage package = packages.FirstOrDefault();
            if (_package != null && (package == null || !Same(_package, package)))
                CustomGameplayRuntime.Clear();
            if (package == null) return;
            // EnsureDatabase may clear an earlier runtime graph. Complete that
            // operation before retaining the original interpreter for this scope.
            CustomGameplayRuntime.EnsurePackage(package);
            var environment = Traverse.Create(manager).Field("_luaEnvironment").GetValue<LuaEnvironment>();
            if (environment == null) throw new InvalidOperationException("原版旧 MOD 上下文缺少 LuaEnvironment");
            environment.InitEnvironment();
            if (environment.Interpreter == null) throw new InvalidOperationException("原版旧 MOD 上下文缺少解释器");
            _package = package;
            _environment = environment;
            _interpreter = environment.Interpreter;
            ModOverlay.CurrentPackage = package;
            LegacyLuaSupport.Register(_interpreter, package);
            ModDisclosure.Enable(package);
            ModDisclosure.SetOriginalContent(false);
        }

        private static bool Same(ModPackage left, ModPackage right)
        {
            return left.Id == right.Id && left.PackageFingerprint == right.PackageFingerprint;
        }

        internal static void Clear()
        {
            ModPackage package = _package;
            LuaEnvironment environment = _environment;
            Script interpreter = _interpreter;
            _package = null;
            _environment = null;
            _interpreter = null;
            bool ownsInterpreter = environment != null && ReferenceEquals(environment.Interpreter, interpreter);
            bool ownsDisclosure = ownsInterpreter && package != null && ModDisclosure.Active
                && ModDisclosure.ModId == package.Id
                && string.Equals(ModDisclosure.PackageFingerprint, package.PackageFingerprint, StringComparison.OrdinalIgnoreCase);
            if (ownsInterpreter)
            {
                var fields = Traverse.Create(environment);
                fields.Field("interpreter").SetValue(null);
                fields.Field("initialised").SetValue(false);
            }
            if (ownsDisclosure) ModDisclosure.Disable();
            if (package != null && LuaManagerPatch.Log != null)
                LuaManagerPatch.Log.LogInfo("已释放原版附加上下文 " + package.Id
                    + "（解释器归属=" + ownsInterpreter + "，披露归属=" + ownsDisclosure + "）");
        }
    }

    [HarmonyPatch(typeof(LuaManager), "ExecuteLuaScript")]
    internal static class LegacyOriginalStoryPatch
    {
        // LuaManagerPatch first handles registered MOD scripts and resets the
        // previous MOD interpreter. Its false return also skips this prefix.
        [HarmonyPriority(Priority.Last)]
        private static bool Prefix(LuaManager __instance)
        {
            try
            {
                LegacyOriginalContext.Enter(__instance);
                return true;
            }
            catch (Exception error)
            {
                CustomGameplayRuntime.Clear();
                LuaManagerPatch.AbortActivePlayback("原版旧 MOD 资源／数据表上下文接入失败", __instance,
                    error, "legacy_original_context", registeredName: __instance.ScriptName);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(CustomGameplayRuntime), "Clear")]
    internal static class LegacyOriginalClearPatch
    {
        private static void Prefix() { LegacyOriginalContext.Clear(); }
    }
}
