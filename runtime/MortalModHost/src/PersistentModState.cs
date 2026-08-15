using System;
using System.Collections.Generic;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    /// <summary>把当前 SaveSystem 隔离槽绑定到纯 C# sidecar，并向 MOD Lua 暴露整数状态。</summary>
    internal static class PersistentModState
    {
        internal static ManualLogSource Log;
        private static PersistentStateStore _store;
        private static ModPackage _package;

        internal static void Initialize(string root)
        {
            _store = new PersistentStateStore(root);
            _package = null;
        }

        internal static int Get(ModPackage package, string key)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore().Get(package, slot, key);
        }

        internal static void Set(ModPackage package, string key, int value)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore().Set(package, slot, key, value);
        }

        internal static int Add(ModPackage package, string key, int delta)
        {
            string slot = CurrentSlot(package);
            _package = package;
            return RequireStore().Add(package, slot, key, delta);
        }

        internal static void BeginNewCampaign(ModPackage package)
        {
            string slot = CurrentSlot(package);
            _package = package;
            RequireStore().BeginNewCampaign(package, slot);
        }

        internal static void OnSlotChanged()
        {
            if (_store != null) _store.ResetMemory();
            _package = null;
        }

        internal static void FlushCurrent()
        {
            if (_store == null || _package == null) return;
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null || !string.Equals(
                saves.CurrentSlot, "mod_" + _package.Id, StringComparison.Ordinal)) return;
            _store.Flush();
        }

        internal static IReadOnlyDictionary<string, int> Snapshot()
        {
            return _store == null
                ? new Dictionary<string, int>() : _store.Snapshot();
        }

        internal static void ResetMemory()
        {
            OnSlotChanged();
        }

        private static PersistentStateStore RequireStore()
        {
            if (_store == null)
                throw new InvalidOperationException("持久变量存储尚未初始化");
            return _store;
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
        private static void Postfix()
        {
            PersistentModState.OnSlotChanged();
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "SaveGameData")]
    internal static class PersistentStateSavePatch
    {
        private static void Postfix()
        {
            try
            {
                PersistentModState.FlushCurrent();
            }
            catch (Exception ex)
            {
                if (PersistentModState.Log != null)
                    PersistentModState.Log.LogError("MOD 持久变量 sidecar 保存失败：" + ex);
            }
        }
    }
}
