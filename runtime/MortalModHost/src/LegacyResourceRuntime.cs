using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using Fungus;
using HarmonyLib;
using Lean.Localization;
using Mortal.Core;
using Mortal.Story;
using Mortal.Combat;
using OBB.Framework.Extensions;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    internal static class LegacyResourceRuntime
    {
        private static readonly Dictionary<string, Sprite> Sprites = new Dictionary<string, Sprite>(StringComparer.OrdinalIgnoreCase);
        private static readonly List<Texture2D> Textures = new List<Texture2D>();
        private static readonly List<GameObject> Animations = new List<GameObject>();
        private static readonly Dictionary<StoryCharacterController, HashSet<Sprite>> Actors = new Dictionary<StoryCharacterController, HashSet<Sprite>>();
        private static readonly Dictionary<Image, Sprite> UiOriginals = new Dictionary<Image, Sprite>();
        private static ModPackage _owner;
        private static bool _clearing;
        private static bool _explicitVoicePending;
        private static ModPackage CurrentPackage
        {
            get
            {
                return ModOverlay.CurrentPackage ?? LegacyOriginalContext.ActivePackage;
            }
        }

        internal static Sprite ResolveSprite(ModPackage package, string legacyPath)
        {
            if (package == null || package.LegacyResources == null || legacyPath == null) return null;
            string path = legacyPath.Replace('\\', '/');
            while (path.StartsWith("./", StringComparison.Ordinal)) path = path.Substring(2);
            LegacySprite config;
            if (!package.LegacyResources.Sprites.TryGetValue(path, out config)) return null;
            if (!ReferenceEquals(_owner, package)) { Clear(); _owner = package; }
            Sprite sprite;
            if (Sprites.TryGetValue(path, out sprite)) return sprite;
            var pixels = new List<Color32[]>();
            Texture2D texture = null;
            foreach (LegacySpriteFrame frame in config.Frames)
            {
                UserContent content;
                if (!package.TryGetUserContent(frame.Image.Substring(5), out content) || content.Type != "image")
                    throw new InvalidOperationException("旧立绘缺少包内图像：" + frame.Image);
                var decoded = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                if (!decoded.LoadImage(content.Bytes))
                {
                    UnityEngine.Object.Destroy(decoded);
                    throw new InvalidOperationException("旧立绘图像解码失败：" + frame.Image);
                }
                if (texture != null && (texture.width != decoded.width || texture.height != decoded.height))
                {
                    UnityEngine.Object.Destroy(decoded);
                    throw new InvalidOperationException("旧 GIF 帧画布尺寸不一致：" + legacyPath);
                }
                pixels.Add(decoded.GetPixels32());
                if (texture == null) texture = decoded;
                else UnityEngine.Object.Destroy(decoded);
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            Textures.Add(texture);
            sprite = Sprite.Create(texture, new Rect(0, 0, texture.width, texture.height),
                new Vector2(config.PivotX, config.PivotY), config.PixelsPerUnit, config.Extrude, (SpriteMeshType)config.SpriteType);
            sprite.name = "__lomlegacy__" + package.Id + "__" + path;
            Sprites.Add(path, sprite);
            if (config.Frames.Count > 1)
            {
                var animation = new GameObject("MOD GIF " + path);
                UnityEngine.Object.DontDestroyOnLoad(animation);
                animation.AddComponent<LegacySpriteAnimation>().Initialize(texture, pixels, config.Frames);
                Animations.Add(animation);
            }
            return sprite;
        }

        internal static bool TryPortrait(StoryCharacterController character, string key, out Sprite sprite)
        {
            sprite = null;
            ModPackage package = CurrentPackage;
            if (_clearing || package == null || package.LegacyResources == null || character == null || character.Data == null) return false;
            foreach (var entry in package.LegacyResources.Sprites)
            {
                if (!entry.Key.StartsWith("Portraits/", StringComparison.OrdinalIgnoreCase)) continue;
                string[] parts = Path.GetFileNameWithoutExtension(entry.Key).Split('_');
                // HookCharacter only links names with exactly character_expression.
                if (parts.Length != 2 || parts[0] != character.Data.Id || parts[1].ToLowerInvariant() != key) continue;
                sprite = ResolveSprite(package, entry.Key);
                HashSet<Sprite> owned;
                if (!Actors.TryGetValue(character, out owned)) { owned = new HashSet<Sprite>(); Actors.Add(character, owned); }
                owned.Add(sprite);
                if (!character.Portraits.Contains(sprite)) character.Portraits.Add(sprite);
                Sprite selected = sprite;
                if (character.State.holder != null && !character.State.allPortraits.Exists(image => image != null && image.sprite == selected))
                    AccessTools.Method(typeof(StoryCharacterController), "AddPortraitObject", new[] { typeof(Sprite) }).Invoke(character, new object[] { sprite });
                return true;
            }
            return false;
        }

        internal static bool TryString(string key, out string text)
        {
            var package = CurrentPackage;
            text = null;
            return package != null && package.LegacyResources != null && key != null && package.LegacyResources.Strings.TryGetValue(key, out text);
        }

        internal static void ApplyAvatar(Image image, string character)
        {
            ModPackage package = CurrentPackage;
            if (image == null || package == null || package.LegacyResources == null) return;
            foreach (var entry in package.LegacyResources.Sprites)
            {
                if (!entry.Key.StartsWith("Portraits/", StringComparison.OrdinalIgnoreCase)) continue;
                string[] parts = Path.GetFileNameWithoutExtension(entry.Key).Split('_');
                if (parts.Length != 2 || parts[0] != character || parts[1].ToLowerInvariant() != "normal") continue;
                Sprite sprite = ResolveSprite(package, entry.Key);
                if (!UiOriginals.ContainsKey(image)) UiOriginals.Add(image, image.sprite);
                image.sprite = sprite;
                return;
            }
        }

        internal static void VoiceKey(string key, ref string text)
        {
            var package = CurrentPackage;
            if (package != null && package.LegacyResources != null && package.LegacyResources.Voices.ContainsKey(key)
                && !(text ?? "").StartsWith("ac<", StringComparison.Ordinal)) text = "ac<" + key + ">" + text;
        }

        internal static void BeforeSay(ref string text)
        {
            var package = CurrentPackage;
            if (package == null || package.LegacyResources == null || package.LegacyResources.Voices.Count == 0) return;
            if (_explicitVoicePending && !(text ?? "").StartsWith("ac<", StringComparison.Ordinal))
            {
                _explicitVoicePending = false;
                return;
            }
            CustomAudioPlayer.StopVoice();
            if (text == null || !text.StartsWith("ac<", StringComparison.Ordinal)) return;
            int end = text.IndexOf('>', 3);
            if (end < 0) return;
            string reference;
            if (!package.LegacyResources.Voices.TryGetValue(text.Substring(3, end - 3), out reference)) return;
            if (!CustomAudioPlayer.PlayVoice(reference)) throw new InvalidOperationException("旧配音无法播放：" + reference);
            _explicitVoicePending = false;
            text = text.Substring(end + 1);
        }

        internal static void VoiceStarted(bool success)
        { _explicitVoicePending = success && CurrentPackage != null && CurrentPackage.LegacyResources != null; }
        internal static void VoiceStopped() { _explicitVoicePending = false; }

        internal static void Clear()
        {
            _clearing = true;
            try
            {
                foreach (var original in UiOriginals) if (original.Key != null) original.Key.sprite = original.Value;
                UiOriginals.Clear();
                ClearActors();
                foreach (var animation in Animations) if (animation != null) UnityEngine.Object.Destroy(animation);
                foreach (var sprite in Sprites.Values) if (sprite != null) UnityEngine.Object.Destroy(sprite);
                foreach (var texture in Textures) if (texture != null) UnityEngine.Object.Destroy(texture);
                Actors.Clear(); Animations.Clear(); Sprites.Clear(); Textures.Clear(); _owner = null; _explicitVoicePending = false;
            }
            finally { _clearing = false; }
        }
        internal static void ClearActors()
        {
            _clearing = true;
            try
            {
                foreach (var actor in Actors)
                {
                    StoryCharacterController character = actor.Key;
                    if (character == null) continue;
                    if (character.Data != null && character.Data.PortraitResourceList != null && character.Data.PortraitResourceList.Length > 0)
                        character.LoadPortraitSync(character.Data.PortraitResourceList[0].Mapping.Value);
                    if (character.State.portraitImage != null && actor.Value.Contains(character.State.portraitImage.sprite))
                        character.State.portraitImage = character.State.allPortraits.Find(image => image != null && !actor.Value.Contains(image.sprite));
                    for (int i = character.State.allPortraits.Count - 1; i >= 0; --i)
                    {
                        Image image = character.State.allPortraits[i];
                        if (image == null || !actor.Value.Contains(image.sprite)) continue;
                        character.State.allPortraits.RemoveAt(i);
                        image.gameObject.SetActive(false);
                        UnityEngine.Object.Destroy(image.gameObject);
                    }
                    foreach (Sprite sprite in actor.Value) character.Portraits.Remove(sprite);
                }
                Actors.Clear();
            }
            finally { _clearing = false; }
        }
        internal static IEnumerator Loaded() { yield break; }
    }

    // Update the texture behind the same Sprite, preserving Fungus' identity lookups.
    internal sealed class LegacySpriteAnimation : MonoBehaviour
    {
        private Texture2D _texture;
        private List<Color32[]> _pixels;
        private List<LegacySpriteFrame> _frames;
        private int _index;
        private float _elapsed;
        internal void Initialize(Texture2D texture, List<Color32[]> pixels, List<LegacySpriteFrame> frames)
        { _texture = texture; _pixels = pixels; _frames = frames; }
        private void Update()
        {
            if (_texture == null || _frames == null) return;
            _elapsed += Time.deltaTime;
            if (_elapsed < _frames[_index].Duration) return;
            // ModSupport advances once per Unity update, including zero-delay frames.
            _elapsed = 0;
            _index = (_index + 1) % _frames.Count;
            _texture.SetPixels32(_pixels[_index]);
            _texture.Apply(false, false);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "LoadPortraitSync", new[] { typeof(string) })]
    internal static class LegacyPortraitSyncPatch
    {
        private static bool Prefix(StoryCharacterController __instance, string key)
        { Sprite sprite; return !LegacyResourceRuntime.TryPortrait(__instance, key, out sprite); }
    }
    [HarmonyPatch(typeof(StoryCharacterController), "LoadPortrait", new[] { typeof(string) })]
    internal static class LegacyPortraitLoadPatch
    {
        private static bool Prefix(StoryCharacterController __instance, string key, ref IEnumerator __result)
        {
            Sprite sprite;
            if (!LegacyResourceRuntime.TryPortrait(__instance, key, out sprite)) return true;
            __result = LegacyResourceRuntime.Loaded(); return false;
        }
    }
    [HarmonyPatch(typeof(StoryCharacterController), "GetPortraitName", new[] { typeof(string) })]
    internal static class LegacyPortraitNamePatch
    {
        private static bool Prefix(StoryCharacterController __instance, string name, ref string __result)
        {
            Sprite sprite;
            if (!LegacyResourceRuntime.TryPortrait(__instance, name, out sprite)) return true;
            AccessTools.Field(typeof(StoryCharacterController), "_originPortrait").SetValue(__instance, name);
            __result = sprite.name; return false;
        }
    }
    [HarmonyPatch(typeof(LeanLocalization), "GetTranslationText", new[] { typeof(string), typeof(string), typeof(bool) })]
    internal static class LegacyLocalizationPatch
    {
        private static bool Prefix(string name, bool replaceTokens, ref string __result)
        {
            string text;
            if (!LegacyResourceRuntime.TryString(name, out text)) return true;
            __result = replaceTokens ? LeanTranslation.FormatText(text) : text;
            return false;
        }
    }
    [HarmonyPatch(typeof(LuaManager), "GetStoryText")]
    internal static class LegacyVoiceTextPatch
    { private static void Postfix(string key, ref string __result) { LegacyResourceRuntime.VoiceKey(key, ref __result); } }
    [HarmonyPatch(typeof(SayDialog), "DoSay")]
    internal static class LegacySayVoicePatch
    { private static void Prefix(ref string text) { LegacyResourceRuntime.BeforeSay(ref text); } }
    [HarmonyPatch(typeof(ModOverlay), "Clear")]
    internal static class LegacyStageResourceClearPatch
    { private static void Prefix() { LegacyResourceRuntime.ClearActors(); } }
    [HarmonyPatch(typeof(CustomGameplayRuntime), "Clear")]
    internal static class LegacyResourceClearPatch
    { private static void Prefix() { LegacyResourceRuntime.Clear(); } }
    [HarmonyPatch(typeof(CustomAudioPlayer), "PlayVoice")]
    internal static class LegacyExplicitVoicePatch
    { private static void Postfix(bool __result) { LegacyResourceRuntime.VoiceStarted(__result); } }
    [HarmonyPatch(typeof(CustomAudioPlayer), "StopVoice")]
    internal static class LegacyStopVoicePatch
    { private static void Postfix() { LegacyResourceRuntime.VoiceStopped(); } }

    [HarmonyPatch(typeof(PlayerAvatarPanel), "OnEnable")]
    internal static class LegacyPlayerAvatarPatch
    {
        private static void Postfix(PlayerAvatarPanel __instance)
        { LegacyResourceRuntime.ApplyAvatar(Traverse.Create(__instance).Field("_avatarImage").GetValue<Image>(), "player"); }
    }
    [HarmonyPatch(typeof(SocialStatPanel), "UpdateInfo")]
    internal static class LegacySocialAvatarPatch
    {
        private static void Postfix(SocialStatPanel __instance)
        {
            Traverse fields = Traverse.Create(__instance);
            RelationshipStat data = fields.Field("_currentData").GetValue<RelationshipStat>();
            if (data != null && !CustomSocialRuntime.IsCustomData(data)) LegacyResourceRuntime.ApplyAvatar(fields.Field("_avatarImage").GetValue<Image>(), data.Type.GetStringValue());
        }
    }
    [HarmonyPatch(typeof(CombatCharacterStatusUI), "Setup")]
    internal static class LegacyCombatAvatarPatch
    {
        [HarmonyPriority(Priority.Last)]
        private static void Postfix(CombatCharacterStatusUI __instance, ref IEnumerator __result)
        { __result = ApplyAfterLoad(__instance, __result); }
        private static IEnumerator ApplyAfterLoad(CombatCharacterStatusUI ui, IEnumerator original)
        {
            while (original != null && original.MoveNext()) yield return original.Current;
            if (ui == null) yield break;
            Traverse fields = Traverse.Create(ui);
            CombatActionController action = fields.Field("_actionController").GetValue<CombatActionController>();
            if (action != null && action.Stat != null && action.Stat.Data != null)
                LegacyResourceRuntime.ApplyAvatar(fields.Field("_avatar").GetValue<Image>(), action.Stat.Data.Name);
        }
    }
}
