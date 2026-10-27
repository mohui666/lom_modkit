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
        private static readonly Dictionary<Character, bool> Overrides = new Dictionary<Character, bool>();

        internal static void Clear() { Overrides.Clear(); }

        internal static void Set(string id, bool dimmed)
        {
            if (string.IsNullOrEmpty(id)) throw new ArgumentException("压暗人物不能为空");
            if (id.StartsWith("user:", StringComparison.Ordinal))
            {
                if (!CustomCharacterRuntime.Dim(id, dimmed))
                    throw new InvalidOperationException("压暗人物尚未登场：" + id);
                return;
            }
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            Stage stage = Stage.GetActiveStage();
            Character character = placeholder != null ? placeholder.Get(id) : null;
            if (stage == null || character == null || character.State == null || character.State.portraitImage == null)
                throw new InvalidOperationException("压暗人物尚未登场：" + id);

            Overrides[character] = dimmed;
            // Replace only RGB tweens; entry/exit alpha and movement must continue.
            LTDescr[] tweens = LeanTween.descriptions(character.State.portraitImage.gameObject);
            foreach (LTDescr tween in tweens)
                if (tween.type == TweenAction.CANVAS_COLOR) LeanTween.cancel(tween.uniqueId);
            character.State.dimmed = !dimmed;
            stage.SetDimmed(character, dimmed);
        }

        internal static bool Resolve(Character character, bool automatic)
        {
            bool value;
            return character != null && Overrides.TryGetValue(character, out value) ? value : automatic;
        }

        internal static void ResolveColor(RectTransform target, ref Color color)
        {
            if (Overrides.Count == 0 || target == null) return;
            foreach (KeyValuePair<Character, bool> entry in Overrides)
            {
                Character character = entry.Key;
                if (character == null || character.State == null || character.State.portraitImage == null
                    || character.State.portraitImage.rectTransform != target) continue;
                Stage stage = Stage.GetActiveStage();
                if (stage == null) return;
                float alpha = color.a;
                color = entry.Value ? stage.DimColor : Color.white;
                color.a = alpha;
                return;
            }
        }
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
