using System;
using System.Collections;
using Fungus;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Uses the same read-only _player2 configuration as the official narcissism
    // talent. Never changes the player's talent level or a ScriptableObject.
    internal static class PlayerAppearance
    {
        private static ModPackage _owner;
        private static string _requested;

        internal static void Clear() { _owner = null; _requested = null; }

        internal static void Set(string value)
        {
            if (value != "game" && value != "original" && value != "beautified")
                throw new ArgumentException("Unknown player appearance: " + value);
            if (ModOverlay.CurrentPackage == null)
                throw new InvalidOperationException("Player appearance requires an active MOD story");
            _owner = ModOverlay.CurrentPackage;
            _requested = value;
        }

        internal static bool Active
        {
            get { return _requested != null && ReferenceEquals(_owner, ModOverlay.CurrentPackage); }
        }

        internal static IEnumerator Load(CharacterPlaceholder placeholder)
        {
            var fields = Traverse.Create(placeholder);
            bool beautified = _requested == "beautified";
            if (_requested == "game")
            {
                var talent = fields.Field("_talentData").GetValue<PlayerTalentData>();
                beautified = talent != null && talent.Level > 0;
            }
            StoryCharacterData data = beautified
                ? fields.Field("_player2").GetValue<StoryCharacterData>()
                : fields.Field("_config").GetValue<StoryCharacterConfig>()?.Get("player");
            if (data == null || data.Id != "player")
                throw new InvalidOperationException("Official player appearance configuration is unavailable");
            StoryCharacterController existing = null;
            try { existing = placeholder.Get("player") as StoryCharacterController; }
            catch (ArgumentException) { }
            if (existing != null && existing.Data == data) yield break;
            if (existing != null)
            {
                // Switching occurs on a show node: hide the old actor before its
                // official unload, then generated stage.Show places the new one.
                foreach (var stage in Resources.FindObjectsOfTypeAll<StoryStageController>())
                {
                    if (stage.CharactersOnStage == null || !stage.CharactersOnStage.Contains(existing)) continue;
                    stage.Hide(new PortraitOptions { character = existing, fadeDuration = 0f,
                        useDefaultSettings = false, waitUntilFinished = false });
                }
                yield return placeholder.UnloadCharacterAsset("player");
            }
            yield return placeholder.AddCharacterGameObject(data);
        }
    }

    [HarmonyPatch(typeof(CharacterPlaceholder), "LoadCharacterAsset")]
    internal static class PlayerAppearanceLoadPatch
    {
        private static void Postfix(CharacterPlaceholder __instance, string key, ref IEnumerator __result)
        {
            if (key == "player" && PlayerAppearance.Active)
                __result = PlayerAppearance.Load(__instance);
        }
    }
}
