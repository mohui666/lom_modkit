using System;
using System.Collections;
using System.Collections.Generic;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Per-stage instance transforms. The native position field becomes the
    // holder after MoveOffsetCoroutine, so keep the actual destination slot.
    internal static class CharacterLayoutRuntime
    {
        private sealed class Actor
        {
            internal StoryCharacterController Character;
            internal RectTransform Slot;
            internal float Scale = 1f;
            internal Coroutine Motion;
        }

        private static MonoBehaviour _host;
        private static readonly Dictionary<Character, Actor> Actors = new Dictionary<Character, Actor>();

        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static void PrepareScale(string id, float percent)
        {
            if (float.IsNaN(percent) || float.IsInfinity(percent) || percent <= 0f)
                throw new ArgumentOutOfRangeException("percent", "人物大小必须为正有限百分比");
            if (ContentRef.IsUserRef(id)) { CustomCharacterRuntime.PrepareShowScale(id, percent); return; }
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            var character = placeholder != null ? placeholder.Get(id) as StoryCharacterController : null;
            if (character == null || character.State == null) throw new InvalidOperationException("人物尚未加载：" + id);
            Get(character).Scale = percent / 100f;
        }

        internal static void FacePreserving(string id, string facing)
        {
            if (facing != "left" && facing != "right") throw new ArgumentException("人物朝向只能是 left/right");
            if (ContentRef.IsUserRef(id))
            {
                if (!CustomCharacterRuntime.Face(id, facing)) throw new InvalidOperationException("人物尚未登场：" + id);
                return;
            }
            StoryCharacterController character = VisibleCharacter(id);
            FacingDirection direction = facing == "left" ? FacingDirection.Left : FacingDirection.Right;
            character.State.facing = direction;
            Vector3 scale = character.State.holder.localScale;
            scale.x = Mathf.Abs(scale.x) * (direction == character.PortraitsFace ? 1f : -1f);
            character.State.holder.localScale = scale;
            MoodControl.RefreshCharacter(character);
        }

        internal static void MovePreserving(string id, string from, string to, float duration)
        {
            if (float.IsNaN(duration) || float.IsInfinity(duration) || duration < 0f)
                throw new ArgumentOutOfRangeException("duration");
            if (ContentRef.IsUserRef(id))
            {
                if (!CustomCharacterRuntime.Move(id, from, to, duration)) throw new InvalidOperationException("人物位移失败：" + id);
                return;
            }
            StoryCharacterController character = VisibleCharacter(id);
            Stage stage = Stage.GetActiveStage();
            RectTransform destination = stage != null ? stage.GetPosition(to) : null;
            RectTransform origin = string.IsNullOrEmpty(from) ? null : stage != null ? stage.GetPosition(from) : null;
            if (destination == null || !string.IsNullOrEmpty(from) && origin == null)
                throw new InvalidOperationException("找不到人物位移站位：" + from + " → " + to);
            Actor actor = Get(character);
            StopMotion(actor);
            CharacterMovementRuntime.CancelStageMovement(character.State.holder);
            actor.Slot = destination;
            if (origin != null) character.State.holder.position = origin.position;
            if (duration > 0f) actor.Motion = _host.StartCoroutine(MoveWorld(actor, destination, duration));
            else { character.State.holder.position = destination.position; character.State.position = destination; }
        }

        private static IEnumerator MoveWorld(Actor actor, RectTransform destination, float duration)
        {
            RectTransform holder = actor.Character.State.holder;
            Vector3 start = holder.position;
            float elapsed = 0f;
            while (elapsed < duration && holder != null && destination != null)
            {
                elapsed += Time.deltaTime;
                holder.position = Vector3.Lerp(start, destination.position, Mathf.Clamp01(elapsed / duration));
                yield return null;
            }
            if (holder != null && destination != null) { holder.position = destination.position; actor.Character.State.position = destination; }
            actor.Motion = null;
        }

        private static Actor Get(StoryCharacterController character)
        {
            Actor actor;
            if (!Actors.TryGetValue(character, out actor))
            {
                actor = new Actor { Character = character };
                if (character.State.position != character.State.holder) actor.Slot = character.State.position;
                Actors.Add(character, actor);
            }
            return actor;
        }

        internal static void BeforeSetup(PortraitOptions options)
        {
            var character = options.character as StoryCharacterController;
            if (ModOverlay.CurrentPackage == null || character == null) return;
            Actor actor = Get(character);
            StopMotion(actor);
            if (options.toPosition != null && options.toPosition != character.State.holder)
                actor.Slot = options.toPosition;
        }

        internal static void AfterSetup(PortraitOptions options)
        {
            Actor actor;
            if (options.character != null && Actors.TryGetValue(options.character, out actor)) ApplyScale(actor);
        }

        internal static void BeforeOffset(StoryCharacterController character)
        {
            if (ModOverlay.CurrentPackage == null) return;
            Actor actor;
            if (Actors.TryGetValue(character, out actor)) StopMotion(actor);
        }

        internal static void Set(string id, float x, float y, float scale, bool absolute, float duration)
        {
            if (float.IsNaN(x) || float.IsInfinity(x) || float.IsNaN(y) || float.IsInfinity(y)
                || float.IsNaN(duration) || float.IsInfinity(duration) || duration < 0f
                || scale != -1f && (float.IsNaN(scale) || float.IsInfinity(scale) || scale <= 0f))
                throw new ArgumentOutOfRangeException("layout", "人物布局需要有限偏移、非负时长和正数缩放");
            if (ContentRef.IsUserRef(id))
            {
                if (!CustomCharacterRuntime.Layout(id, x, y, scale, absolute, duration))
                    throw new InvalidOperationException("布局人物尚未登场：" + id);
                return;
            }
            StoryCharacterController character = VisibleCharacter(id);
            Actor actor = Get(character);
            if (absolute && actor.Slot == null)
                throw new InvalidOperationException("找不到人物的原始站位：" + id);
            StopMotion(actor);
            CharacterMovementRuntime.CancelStageMovement(character.State.holder);
            if (scale != -1f) actor.Scale = scale / 100f;
            ApplyScale(actor);
            RectTransform holder = character.State.holder;
            Vector2 target = (absolute ? actor.Slot.anchoredPosition : holder.anchoredPosition) + new Vector2(x, y);
            if (duration > 0f)
            {
                if (_host == null) throw new InvalidOperationException("人物布局宿主尚未初始化");
                actor.Motion = _host.StartCoroutine(Move(actor, target, duration));
            }
            else
            {
                holder.anchoredPosition = target;
                character.State.position = holder;
            }
        }

        internal static void Reset(string id)
        {
            if (ContentRef.IsUserRef(id))
            {
                if (!CustomCharacterRuntime.ResetLayout(id))
                    throw new InvalidOperationException("布局人物尚未登场：" + id);
                return;
            }
            Actor actor = Get(VisibleCharacter(id));
            StopMotion(actor);
            actor.Scale = 1f;
            ApplyScale(actor);
        }

        private static StoryCharacterController VisibleCharacter(string id)
        {
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            var character = placeholder != null ? placeholder.Get(id) as StoryCharacterController : null;
            if (character == null || character.State.holder == null || !character.State.onScreen)
                throw new InvalidOperationException("布局人物尚未登场：" + id);
            return character;
        }

        private static void ApplyScale(Actor actor)
        {
            if (actor.Character == null || actor.Character.State.holder == null) return;
            RectTransform holder = actor.Character.State.holder;
            holder.localScale = new Vector3(holder.localScale.x < 0f ? -actor.Scale : actor.Scale, actor.Scale, 1f);
        }

        private static IEnumerator Move(Actor actor, Vector2 target, float duration)
        {
            RectTransform holder = actor.Character.State.holder;
            Vector2 start = holder.anchoredPosition;
            float elapsed = 0f;
            while (elapsed < duration && holder != null)
            {
                elapsed += Time.deltaTime;
                holder.anchoredPosition = Vector2.Lerp(start, target, Mathf.Clamp01(elapsed / duration));
                yield return null;
            }
            if (holder != null)
            {
                holder.anchoredPosition = target;
                actor.Character.State.position = holder;
            }
            actor.Motion = null;
        }

        private static void StopMotion(Actor actor)
        {
            if (actor.Motion != null && _host != null) _host.StopCoroutine(actor.Motion);
            actor.Motion = null;
        }

        internal static void ResolveMoodFacing(CharacterMoodPanel panel, ref float scaleX)
        {
            foreach (Actor actor in Actors.Values)
                if (actor.Scale != 1f && actor.Character != null && actor.Character.Mood == panel)
                {
                    scaleX = scaleX < 0f ? -1f : 1f;
                    return;
                }
        }

        internal static void Clear()
        {
            foreach (Actor actor in Actors.Values)
            {
                StopMotion(actor);
                actor.Scale = 1f;
                ApplyScale(actor);
            }
            Actors.Clear();
        }
    }

    [HarmonyPatch(typeof(PortraitController), "SetupPortrait", new[] { typeof(PortraitOptions) })]
    internal static class CharacterLayoutSetupPatch
    {
        private static void Prefix(PortraitOptions options) { CharacterLayoutRuntime.BeforeSetup(options); }
        private static void Postfix(PortraitOptions options) { CharacterLayoutRuntime.AfterSetup(options); }
    }

    [HarmonyPatch(typeof(CharacterMoodPanel), "Show", new[] { typeof(string), typeof(float) })]
    internal static class CharacterLayoutMoodFacingPatch
    {
        private static void Prefix(CharacterMoodPanel __instance, ref float scaleX)
        {
            CharacterLayoutRuntime.ResolveMoodFacing(__instance, ref scaleX);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "MoveOffset", new[] { typeof(float), typeof(float), typeof(float) })]
    internal static class CharacterLayoutNativeOffsetPatch
    {
        private static void Prefix(StoryCharacterController __instance) { CharacterLayoutRuntime.BeforeOffset(__instance); }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "MoveOffsetCoroutine", new[] { typeof(float), typeof(float), typeof(float) })]
    internal static class CharacterLayoutNativeOffsetCoroutinePatch
    {
        private static void Prefix(StoryCharacterController __instance) { CharacterLayoutRuntime.BeforeOffset(__instance); }
    }
}
