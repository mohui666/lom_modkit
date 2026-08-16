using System;
using System.Reflection;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    /// <summary>
    /// 把 MOD 战役的手动槽、三类自动槽和 universe 当前槽与原版存档完全隔离。
    /// MOD 自动档写入 Save_mod_&lt;id&gt;_auto*.dat；SaveUniverseData 临时看到的
    /// CurrentSlot 始终是最后一个原版槽，因此标题页的官方继续游戏不会指向 MOD。
    /// </summary>
    internal static class ModSaveIsolation
    {
        private static readonly FieldInfo CurrentSlotField =
            AccessTools.Field(typeof(SaveSystem), "_currentSlot");
        private static readonly MethodInfo AutoSaveDataMethod =
            AccessTools.Method(typeof(SaveSystem), "AutoSaveData", new Type[] { typeof(string) });

        internal static ManualLogSource Log;
        internal static string LastOfficialSlot { get; private set; }
        internal static bool CanRedirectAutoSave { get { return AutoSaveDataMethod != null; } }
        internal static bool CanProtectUniverse { get { return CurrentSlotField != null; } }

        internal static void Reset()
        {
            LastOfficialSlot = "";
        }

        internal static void Initialize(SaveSystem saves)
        {
            Reset();
            if (saves == null) return;
            ObserveSlot(saves.CurrentSlot);
            if (!string.IsNullOrEmpty(LastOfficialSlot)) return;
            try
            {
                UniverseSave universe = saves.GetUniverseSaveData();
                if (universe != null) ObserveSlot(universe.CurrentSlot);
            }
            catch (Exception ex)
            {
                Log?.LogWarning("读取 Universe 最近原版槽失败；MOD 活动期间将拒绝改写 Universe：" + ex.Message);
            }
        }

        internal static bool IsModSlot(string slot)
        {
            return ModSaveSlotPolicy.IsModSlot(slot);
        }

        internal static void ObserveSlot(string slot)
        {
            LastOfficialSlot = ModSaveSlotPolicy.ObserveOfficialSlot(LastOfficialSlot, slot);
        }

        internal static void BeforeEnterModSlot(string currentSlot)
        {
            ObserveSlot(currentSlot);
        }

        internal static bool TrySaveIsolatedAuto(SaveSystem saves, string originalAutoSlot)
        {
            if (!IsActiveModSlot(saves)) return false;
            if (AutoSaveDataMethod == null)
                throw new MissingMethodException("SaveSystem.AutoSaveData(string)");
            string isolated = ModSaveSlotPolicy.IsolatedAutoSlot(
                saves.CurrentSlot, originalAutoSlot);
            AutoSaveDataMethod.Invoke(saves, new object[] { isolated });
            Log?.LogInfo("MOD 自动存档已写入隔离槽 " + isolated);
            return true;
        }

        internal static void RedirectAutoLoad(SaveSystem saves, ref string slot)
        {
            if (!IsActiveModSlot(saves) || string.IsNullOrEmpty(slot)
                || slot.StartsWith("mod_", StringComparison.Ordinal)) return;
            if (!ModSaveSlotPolicy.IsOfficialAutoSlot(slot)) return;
            slot = ModSaveSlotPolicy.IsolatedAutoSlot(saves.CurrentSlot, slot);
        }

        internal static string HideModSlotFromUniverse(SaveSystem saves)
        {
            if (saves == null || CurrentSlotField == null || !IsModSlot(saves.CurrentSlot))
                return null;
            string hidden = saves.CurrentSlot;
            CurrentSlotField.SetValue(saves, LastOfficialSlot ?? "");
            return hidden;
        }

        internal static void RestoreModSlotAfterUniverseSave(SaveSystem saves, string hidden)
        {
            if (saves != null && CurrentSlotField != null && !string.IsNullOrEmpty(hidden))
                CurrentSlotField.SetValue(saves, hidden);
        }

        private static bool IsActiveModSlot(SaveSystem saves)
        {
            return saves != null && ModCampaignState.Active
                && string.Equals(
                    saves.CurrentSlot,
                    "mod_" + ModCampaignState.ActiveModId,
                    StringComparison.Ordinal);
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "AutoSaveStoryData", new Type[] { })]
    internal static class IsolatedStoryAutoSavePatch
    {
        private static bool Prefix(SaveSystem __instance)
        {
            try { return !ModSaveIsolation.TrySaveIsolatedAuto(__instance, "auto"); }
            catch (Exception ex)
            {
                ModSaveIsolation.Log?.LogError("MOD Story 自动存档隔离失败；为保护原版自动档已拒绝写入：" + ex);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "AutoSaveFreeData", new Type[] { })]
    internal static class IsolatedFreeAutoSavePatch
    {
        private static bool Prefix(SaveSystem __instance)
        {
            try { return !ModSaveIsolation.TrySaveIsolatedAuto(__instance, "auto_free"); }
            catch (Exception ex)
            {
                ModSaveIsolation.Log?.LogError("MOD Free 自动存档隔离失败；为保护原版自动档已拒绝写入：" + ex);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "AutoSaveBattleData", new Type[] { })]
    internal static class IsolatedBattleAutoSavePatch
    {
        private static bool Prefix(SaveSystem __instance)
        {
            try { return !ModSaveIsolation.TrySaveIsolatedAuto(__instance, "auto_battle"); }
            catch (Exception ex)
            {
                ModSaveIsolation.Log?.LogError("MOD Battle 自动存档隔离失败；为保护原版自动档已拒绝写入：" + ex);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "AutoLoadGameData", new Type[] { typeof(string) })]
    internal static class IsolatedAutoLoadPatch
    {
        private static void Prefix(SaveSystem __instance, ref string slot)
        {
            ModSaveIsolation.RedirectAutoLoad(__instance, ref slot);
        }
    }

    [HarmonyPatch(typeof(SaveSystem), "SaveUniverseData", new Type[] { })]
    internal static class IsolatedUniverseSavePatch
    {
        private static bool Prefix(SaveSystem __instance, out string __state)
        {
            __state = null;
            if (__instance != null && ModSaveIsolation.IsModSlot(__instance.CurrentSlot)
                && !ModSaveIsolation.CanProtectUniverse)
            {
                ModSaveIsolation.Log?.LogError(
                    "SaveSystem._currentSlot 不可用；为保护原版继续游戏指针，已拒绝本次 Universe 保存");
                return false;
            }
            if (__instance != null && ModSaveIsolation.IsModSlot(__instance.CurrentSlot)
                && string.IsNullOrEmpty(ModSaveIsolation.LastOfficialSlot))
            {
                ModSaveIsolation.Log?.LogWarning(
                    "当前处于 MOD 槽但找不到可确认的原版槽；为保护原版继续游戏指针，已跳过本次 Universe 保存");
                return false;
            }
            __state = ModSaveIsolation.HideModSlotFromUniverse(__instance);
            return true;
        }

        private static void Postfix(SaveSystem __instance, string __state)
        {
            ModSaveIsolation.RestoreModSlotAfterUniverseSave(__instance, __state);
        }

        private static Exception Finalizer(SaveSystem __instance, string __state, Exception __exception)
        {
            ModSaveIsolation.RestoreModSlotAfterUniverseSave(__instance, __state);
            return __exception;
        }
    }
}
