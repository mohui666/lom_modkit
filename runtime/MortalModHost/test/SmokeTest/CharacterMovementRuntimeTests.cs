using System;
using System.Collections.Generic;
using System.Reflection;
using UnityEngine;

namespace MortalModHost
{
    internal static class CharacterMovementRuntimeTests
    {
        internal static void Run()
        {
            foreach (Type patch in new[] { typeof(CharacterMoveOffsetPatch), typeof(CharacterMoveOffsetCoroutinePatch) })
            {
                LeanTween.Clear();
                var actor = new Mortal.Story.StoryCharacterController();
                var other = new Mortal.Story.StoryCharacterController();
                var image = new GameObject();
                // Model the pending zero-time movement left by stage.show/face.
                // Without cancellation its next update overwrites an immediate offset.
                LeanTween.Add(actor.State.holder.gameObject, TweenAction.MOVE, 0f);
                actor.State.holder.gameObject.Position = 50f;
                LeanTween.Tick();
                if (actor.State.holder.gameObject.Position != 0f) throw new Exception("movement race was not reproduced");

                LeanTween.Clear();
                LeanTween.Add(actor.State.holder.gameObject, TweenAction.MOVE, 0f);
                LeanTween.Add(actor.State.holder.gameObject, TweenAction.ROTATE, 12f);
                LeanTween.Add(image, TweenAction.CANVAS_ALPHA, 0.5f);
                LeanTween.Add(other.State.holder.gameObject, TweenAction.MOVE, 90f);
                var prefix = patch.GetMethod("Prefix", BindingFlags.NonPublic | BindingFlags.Static);
                ModOverlay.CurrentPackage = null;
                prefix.Invoke(null, new object[] { actor });
                if (LeanTween.Count != 4) throw new Exception("vanilla movement was intercepted");
                ModOverlay.CurrentPackage = new object();
                prefix.Invoke(null, new object[] { actor });
                actor.State.holder.gameObject.Position = 50f;
                LeanTween.Tick();
                if (actor.State.holder.gameObject.Position != 50f) throw new Exception("pending stage tween overwrote offset");
                if (actor.State.holder.gameObject.Rotation != 12f || image.Alpha != 0.5f
                    || other.State.holder.gameObject.Position != 90f)
                    throw new Exception("offset cancelled an unrelated animation");

                actor.State.holder = null;
                prefix.Invoke(null, new object[] { actor });
            }
            LeanTween.Clear();
            ModOverlay.CurrentPackage = null;
            Console.WriteLine("PASS offset cancels pending stage movement, preserves other actors/alpha/rotation and vanilla scope (offline)");
        }
    }
}

namespace UnityEngine
{
    internal class GameObject { internal float Position, Rotation, Alpha; }
}

internal enum TweenAction { MOVE, ROTATE, CANVAS_ALPHA }
internal sealed class LTDescr
{
    internal int uniqueId;
    internal TweenAction type;
    internal GameObject Target;
    internal float Value;
}
internal static class LeanTween
{
    private static readonly List<LTDescr> active = new List<LTDescr>();
    private static int nextId;
    internal static int Count { get { return active.Count; } }
    internal static void Add(GameObject target, TweenAction type, float value)
    {
        active.Add(new LTDescr { uniqueId = ++nextId, Target = target, type = type, Value = value });
    }
    internal static LTDescr[] descriptions(GameObject target) { return active.FindAll(t => t.Target == target).ToArray(); }
    internal static void cancel(int id) { active.RemoveAll(t => t.uniqueId == id); }
    internal static void Clear() { active.Clear(); }
    internal static void Tick()
    {
        foreach (LTDescr tween in active)
            switch (tween.type)
            {
                case TweenAction.MOVE: tween.Target.Position = tween.Value; break;
                case TweenAction.ROTATE: tween.Target.Rotation = tween.Value; break;
                case TweenAction.CANVAS_ALPHA: tween.Target.Alpha = tween.Value; break;
            }
        active.Clear();
    }
}
