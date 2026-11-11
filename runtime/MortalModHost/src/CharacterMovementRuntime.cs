using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    internal static class CharacterMovementRuntime
    {
        // Show/face leave a LeanTween MOVE on the holder even for a zero-duration
        // command. The following DOTween offset must own that same transform.
        internal static void CancelStageMovement(Transform holder)
        {
            if (holder == null) return;
            foreach (LTDescr tween in LeanTween.descriptions(holder.gameObject))
                if (tween.type == TweenAction.MOVE) LeanTween.cancel(tween.uniqueId);
        }

        internal static void BeforeOffset(StoryCharacterController character)
        {
            if (ModOverlay.CurrentPackage == null || character.State == null) return;
            CancelStageMovement(character.State.holder);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "MoveOffset", new[] { typeof(float), typeof(float), typeof(float) })]
    internal static class CharacterMoveOffsetPatch
    {
        private static void Prefix(StoryCharacterController __instance)
        {
            CharacterMovementRuntime.BeforeOffset(__instance);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "MoveOffsetCoroutine", new[] { typeof(float), typeof(float), typeof(float) })]
    internal static class CharacterMoveOffsetCoroutinePatch
    {
        private static void Prefix(StoryCharacterController __instance)
        {
            CharacterMovementRuntime.BeforeOffset(__instance);
        }
    }
}
