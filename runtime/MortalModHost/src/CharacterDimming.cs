using System;
using System.Collections.Generic;
using System.Reflection;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Explicit author choices outlive automatic speaker dimming within this stage.
    internal static class CharacterDimming
    {
        private static readonly CharacterDimmingState State = new CharacterDimmingState();
        private static readonly Dictionary<Character, string> Characters = new Dictionary<Character, string>();
        private static Character _speaker;
        private static string _customSpeaker;
        private static HashSet<Character> _highlighted;
        private static Dictionary<Character, bool> _highlightOriginal;
        private static Dictionary<string, bool> _highlightCustomOriginal;

        internal static void Clear() { EndHighlights(); State.Clear(); Characters.Clear(); _speaker = null; _customSpeaker = null; }

        internal static void BeginHighlights(IEnumerable<string> ids)
        {
            EndHighlights();
            Stage stage = Stage.GetActiveStage();
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            if (stage == null || placeholder == null)
                throw new InvalidOperationException("旁白点亮人物需要已载入的剧情舞台");
            var selected = new HashSet<Character>();
            var selectedCustom = new HashSet<string>(StringComparer.Ordinal);
            var custom = CustomCharacterRuntime.CaptureVisibleDimming();
            foreach (string id in ids)
            {
                if (string.IsNullOrWhiteSpace(id)) throw new ArgumentException("旁白点亮人物不能为空");
                if (id.StartsWith("user:", StringComparison.Ordinal))
                {
                    if (!custom.ContainsKey(id)) throw new InvalidOperationException("旁白点亮人物尚未登场：" + id);
                    selectedCustom.Add(id);
                }
                else
                {
                    Character character = placeholder.Get(id);
                    if (character == null || character.State == null || !character.State.onScreen
                        || character.State.portraitImage == null || !stage.CharactersOnStage.Contains(character))
                        throw new InvalidOperationException("旁白点亮人物尚未登场：" + id);
                    selected.Add(character);
                }
            }
            var original = new Dictionary<Character, bool>();
            foreach (Character character in stage.CharactersOnStage)
                if (character != null && character.State != null && character.State.onScreen && character.State.portraitImage != null)
                    original[character] = character.State.dimmed;
            _highlightOriginal = original;
            _highlightCustomOriginal = custom;
            _highlighted = selected;
            foreach (Character character in original.Keys) Apply(character, !selected.Contains(character));
            foreach (string id in custom.Keys) CustomCharacterRuntime.Dim(id, !selectedCustom.Contains(id));
        }

        internal static void EndHighlights()
        {
            var original = _highlightOriginal;
            var custom = _highlightCustomOriginal;
            _highlighted = null;
            _highlightOriginal = null;
            _highlightCustomOriginal = null;
            if (original != null)
                foreach (var entry in original)
                    if (entry.Key != null && entry.Key.State != null && entry.Key.State.portraitImage != null)
                        Apply(entry.Key, entry.Value);
            if (custom != null)
                foreach (var entry in custom) CustomCharacterRuntime.Dim(entry.Key, entry.Value);
        }

        internal static void BeginDialogue(Character speaker)
        {
            _speaker = speaker;
            _customSpeaker = null;
            if (_highlighted == null) { State.BeginDialogue(); RefreshAutomatic(); }
        }

        internal static void BeginCustomDialogue(string id)
        {
            _speaker = null;
            _customSpeaker = id;
            if (_highlighted == null) { State.BeginDialogue(); RefreshAutomatic(); }
        }

        private static void RefreshAutomatic()
        {
            var automatic = new HashSet<string>(State.Automatic, StringComparer.Ordinal);
            foreach (string id in automatic)
                if (id.StartsWith("user:", StringComparison.Ordinal))
                    CustomCharacterRuntime.Dim(id, id != _customSpeaker);
            foreach (var entry in Characters)
                if (entry.Key != null && automatic.Contains(entry.Value) && entry.Key.State != null
                    && entry.Key.State.onScreen && entry.Key.State.portraitImage != null)
                    Apply(entry.Key, entry.Key != _speaker);
        }

        internal static void Set(string id, bool dimmed)
        { SetMode(id, dimmed, "manual"); }

        internal static void SetMode(string id, bool dimmed, string mode)
        {
            if (mode != "manual" && mode != "until_dialogue" && mode != "auto")
                throw new ArgumentException("未知压暗生效方式：" + mode);
            if (string.IsNullOrEmpty(id)) throw new ArgumentException("压暗人物不能为空");
            if (id.StartsWith("user:", StringComparison.Ordinal))
            {
                if (!CustomCharacterRuntime.Dim(id, mode == "auto" ? id != _customSpeaker : dimmed))
                    throw new InvalidOperationException("压暗人物尚未登场：" + id);
                State.Set(id, dimmed, mode);
                return;
            }
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            Stage stage = Stage.GetActiveStage();
            Character character = placeholder != null ? placeholder.Get(id) : null;
            if (stage == null || character == null || character.State == null || character.State.portraitImage == null)
                throw new InvalidOperationException("压暗人物尚未登场：" + id);

            Characters[character] = id;
            State.Set(id, dimmed, mode);
            Apply(character, mode == "auto" ? character != _speaker : dimmed);
        }

        private static void Apply(Character character, bool dimmed)
        {
            Stage stage = Stage.GetActiveStage();
            if (stage == null) return;
            // Replace only RGB tweens; entry/exit alpha and movement must continue.
            LTDescr[] tweens = LeanTween.descriptions(character.State.portraitImage.gameObject);
            foreach (LTDescr tween in tweens)
                if (tween.type == TweenAction.CANVAS_COLOR) LeanTween.cancel(tween.uniqueId);
            character.State.dimmed = !dimmed;
            stage.SetDimmed(character, dimmed);
        }

        internal static bool Resolve(Character character, bool automatic)
        {
            if (_highlighted != null && character != null) return !_highlighted.Contains(character);
            bool value;
            string id;
            return character != null && Characters.TryGetValue(character, out id) && State.TryGet(id, out value) ? value : automatic;
        }

        internal static void ResolveColor(RectTransform target, ref Color color)
        {
            if (target == null) return;
            if (_highlightOriginal != null)
                foreach (Character character in _highlightOriginal.Keys)
                    if (character != null && character.State != null && character.State.portraitImage != null
                        && character.State.portraitImage.rectTransform == target)
                    {
                        Stage stage = Stage.GetActiveStage();
                        if (stage == null) return;
                        float alpha = color.a;
                        color = _highlighted.Contains(character) ? Color.white : stage.DimColor;
                        color.a = alpha;
                        return;
                    }
            foreach (KeyValuePair<Character, string> entry in Characters)
            {
                Character character = entry.Key;
                if (character == null || character.State == null || character.State.portraitImage == null
                    || character.State.portraitImage.rectTransform != target) continue;
                bool dimmed;
                if (!State.TryGet(entry.Value, out dimmed)) continue;
                Stage stage = Stage.GetActiveStage();
                if (stage == null) return;
                float alpha = color.a;
                color = dimmed ? stage.DimColor : Color.white;
                color.a = alpha;
                return;
            }
        }
    }

    [HarmonyPatch(typeof(SayDialog), "SetCharacter")]
    internal static class CharacterDimDialoguePatch
    {
        private static void Prefix(Character character) { CharacterDimming.BeginDialogue(character); }
    }

    [HarmonyPatch]
    internal static class CharacterDimStatePatch
    {
        private static IEnumerable<MethodBase> TargetMethods()
        {
            yield return AccessTools.Method(typeof(PortraitController), "SetDimmed");
            yield return AccessTools.Method(typeof(StoryStageController), "SetDimmed");
        }

        private static void Prefix(Character character, ref bool dimmedState)
        {
            dimmedState = CharacterDimming.Resolve(character, dimmedState);
        }
    }

    // Show/ShowPortrait also create white RGB tweens independently of SetDimmed.
    [HarmonyPatch(typeof(LeanTween), "color", new Type[] { typeof(RectTransform), typeof(Color), typeof(float) })]
    internal static class CharacterDimColorPatch
    {
        private static void Prefix(RectTransform rectTrans, ref Color to)
        {
            CharacterDimming.ResolveColor(rectTrans, ref to);
        }
    }
}
