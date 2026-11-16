using System;
using System.Collections.Generic;
using System.Globalization;
using Fungus;
using HarmonyLib;
using UnityEngine.UI;

namespace MortalModHost
{
    // Use the authored size, or the dialog's default size, without fitting it to
    // the line length. Writer's changing markup must never trigger best-fit.
    internal static class DialogueFontRuntime
    {
        private sealed class FontState
        {
            internal int Size;
            internal bool BestFit;
        }

        private static readonly Dictionary<Text, FontState> Original = new Dictionary<Text, FontState>();

        // Host unload is a real end of ownership. Do not use this for another
        // dialog's next line, stage cleanup, or a character fading out.
        internal static void Clear()
        {
            foreach (var pair in Original)
            {
                if (pair.Key == null) continue;
                pair.Key.fontSize = pair.Value.Size;
                pair.Key.resizeTextForBestFit = pair.Value.BestFit;
            }
            Original.Clear();
        }

        private static void Restore(Text text)
        {
            if (ReferenceEquals(text, null)) return;
            FontState state;
            if (!Original.TryGetValue(text, out state)) return;
            Original.Remove(text);
            if (text != null)
            {
                text.fontSize = state.Size;
                text.resizeTextForBestFit = state.BestFit;
            }
        }

        internal static void ClearFor(SayDialog dialog)
        {
            if (dialog != null) Restore(dialog.StoryTextObject);
        }

        internal static void BeforeSay(SayDialog dialog, ref string line)
        {
            Text text = dialog.StoryTextObject;
            // Character, narrative and centered dialogs can remain visible
            // together. Starting a new dialog must not release another one's
            // font while its previous line is still on screen.
            Restore(text);
            if (ModOverlay.CurrentPackage == null) return;
            if (text == null) throw new InvalidOperationException("当前对白缺少正文 Text 组件");
            Original.Add(text, new FontState { Size = text.fontSize, BestFit = text.resizeTextForBestFit });
            text.resizeTextForBestFit = false;
            // Zero has no size token in released Lua. Keep its original dialog
            // size too, rather than silently shrinking default-size long lines.
            if (!string.IsNullOrEmpty(line) && line.StartsWith("{size=", StringComparison.Ordinal))
            {
                int end = line.IndexOf('}', 6);
                int size;
                if (end >= 0 && int.TryParse(line.Substring(6, end - 6),
                    NumberStyles.None, CultureInfo.InvariantCulture, out size) && size > 0 && size <= 200)
                    text.fontSize = size;
            }
        }
    }

    [HarmonyPatch(typeof(SayDialog), "DoSay")]
    internal static class DialogueFontPatch
    {
        [HarmonyPriority(Priority.Last)]
        private static void Prefix(SayDialog __instance, ref string text)
        {
            DialogueFontRuntime.BeforeSay(__instance, ref text);
        }
    }

    // Removing characters does not clear the displayed body. Keep its fixed
    // size until another line replaces it or the owning dialog actually clears.
    [HarmonyPatch(typeof(SayDialog), "Clear")]
    internal static class DialogueFontClearPatch
    {
        private static void Prefix(SayDialog __instance)
        {
            DialogueFontRuntime.ClearFor(__instance);
        }
    }

    [HarmonyPatch(typeof(SayDialog), "OnDestroy")]
    internal static class DialogueFontDestroyPatch
    {
        private static void Prefix(SayDialog __instance)
        {
            DialogueFontRuntime.ClearFor(__instance);
        }
    }
}
