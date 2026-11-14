using System;
using System.Globalization;
using Fungus;
using HarmonyLib;
using UnityEngine.UI;

namespace MortalModHost
{
    // SayDialog.DoSay receives the size token emitted by supported compilers.
    // Best-fit otherwise rescales the same explicit size for every line length.
    internal static class DialogueFontRuntime
    {
        private static Text _text;
        private static int _size;
        private static bool _bestFit;

        internal static void Clear()
        {
            if (_text != null)
            {
                _text.fontSize = _size;
                _text.resizeTextForBestFit = _bestFit;
            }
            _text = null;
        }

        internal static void BeforeSay(SayDialog dialog, string line)
        {
            Clear();
            if (ModOverlay.CurrentPackage == null || string.IsNullOrEmpty(line)
                || !line.StartsWith("{size=", StringComparison.Ordinal)) return;
            int end = line.IndexOf('}', 6);
            int size;
            if (end < 0 || !int.TryParse(line.Substring(6, end - 6),
                NumberStyles.None, CultureInfo.InvariantCulture, out size) || size <= 0 || size > 200) return;
            Text text = dialog.StoryTextObject;
            if (text == null) throw new InvalidOperationException("当前对白缺少正文 Text 组件");
            _text = text;
            _size = text.fontSize;
            _bestFit = text.resizeTextForBestFit;
            text.resizeTextForBestFit = false;
            text.fontSize = size;
        }
    }

    [HarmonyPatch(typeof(SayDialog), "DoSay")]
    internal static class DialogueFontPatch
    {
        [HarmonyPriority(Priority.Last)]
        private static void Prefix(SayDialog __instance, string text)
        {
            DialogueFontRuntime.BeforeSay(__instance, text);
        }
    }
}
