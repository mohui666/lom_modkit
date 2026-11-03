using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    // SaveSystem holds a serialized reference to the official database. Redirect its
    // operations to the session clone without replacing that serialized asset field.
    [HarmonyPatch(typeof(ItemDatabase), "Save")]
    internal static class CustomGameplayItemSavePatch
    {
        private static bool Prefix(ItemDatabase __instance, GameSave gameSave)
        {
            if (!CustomGameplayRuntime.Redirect(__instance)) return true;
            CustomGameplayRuntime.Database.Save(gameSave);
            return false;
        }
    }
    [HarmonyPatch(typeof(ItemDatabase), "Load")]
    internal static class CustomGameplayItemLoadPatch
    {
        private static bool Prefix(ItemDatabase __instance, GameSave gameSave)
        {
            if (!CustomGameplayRuntime.Redirect(__instance)) return true;
            CustomGameplayRuntime.Database.Load(gameSave);
            return false;
        }
    }
    [HarmonyPatch(typeof(ItemDatabase), "Reset")]
    internal static class CustomGameplayItemResetPatch
    {
        private static bool Prefix(ItemDatabase __instance)
        {
            if (!CustomGameplayRuntime.Redirect(__instance)) return true;
            CustomGameplayRuntime.Database.Reset();
            return false;
        }
    }
    [HarmonyPatch(typeof(ItemDatabase), "GetItem")]
    internal static class CustomGameplayItemLookupPatch
    {
        private static bool Prefix(ItemDatabase __instance, GameItemType type, string id, ref IGameItem __result)
        {
            if (!CustomGameplayRuntime.Redirect(__instance)) return true;
            __result = CustomGameplayRuntime.Database.GetItem(type, id);
            return false;
        }
    }
    [HarmonyPatch]
    internal static class CustomGameplayItemCollectionsPatch
    {
        private static IEnumerable<MethodBase> TargetMethods()
        {
            foreach (string property in new[] { "Books", "Miscs", "Special", "Consumables" })
                yield return AccessTools.PropertyGetter(typeof(ItemDatabase), property);
        }
        private static bool Prefix(ItemDatabase __instance, MethodBase __originalMethod, ref ItemDataCollection __result)
        {
            if (!CustomGameplayRuntime.Redirect(__instance)) return true;
            __result = (ItemDataCollection)((MethodInfo)__originalMethod).Invoke(CustomGameplayRuntime.Database, null);
            return false;
        }
    }

    // MartialStatReference and BookLevelStatConvertData retain serialized references
    // to official books. Read their current state from the session copy as well.
    [HarmonyPatch(typeof(ItemData), "get_CurrentCount")]
    internal static class CustomGameplayReferencedCountPatch
    {
        private static bool Prefix(ItemData __instance, ref int __result)
        {
            ItemData item = CustomGameplayRuntime.SessionItem(__instance);
            if (item == null) return true;
            __result = item.CurrentCount;
            return false;
        }
    }
    [HarmonyPatch(typeof(Book), "get_CurrentMartialLevel")]
    internal static class CustomGameplayReferencedLevelPatch
    {
        private static bool Prefix(Book __instance, ref int __result)
        {
            Book book = CustomGameplayRuntime.SessionItem(__instance) as Book;
            if (book == null) return true;
            __result = book.CurrentMartialLevel;
            return false;
        }
    }
    [HarmonyPatch(typeof(Book), "get_IsMaxLevel")]
    internal static class CustomGameplayReferencedMaxLevelPatch
    {
        private static bool Prefix(Book __instance, ref bool __result)
        {
            Book book = CustomGameplayRuntime.SessionItem(__instance) as Book;
            if (book == null) return true;
            __result = book.IsMaxLevel;
            return false;
        }
    }
}
