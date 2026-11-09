using System;
using Fungus;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    internal static class CharacterTitleRuntime
    {
        internal static string CustomSpeaker;

        internal static void Set(string character, string action, string title)
        {
            ModPackage package = ModOverlay.CurrentPackage;
            if (package == null) throw new InvalidOperationException("人物称号只能在 MOD 演出中修改");
            if (action == "set")
            {
                if (string.IsNullOrWhiteSpace(title)) throw new ArgumentException("新称号不能为空，请使用隐藏称号");
            }
            else if (action == "hide") title = "";
            else if (action == "reset") title = null;
            else throw new ArgumentException("未知称号操作：" + action);
            PersistentModState.SetCharacterTitle(package, character, title);

            SayDialog dialog = SayDialog.GetSayDialog();
            if (dialog == null) return;
            if (CustomSpeaker == character) CustomCharacterRuntime.RefreshTitle(character, dialog);
            else
            {
                var actor = dialog.SpeakingCharacter as StoryCharacterController;
                if (actor != null && actor.Data != null && actor.Data.Id == character)
                    dialog.SetCharacterName(string.IsNullOrEmpty(actor.NameText) ? actor.GetObjectName() : actor.NameText, actor.NameColor);
            }
        }

        internal static void Apply(SayDialog dialog, string character, string original)
        {
            var panel = dialog as CharacterSayDialog;
            if (panel == null) return;
            Text label = Traverse.Create(panel).Field("_titleText").GetValue<Text>();
            if (label == null) throw new InvalidOperationException("人物对白框缺少称号文字控件");
            string custom;
            label.text = ModOverlay.CurrentPackage != null
                && PersistentModState.TryGetCharacterTitle(ModOverlay.CurrentPackage, character, out custom)
                ? custom : original ?? "";
            // Empty text hides the title without leaving the shared nameplate disabled.
            label.gameObject.SetActive(true);
        }
    }

    [HarmonyPatch(typeof(SayDialog), "SetCharacter", new[] { typeof(Character) })]
    internal static class CharacterTitleSpeakerPatch
    {
        private static void Prefix() { CharacterTitleRuntime.CustomSpeaker = null; }
    }

    [HarmonyPatch(typeof(CharacterSayDialog), "SetCharacterName", new[] { typeof(string), typeof(Color) })]
    internal static class CharacterTitleNamePatch
    {
        private static void Postfix(CharacterSayDialog __instance, Text ____titleText)
        {
            if (ModOverlay.CurrentPackage == null || CharacterTitleRuntime.CustomSpeaker != null) return;
            var actor = __instance.SpeakingCharacter as StoryCharacterController;
            if (actor != null && actor.Data != null && ____titleText != null)
            {
                CharacterTitleRuntime.Apply(__instance, actor.Data.Id, ____titleText.text);
                CharacterNameRuntime.Apply(__instance, actor.Data.Id);
            }
        }
    }
}
