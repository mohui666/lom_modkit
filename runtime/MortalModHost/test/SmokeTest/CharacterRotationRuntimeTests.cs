using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using UnityEngine;

namespace MortalModHost
{
    // Runs the production coroutine against a small scheduler, without loading Unity.
    internal static class CharacterRotationRuntimeTests
    {
        private static void Near(float actual, float expected)
        {
            if (Math.Abs(actual - expected) > 0.01f)
                throw new Exception("rotation expected " + expected + ", got " + actual);
        }

        private static float Angle(Transform holder, bool world) { return (world ? holder.rotation : holder.localRotation).Z; }
        private static float Delta(float from, float to) { return (to - from + 540f) % 360f - 180f; }

        internal static void Run()
        {
            var host = new MonoBehaviour();
            CharacterRotationRuntime.Init(host);
            foreach (bool world in new[] { false, true })
            foreach (float target in new[] { 355f, 360f, -355f, -360f, 720f, -720f })
            {
                var holder = new Transform();
                CharacterRotationRuntime.Rotate(holder, 0f, target, 1f, world);
                float last = 0f, travelled = 0f;
                for (int frame = 0; frame < 12; frame++)
                {
                    float angle = Angle(holder, world);
                    float delta = Delta(last, angle);
                    if (delta * target < -0.001f) throw new Exception("rotation reversed direction");
                    travelled += delta;
                    last = angle;
                    host.Tick();
                }
                Near(travelled, target);
                CharacterRotationRuntime.Clear();
            }

            var actor = new Transform();
            CharacterRotationRuntime.Rotate(actor, 0f, 355f, 0f, false);
            CharacterRotationRuntime.Rotate(actor, 0f, 360f, 1f, false);
            Near(actor.localRotation.Z, 355.625f); // no reverse lap after a completed 355°
            for (int i = 0; i < 12; i++) host.Tick();
            Near(actor.localRotation.Z, 0f);
            CharacterRotationRuntime.Rotate(actor, 0f, 360f, 1f, false);
            Near(actor.localRotation.Z, 0f); // same absolute target adds no turn
            for (int i = 0; i < 12; i++) host.Tick();
            CharacterRotationRuntime.Rotate(actor, 0f, 720f, 1f, false);
            Near(actor.localRotation.Z, 45f); // 360→720 is one more lap, not two
            CharacterRotationRuntime.Clear();

            actor = new Transform();
            CharacterRotationRuntime.Rotate(actor, 0f, 360f, 1f, false);
            host.Tick(); // currently 90°, not the old requested target of 360°
            Near(actor.localRotation.Z, 90f);
            CharacterRotationRuntime.Rotate(actor, 0f, -360f, 1f, false);
            Near(actor.localRotation.Z, 33.75f);
            CharacterRotationRuntime.Forget(actor);
            for (int i = 0; i < 12; i++) host.Tick();
            Near(actor.localRotation.Z, 33.75f);
            CharacterRotationRuntime.Rotate(actor, 0f, 360f, 1f, false);
            Near(actor.localRotation.Z, 45f); // reset starts from the supplied native angle
            CharacterRotationRuntime.Clear();
            host.Tick();
            Near(actor.localRotation.Z, 45f);

            // Verify the wrapper retains the original target field and stays scoped to MODs.
            var native = new Mortal.Story.StoryCharacterController();
            var prefix = typeof(CharacterRotatePatch).GetMethod("Prefix", BindingFlags.NonPublic | BindingFlags.Static);
            object[] args = { native, 1f, 360f, 0f };
            ModOverlay.CurrentPackage = null;
            if (!(bool)prefix.Invoke(null, args)) throw new Exception("vanilla Rotate was intercepted");
            Near((float)args[3], 0f);
            ModOverlay.CurrentPackage = new object();
            if ((bool)prefix.Invoke(null, args)) throw new Exception("MOD Rotate used quaternion tween");
            Near((float)args[3], 360f);
            Near(native.State.holder.rotation.Z, 84.375f);
            foreach (Type patch in new[] { typeof(CharacterResetRotationPatch), typeof(CharacterRestoreRotationPatch) })
            {
                patch.GetMethod("Prefix", BindingFlags.NonPublic | BindingFlags.Static).Invoke(null, new object[] { native });
                float stopped = native.State.holder.rotation.Z;
                for (int i = 0; i < 12; i++) host.Tick();
                Near(native.State.holder.rotation.Z, stopped);
                CharacterRotationRuntime.Rotate(native.State.holder, 0f, 360f, 1f, true);
            }
            CharacterRotationRuntime.Clear();
            ModOverlay.CurrentPackage = null;
            Console.WriteLine("PASS signed multi-turn rotation, retargeting, reset/restore and MOD scope (offline scheduler)");
        }
    }

    internal static class ModOverlay { internal static object CurrentPackage; }
}

// Test doubles normalize quaternions, so only the production runtime's retained
// scalar state can distinguish 0°, 360° and 720°. No native rendering is simulated.
namespace UnityEngine
{
    internal struct Quaternion
    {
        internal float Z;
        internal static Quaternion Euler(float x, float y, float z) { return new Quaternion { Z = (z % 360f + 360f) % 360f }; }
    }
    internal class Transform
    {
        internal Quaternion rotation, localRotation;
        internal readonly GameObject gameObject = new GameObject();
    }
    internal class Coroutine { internal IEnumerator Steps; }
    internal class MonoBehaviour
    {
        private readonly List<Coroutine> active = new List<Coroutine>();
        internal Coroutine StartCoroutine(IEnumerator steps)
        {
            var routine = new Coroutine { Steps = steps };
            if (steps.MoveNext()) active.Add(routine);
            return routine;
        }
        internal void StopCoroutine(Coroutine routine) { active.Remove(routine); }
        internal void Tick()
        {
            foreach (var routine in active.ToArray())
                if (!routine.Steps.MoveNext()) active.Remove(routine);
        }
    }
    internal static class Time { internal static float deltaTime = 0.125f, unscaledDeltaTime = 0.125f; }
    internal static class Mathf { internal static float Clamp01(float value) { return Math.Clamp(value, 0f, 1f); } }
}
namespace Mortal.Story
{
    internal class StoryCharacterController { internal PortraitState State = new PortraitState(); }
    internal class PortraitState { internal UnityEngine.Transform holder = new UnityEngine.Transform(); }
}
namespace HarmonyLib
{
    [AttributeUsage(AttributeTargets.Class)]
    internal sealed class HarmonyPatch : Attribute
    {
        internal HarmonyPatch(Type type, string method, params Type[] args) { }
    }
}
