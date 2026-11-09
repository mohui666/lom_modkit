using System.Collections;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Keep the unwrapped angle: quaternions alone lose both direction and turns.
    internal static class CharacterRotationRuntime
    {
        private sealed class Motion
        {
            internal float Angle;
            internal Coroutine Routine;
        }

        private static MonoBehaviour _host;
        private static readonly Dictionary<Transform, Motion> Motions = new Dictionary<Transform, Motion>();

        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static void Rotate(Transform holder, float initialAngle, float target, float duration, bool worldSpace)
        {
            Motion motion;
            if (!Motions.TryGetValue(holder, out motion))
            {
                motion = new Motion { Angle = initialAngle };
                Motions.Add(holder, motion);
            }
            if (motion.Routine != null) _host.StopCoroutine(motion.Routine);
            motion.Routine = null;
            if (duration > 0f)
                motion.Routine = _host.StartCoroutine(Animate(holder, motion, target, duration, worldSpace));
            else
                Apply(holder, motion, target, worldSpace);
        }

        private static void Apply(Transform holder, Motion motion, float angle, bool worldSpace)
        {
            motion.Angle = angle;
            Quaternion rotation = Quaternion.Euler(0f, 0f, angle);
            if (worldSpace) holder.rotation = rotation;
            else holder.localRotation = rotation;
        }

        private static IEnumerator Animate(Transform holder, Motion motion, float target, float duration, bool worldSpace)
        {
            float start = motion.Angle;
            float elapsed = 0f;
            while (elapsed < duration && holder != null)
            {
                // Preserve native DOTween's default OutQuad and custom actors' linear timing.
                elapsed += worldSpace ? Time.deltaTime : Time.unscaledDeltaTime;
                float progress = Mathf.Clamp01(elapsed / duration);
                if (worldSpace) progress = 1f - (1f - progress) * (1f - progress);
                Apply(holder, motion, start + (target - start) * progress, worldSpace);
                yield return null;
            }
            if (holder != null) Apply(holder, motion, target, worldSpace);
            motion.Routine = null;
        }

        internal static void Forget(Transform holder)
        {
            if (ReferenceEquals(holder, null)) return;
            Motion motion;
            if (!Motions.TryGetValue(holder, out motion)) return;
            if (motion.Routine != null) _host.StopCoroutine(motion.Routine);
            Motions.Remove(holder);
        }

        internal static void Clear()
        {
            foreach (Motion motion in Motions.Values)
                if (motion.Routine != null) _host.StopCoroutine(motion.Routine);
            Motions.Clear();
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "Rotate", new[] { typeof(float), typeof(float) })]
    internal static class CharacterRotatePatch
    {
        private static bool Prefix(StoryCharacterController __instance, float duration, float angle, ref float ____rotationAngle)
        {
            if (ModOverlay.CurrentPackage == null) return true;
            float previous = ____rotationAngle;
            ____rotationAngle = angle; // RestoreRotation still restores the native target.
            if (__instance.State != null && __instance.State.holder != null)
                CharacterRotationRuntime.Rotate(__instance.State.holder, previous, angle, duration, true);
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "ResetRotation")]
    internal static class CharacterResetRotationPatch
    {
        private static void Prefix(StoryCharacterController __instance)
        {
            if (__instance.State != null) CharacterRotationRuntime.Forget(__instance.State.holder);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "RestoreRotation")]
    internal static class CharacterRestoreRotationPatch
    {
        private static void Prefix(StoryCharacterController __instance)
        {
            if (__instance.State != null) CharacterRotationRuntime.Forget(__instance.State.holder);
        }
    }
}
