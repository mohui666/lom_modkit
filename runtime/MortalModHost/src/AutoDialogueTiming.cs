using System;
using System.Globalization;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    internal enum AutoDialogueSpeed { Slow, Standard, Fast }

    internal sealed class AutoDialogueTiming
    {
        private static readonly Regex RichTags = new Regex(
            @"</?(?:b|i|size|color|material|quad|sprite|font|alpha|u|s|mark|link|align|voffset|space|cspace|mspace|nobr)(?:[=\s][^<>]*)?>",
            RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
        internal readonly int Characters;
        internal float Elapsed { get; private set; }
        private float _voiceTail;
        private bool _hadVoice;
        private bool _sent;

        internal AutoDialogueTiming(string displayedText, bool hadVoice)
        {
            var characters = StringInfo.GetTextElementEnumerator(RichTags.Replace(displayedText ?? "", ""));
            while (characters.MoveNext())
                if (!string.IsNullOrWhiteSpace(characters.GetTextElement())) Characters++;
            _hadVoice = hadVoice;
        }

        internal float Duration(AutoDialogueSpeed speed)
        {
            float multiplier = speed == AutoDialogueSpeed.Slow ? 1.35f : speed == AutoDialogueSpeed.Fast ? 0.75f : 1f;
            return Math.Max(1.2f, Characters / 8f) * multiplier;
        }

        internal bool Tick(float delta, bool active, bool voicePlaying, AutoDialogueSpeed speed)
        {
            if (voicePlaying) { _hadVoice = true; _voiceTail = 0f; }
            if (!active || _sent) return false;
            Elapsed += delta;
            if (_hadVoice && !voicePlaying) _voiceTail += delta;
            if (Elapsed < Duration(speed) || voicePlaying || _hadVoice && _voiceTail < 0.3f) return false;
            _sent = true;
            return true;
        }
    }
}
