using System;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using Fungus;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using TMPro;
using UnityEngine;
using UnityEngine.TextCore.LowLevel;
using UnityEngine.UI;

namespace MortalModHost
{
    internal static class NameGlyphFallback
    {
        private static readonly (string Family, string File)[] WindowsFaces =
        {
            ("KaiTi", "simkai.ttf"), ("STKaiti", "stkaiti.ttf"),
            ("Microsoft YaHei", "msyh.ttc"), ("Microsoft JhengHei", "msjh.ttc"),
            ("SimSun", "simsun.ttc"), ("SimSun-ExtB", "simsunb.ttf"),
            ("SimSun-ExtG", "simsunextg.ttf"), ("MingLiU-ExtB", "mingliub.ttc"),
            ("Yu Gothic", "YuGothM.ttc"), ("Malgun Gothic", "malgun.ttf"), ("Arial", "arial.ttf")
        };
        private static readonly ConditionalWeakTable<Text, Font> Originals =
            new ConditionalWeakTable<Text, Font>();
        private static readonly ConditionalWeakTable<TMP_Text, object> TmpNameLabels =
            new ConditionalWeakTable<TMP_Text, object>();
        private static readonly object TmpNameMarker = new object();
        private static readonly ConditionalWeakTable<Font, string> TmpSourceFiles =
            new ConditionalWeakTable<Font, string>();
        private static readonly ConditionalWeakTable<Font, TMP_FontAsset> GameFontFaces =
            new ConditionalWeakTable<Font, TMP_FontAsset>();
        private static readonly Lazy<Font[]> GameCalligraphy = new Lazy<Font[]>(() =>
        {
            Font[] loaded = Resources.FindObjectsOfTypeAll<Font>();
            return Array.ConvertAll(new[] { "DFXingShuStd-W5", "Bakudai-Medium" },
                face => Array.Find(loaded, font => font.name == face));
        });
        private static readonly MethodInfo HasCodePoint = AccessTools.Method(typeof(Font),
            nameof(Font.HasCharacter), new[] { typeof(int) });
        private static Font _fallback;
        private static readonly Lazy<TMP_FontAsset>[] TmpFonts = Array.ConvertAll(WindowsFaces,
            face => new Lazy<TMP_FontAsset>(() => CreateTmpFont(face.Family, face.File)));

        internal static void Apply(Text label)
        {
            if (label == null || label.font == null) return;
            Font original = label.font;
            Font stored;
            if (original == _fallback && Originals.TryGetValue(label, out stored))
                original = stored;

            Font selected = original;
            if (!Covers(original, label.text))
            {
                if (_fallback == null)
                {
                    _fallback = Font.CreateDynamicFontFromOSFont(
                        Array.ConvertAll(WindowsFaces, face => face.Family), 24);
                    _fallback.hideFlags = HideFlags.HideAndDontSave;
                }
                if (Covers(_fallback, label.text)) selected = _fallback;
            }
            if (selected == label.font) return;
            Originals.Remove(label);
            if (selected == _fallback) Originals.Add(label, original);
            label.font = selected;
        }

        internal static void Apply(TMP_Text label)
        {
            if (label == null || TmpNameLabels.TryGetValue(label, out _)) return;
            TmpNameLabels.Add(label, TmpNameMarker);
            label.havePropertiesChanged = true;
            label.SetVerticesDirty();
        }

        private static TMP_FontAsset CreateTmpFont(string family, string file)
        {
            string path = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Fonts), file);
            if (!File.Exists(path)) return null;
            var osFont = Font.CreateDynamicFontFromOSFont(family, 24);
            TmpSourceFiles.Add(osFont, path);
            var asset = CreateTmpFont(osFont);
            if (asset == null)
            {
                TmpSourceFiles.Remove(osFont);
                UnityEngine.Object.Destroy(osFont);
                return null;
            }
            osFont.hideFlags = HideFlags.HideAndDontSave;
            return asset;
        }

        private static TMP_FontAsset CreateTmpFont(Font source)
        {
            var asset = TMP_FontAsset.CreateFontAsset(source);
            if (asset == null) return null;
            asset.hideFlags = HideFlags.HideAndDontSave;
            asset.name = "LoM Modkit " + source.name;
            return asset;
        }

        [HarmonyPatch(typeof(TMP_Text), "GetTextElement")]
        private static class TmpNameCharacter
        {
            private static void Postfix(TMP_Text __instance, uint unicode, TMP_FontAsset fontAsset,
                FontStyles fontStyle, FontWeight fontWeight, ref bool isUsingAlternativeTypeface,
                ref TMP_TextElement __result)
            {
                if (__result != null || !TmpNameLabels.TryGetValue(__instance, out _)) return;
                // The static StoryTitle atlases can miss glyphs contained in
                // the installed typefaces. Preserve the nameplate's calligraphy.
                if (fontAsset.faceInfo.familyName?.IndexOf("XingShu", StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    var source = fontAsset.sourceFontFile;
                    if (source != null)
                        __result = Lookup(GameFontFaces.GetValue(source, CreateTmpFont), unicode,
                            fontStyle, fontWeight, ref isUsingAlternativeTypeface);
                    if (__result != null) return;
                    foreach (Font face in GameCalligraphy.Value)
                    {
                        if (face == null || face == source) continue;
                        __result = Lookup(GameFontFaces.GetValue(face, CreateTmpFont), unicode,
                            fontStyle, fontWeight, ref isUsingAlternativeTypeface);
                        if (__result != null) return;
                    }
                }
                foreach (var font in TmpFonts)
                {
                    __result = Lookup(font.Value, unicode, fontStyle, fontWeight,
                        ref isUsingAlternativeTypeface);
                    if (__result != null) return;
                }
            }

            private static TMP_Character Lookup(TMP_FontAsset asset, uint unicode,
                FontStyles style, FontWeight weight, ref bool alternative)
            {
                if (asset == null) return null;
                var character = TMP_FontAssetUtilities.GetCharacterFromFontAsset(
                    unicode, asset, false, style, weight, out alternative);
                // Some game fonts retain cmap entries for stripped glyphs.
                var glyph = character?.glyph;
                return glyph != null && glyph.metrics.width > 0 && glyph.metrics.height > 0
                    ? character : null;
            }
        }

        private static bool Covers(Font font, string text)
        {
            if (font == null) return false;
            if (string.IsNullOrEmpty(text)) return true;
            for (int i = 0; i < text.Length; i++)
            {
                char character = text[i];
                if (char.IsWhiteSpace(character)) continue;
                if (char.IsSurrogatePair(text, i))
                {
                    int codePoint = char.ConvertToUtf32(text, i++);
                    if (HasCodePoint == null || !(bool)HasCodePoint.Invoke(font, new object[] { codePoint }))
                        return false;
                }
                else if (!font.HasCharacter(character)) return false;
            }
            return true;
        }

        // TMP requires an imported Font with font data when taking a Font
        // object. Only our own dynamic OS Font instances use the file overload.
        [HarmonyPatch(typeof(FontEngine), nameof(FontEngine.LoadFontFace), new[] { typeof(Font), typeof(int) })]
        private static class TmpFontFace
        {
            private static bool Prefix(Font __0, int __1, ref FontEngineError __result)
            {
                string path;
                if (__0 == null || !TmpSourceFiles.TryGetValue(__0, out path))
                    return true;
                __result = FontEngine.LoadFontFace(path, __1);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(SayDialog), "set_NameText")]
    internal static class DialogueNameGlyphs
    {
        private static void Postfix(Text ___nameText, GameObject ___nameTextGO)
        {
            if (___nameText != null) NameGlyphFallback.Apply(___nameText);
            else if (___nameTextGO != null)
            {
                NameGlyphFallback.Apply(___nameTextGO.GetComponent<Text>());
                NameGlyphFallback.Apply(___nameTextGO.GetComponent<TMP_Text>());
            }
        }
    }

    [HarmonyPatch(typeof(CombatStatUI), nameof(CombatStatUI.Setup))]
    internal static class DuelNameGlyphs
    {
        [HarmonyPriority(Priority.Last)]
        private static void Postfix(Text ____nameText)
        {
            NameGlyphFallback.Apply(____nameText);
        }
    }

    [HarmonyPatch(typeof(SocialStatPanel), "UpdateInfo")]
    internal static class SocialNameGlyphs
    {
        [HarmonyPriority(Priority.Last)]
        private static void Postfix(Text ____nameText, Text ____titleText)
        {
            NameGlyphFallback.Apply(____nameText);
            NameGlyphFallback.Apply(____titleText);
        }
    }
}
