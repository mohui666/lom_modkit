using System;
using System.Collections;
using System.Collections.Generic;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using DataType = MoonSharp.Interpreter.DataType;
using Table = MoonSharp.Interpreter.Table;

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
        private static NamedMove _namedMove;

        internal sealed class NamedMove
        {
            internal StoryCharacterController Character;
            internal float Height;
        }

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
            float height = CarriedHeight(actor);
            Vector3 lift = Vector3.up * height;
            actor.Slot = destination;
            if (origin != null) character.State.holder.position = origin.position + lift;
            if (duration > 0f) actor.Motion = _host.StartCoroutine(MoveWorld(actor, destination, lift, duration));
            else FinishMove(actor, destination, lift);
        }

        private static IEnumerator MoveWorld(Actor actor, RectTransform destination, Vector3 lift, float duration)
        {
            RectTransform holder = actor.Character.State.holder;
            Vector3 start = holder.position;
            float elapsed = 0f;
            while (elapsed < duration && holder != null && destination != null)
            {
                elapsed += Time.deltaTime;
                holder.position = Vector3.Lerp(start, destination.position + lift, Mathf.Clamp01(elapsed / duration));
                yield return null;
            }
            if (holder != null && destination != null) FinishMove(actor, destination, lift);
            actor.Motion = null;
        }

        private static void FinishMove(Actor actor, RectTransform destination, Vector3 lift)
        {
            var state = actor.Character.State;
            state.holder.position = destination.position + lift;
            state.position = lift.y == 0f ? destination : state.holder;
        }

        private static float CarriedHeight(Actor actor)
        {
            var state = actor.Character.State;
            // MoveOffset and absolute layout both mark the holder as the current position.
            return state.holder != null && state.position == state.holder && actor.Slot != null
                ? state.holder.position.y - actor.Slot.position.y : 0f;
        }

        internal static NamedMove BeginNamedMove(Table table)
        {
            NamedMove previous = _namedMove;
            _namedMove = null;
            // Released editors emit this movement-only stage.show call. An explicit
            // show (including one at the same slot) still resets the layout position.
            if (ModOverlay.CurrentPackage == null || table.Get("useDefaultSettings").Type != DataType.Boolean
                || table.Get("useDefaultSettings").Boolean || table.Get("moveDuration").Type != DataType.Number
                || table.Get("fromPosition").Type != DataType.String || table.Get("toPosition").Type != DataType.String
                || !table.Get("portrait").IsNil() || !table.Get("facing").IsNil() || !table.Get("fadeDuration").IsNil()
                || !table.Get("display").IsNil() || !table.Get("shiftIntoPlace").IsNil()) return previous;
            var character = table.Get("character").ToObject<Character>() as StoryCharacterController;
            Actor actor;
            if (character == null || character.State == null || !character.State.onScreen
                || !Actors.TryGetValue(character, out actor)) return previous;
            CharacterShockRuntime.Forget(character.State.holder);
            float height = CarriedHeight(actor);
            if (height != 0f) _namedMove = new NamedMove { Character = character, Height = height };
            return previous;
        }

        internal static void FinishNamedMove(NamedMove previous, bool completed)
        {
            NamedMove current = _namedMove;
            _namedMove = previous;
            if (completed && current != null) current.Character.State.position = current.Character.State.holder;
        }

        internal static void AdjustNamedTween(PortraitOptions options)
        {
            if (_namedMove == null || options.character != _namedMove.Character) return;
            foreach (LTDescr tween in LeanTween.descriptions(options.character.State.holder.gameObject))
                if (tween.type == TweenAction.MOVE) tween.setTo(tween.to + Vector3.up * _namedMove.Height);
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
            if (_namedMove != null && options.character == _namedMove.Character
                && options.fromPosition != options.character.State.holder)
                options.character.State.holder.position += Vector3.up * _namedMove.Height;
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

    [HarmonyPatch(typeof(PortraitController), "Show", new[] { typeof(Table) })]
    internal static class CharacterNamedMovePatch
    {
        private static void Prefix(Table optionsTable, out CharacterLayoutRuntime.NamedMove __state)
        { __state = CharacterLayoutRuntime.BeginNamedMove(optionsTable); }

        private static Exception Finalizer(Exception __exception, CharacterLayoutRuntime.NamedMove __state)
        {
            CharacterLayoutRuntime.FinishNamedMove(__state, __exception == null);
            return __exception;
        }
    }

    [HarmonyPatch(typeof(PortraitController), "DoMoveTween", new[] { typeof(PortraitOptions) })]
    internal static class CharacterNamedMoveTweenPatch
    {
        private static void Postfix(PortraitOptions options) { CharacterLayoutRuntime.AdjustNamedTween(options); }
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
