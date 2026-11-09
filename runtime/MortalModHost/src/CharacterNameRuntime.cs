using System;
using Fungus;
using Mortal.Story;

namespace MortalModHost
{
    internal static class CharacterNameRuntime
    {
        internal static void Set(string character, string action, string name)
        {
            ModPackage package = ModOverlay.CurrentPackage;
            if (package == null) throw new InvalidOperationException("人物姓名只能在 MOD 演出中修改");
            if (action == "reset") name = null;
            else if (action != "set") throw new ArgumentException("未知姓名操作：" + action);
            PersistentModState.SetCharacterName(package, character, name);

            SayDialog dialog = SayDialog.GetSayDialog();
            if (dialog == null) return;
            if (CharacterTitleRuntime.CustomSpeaker == character) CustomCharacterRuntime.RefreshName(character, dialog);
            else
            {
                var actor = dialog.SpeakingCharacter as StoryCharacterController;
                if (actor != null && actor.Data != null && actor.Data.Id == character)
                    dialog.SetCharacterName(string.IsNullOrEmpty(actor.NameText) ? actor.GetObjectName() : actor.NameText, actor.NameColor);
            }
        }

        internal static void Apply(SayDialog dialog, string character)
        {
            string name;
            if (ModOverlay.CurrentPackage != null && PersistentModState.TryGetCharacterName(ModOverlay.CurrentPackage, character, out name))
                dialog.NameText = name;
        }
    }
}
