using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    // These three confirmed presenters only instantiate visual notifications.
    // StoryMainUI still records the change; the original mutation runs normally.
    [HarmonyPatch(typeof(GameMessagePanel),"DisplayMessage")]
    internal static class QuietFloatingSystemMessagePatch
    {
        private static bool Prefix() { return !SystemMessageScope.Suppressed; }
    }
    [HarmonyPatch(typeof(StatModifyMessagePanel),"DisplayStatModification")]
    internal static class QuietStatSystemMessagePatch
    {
        private static bool Prefix() { return !SystemMessageScope.Suppressed; }
    }
    [HarmonyPatch(typeof(RelationshipModifyMessagePanel),"DisplayRelationshipModification")]
    internal static class QuietRelationshipSystemMessagePatch
    {
        private static bool Prefix() { return !SystemMessageScope.Suppressed; }
    }
}
