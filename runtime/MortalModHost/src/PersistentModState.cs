using System;
using System.Collections.Generic;
using System.Reflection;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    /// <summary>把当前 SaveSystem 隔离槽绑定到纯 C# sidecar，并向 MOD Lua 暴露整数状态。</summary>
    internal static class PersistentModState
    {
        internal static ManualLogSource Log;
        private static string _root;
        private sealed class BoundState
        {
            internal ModPackage Package;
            internal PersistentStateStore Store;
        }
        private static readonly Dictionary<string, BoundState> Stores =
            new Dictionary<string, BoundState>(StringComparer.Ordinal);
        private static ModPackage _package;

        internal static void Initialize(string root)
        {
            if (string.IsNullOrWhiteSpace(root)) throw new ArgumentException("持久变量目录不能为空");
            _root = root;
            Stores.Clear();
            _package = null;
        }

        internal static int Get(ModPackage package, string key)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore(package).Get(package, slot, key);
        }

        internal static void Set(ModPackage package, string key, int value)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore(package).Set(package, slot, key, value);
        }

        internal static int Add(ModPackage package, string key, int delta)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore(package).Add(package, slot, key, delta);
        }

        internal static bool TryGetCharacterTitle(ModPackage package, string character, out string title)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore(package).TryGetCharacterTitle(package, slot, character, out title);
        }

        internal static void SetCharacterTitle(ModPackage package, string character, string title)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore(package).SetCharacterTitle(package, slot, character, title);
        }

        internal static bool TryGetCharacterName(ModPackage package, string character, out string name)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore(package).TryGetCharacterName(package, slot, character, out name);
        }

        internal static void SetCharacterName(ModPackage package, string character, string name)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore(package).SetCharacterName(package, slot, character, name);
        }

        internal static void BeginNewCampaign(ModPackage package)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore(package).BeginNewCampaign(package, slot);
        }

        internal static void OnSlotChanged()
        {
            Stores.Clear();
            _package = null;
        }

        internal static void FlushCurrent()
        {
            SaveSystem saves = SaveSystem.Instance;
            if (saves != null) FlushToSlot(saves.CurrentSlot);
        }

        internal static void FlushToSlot(string savedSlot)
        {
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null || !ModSaveSlotPolicy.IsModSlot(savedSlot)) return;
            if (Plugin.LoadedMods != null)
                foreach (ModPackage package in Plugin.LoadedMods)
                    if (ModPlayModeSession.Allows(package) && Owns(package, saves.CurrentSlot))
                        RequireStore(package).Open(package, saves.CurrentSlot);
            foreach (BoundState state in Stores.Values)
            {
                if (!Owns(state.Package, saves.CurrentSlot) || !Owns(state.Package, savedSlot)) continue;
                state.Store.SaveToSlot(state.Package, savedSlot);
            }
        }

        internal static IReadOnlyDictionary<string, int> Snapshot()
        {
            BoundState state;
            return _package == null || !Stores.TryGetValue(_package.Id, out state)
                ? new Dictionary<string, int>() : state.Store.Snapshot();
        }

        internal static void ResetMemory()
        {
            OnSlotChanged();
        }

        private static PersistentStateStore RequireStore(ModPackage package)
        {
            if (string.IsNullOrEmpty(_root))
                throw new InvalidOperationException("持久变量存储尚未初始化");
            BoundState state;
            if (!Stores.TryGetValue(package.Id, out state))
            {
                state = new BoundState { Package = package, Store = new PersistentStateStore(_root) };
                Stores.Add(package.Id, state);
            }
            state.Package = package;
            return state.Store;
        }

        private static bool Owns(ModPackage package, string slot)
        {
            return package != null && (CampaignIdentity.OwnsSlot(package.CampaignId, slot)
                || (package.PlayMode == "addon" && ModSaveSlotPolicy.IsAddonSlot(slot)));
        }

        private static string CurrentSlot(ModPackage package)
        {
            if (package == null) throw new InvalidOperationException("当前 MOD 包为空");
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null) throw new InvalidOperationException("SaveSystem 尚未就绪");
            return saves.CurrentSlot;
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "SetSlot")]
    internal static class PersistentStateSlotPatch
    {
        private static void Postfix(SaveSystem __instance)
        {
            PersistentModState.OnSlotChanged();
            ModSaveIsolation.ObserveSlot(__instance != null ? __instance.CurrentSlot : "");
        }
    }

    [HarmonyPatch]
    internal static class PersistentStateSavePatch
    {
        // SaveGameData() 内部调用 SaveGameData(_currentSlot)。只挂 string 重载，既覆盖
        // 菜单直接保存，也覆盖 SaveGamePanel 写入当前 MOD 隔离槽的确认操作。
        internal static MethodBase TargetMethod()
        {
            return AccessTools.Method(
                typeof(SaveSystem), nameof(SaveSystem.SaveGameData), new Type[] { typeof(string) });
        }

        private static void Prefix(out bool __state)
        {
            __state = CombatPlayerStatSession.SuspendForSave();
        }

        private static void Postfix(SaveSystem __instance, string slot)
        {
            try
            {
                PersistentModState.FlushToSlot(slot);
                ModSaveCheckpointHooks.AfterSave(__instance, slot);
            }
            catch (Exception ex)
            {
                if (PersistentModState.Log != null)
                    PersistentModState.Log.LogError("MOD 持久变量 sidecar 保存失败：" + ex);
            }
        }

        private static Exception Finalizer(bool __state, Exception __exception)
        {
            try
            {
                CombatPlayerStatSession.ResumeAfterSave(__state);
            }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogError("MOD 存档后恢复赵活决斗基准失败：" + ex);
            }
            return __exception;
        }
    }

    /// <summary>
    /// SaveGameData() 和私有 AutoSaveData(string) 都必须把 MOD Gameplay 上下文和
    /// 原版存档写在同一次成功保存之后。否则重启后只能看到 runtime scene key，不能
    /// 重建原来的 Combat/Battle。
    /// </summary>
    internal static class ModSaveCheckpointHooks
    {
        internal static void AfterSave(SaveSystem saves, string savedSlot)
        {
            if (saves == null || !ModSaveSlotPolicy.IsModSlot(savedSlot)) return;
            if (ModSaveSlotPolicy.IsAddonSlot(savedSlot))
                AddonSaveDependencies.Record(savedSlot, Plugin.LoadedMods);
            ModPackage package = Plugin.FindPackageForSlot(savedSlot);
            if (ModSaveSlotPolicy.IsAddonSlot(savedSlot))
            {
                package = Plugin.LoadedMods != null
                    ? Plugin.LoadedMods.Find(p => p.PlayMode == "addon" && GameplaySession.BelongsTo(p)) : null;
                if (package == null && !GameplaySession.HasPending)
                {
                    ModPackage current = ModOverlay.CurrentPackage;
                    if (current != null && current.PlayMode == "addon") package = current;
                }
                if (package == null)
                {
                    // Clear the old slot context when this snapshot has no active
                    // add-on scene; it must not resume a battle from an older save.
                    if (Plugin.LoadedMods != null)
                        foreach (ModPackage addon in Plugin.LoadedMods)
                            if (addon.PlayMode == "addon") GameplayCheckpointStore.Clear(addon, savedSlot);
                    return;
                }
            }
            if (package == null)
            {
                PersistentModState.Log?.LogWarning(
                    "MOD 存档已成功写入，但找不到对应包，未写入 Gameplay 上下文：" + savedSlot);
                return;
            }
            // SaveGamePanel 可以把当前战役从 001 切换保存到 002~020，但官方
            // SaveSystem.CurrentSlot 不会随 SaveGameData(string) 改写。手动档的
            // context 必须绑定目标栏位本身；只有 auto* 才需要保留产生它的来源栏位。
            bool automatic = ModSaveSlotPolicy.IsAddonSlot(savedSlot)
                ? !ModSaveSlotPolicy.IsAddonManualSlot(savedSlot)
                : ModSaveSlotPolicy.IsIsolatedAutoSlotForCampaign(package.CampaignId, savedSlot);
            string sourceSlot = automatic ? saves.CurrentSlot : savedSlot;
            if (!(package.PlayMode == "addon" && ModSaveSlotPolicy.IsAddonSlot(sourceSlot))
                && !CampaignIdentity.OwnsSlot(package.CampaignId, sourceSlot))
                throw new InvalidOperationException("MOD 存档来源槽不属于当前战役：" + sourceSlot);
            GameplayCheckpointStore.Save(package, savedSlot, sourceSlot);
            PersistentModState.Log?.LogInfo(
                GameplaySession.HasPending
                    ? "MOD Gameplay 上下文已绑定存档：" + savedSlot
                    : "MOD Gameplay 上下文已清理：" + savedSlot);
        }
    }

    [HarmonyPatch]
    internal static class PersistentStateAutoSavePatch
    {
        internal static MethodBase TargetMethod()
        {
            return AccessTools.Method(typeof(SaveSystem), "AutoSaveData",
                new Type[] { typeof(string) });
        }

        private static void Postfix(SaveSystem __instance, string slot)
        {
            try
            {
                PersistentModState.FlushToSlot(slot);
                ModSaveCheckpointHooks.AfterSave(__instance, slot);
            }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogError("MOD 自动存档上下文保存失败：" + ex);
            }
        }
    }
}
