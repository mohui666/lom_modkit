using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    // Extend only the loaded Character instance. Its StoryCharacterData and
    // Addressables resources remain untouched; assets come from the active MOD.
    internal static class OfficialPortraitRuntime
    {
        private sealed class Actor
        {
            internal StoryCharacterController Character;
            internal UserContent Content;
            internal readonly Dictionary<string, Sprite> Sprites = new Dictionary<string, Sprite>(StringComparer.Ordinal);
            internal readonly List<Texture2D> Textures = new List<Texture2D>();
        }

        private static readonly Dictionary<StoryCharacterController, Actor> Actors =
            new Dictionary<StoryCharacterController, Actor>();
        private static readonly MethodInfo AddPortraitObject =
            AccessTools.Method(typeof(StoryCharacterController), "AddPortraitObject", new[] { typeof(Sprite) });
        private static readonly FieldInfo OriginPortrait =
            AccessTools.Field(typeof(StoryCharacterController), "_originPortrait");
        private static ModPackage _owner;
        private static bool _clearing;

        internal static bool TryPortrait(StoryCharacterController character, string key, out Sprite sprite)
        {
            sprite = null;
            if (_clearing) return false;
            ModPackage package = ModOverlay.CurrentPackage;
            if (!ReferenceEquals(_owner, package))
            {
                Clear();
                _owner = package;
            }
            UserContent content;
            string filename;
            if (package == null || character == null || character.Data == null
                || !package.CharacterExtensions.TryGetValue(character.Data.Id, out content)
                || !content.Portraits.TryGetValue(key ?? "", out filename)) return false;
            Actor actor;
            if (!Actors.TryGetValue(character, out actor))
            {
                actor = new Actor { Character = character, Content = content };
                Actors.Add(character, actor);
            }
            if (!actor.Sprites.TryGetValue(key, out sprite) || sprite == null)
            {
                byte[] bytes;
                if (content.Files == null || !content.Files.TryGetValue(filename, out bytes))
                    throw new InvalidOperationException("包内缺少扩展立绘：" + content.Id + "/" + filename);
                Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                if (!texture.LoadImage(bytes))
                {
                    UnityEngine.Object.Destroy(texture);
                    throw new InvalidOperationException("扩展立绘解码失败：" + content.Id + "/" + filename);
                }
                texture.wrapMode = TextureWrapMode.Clamp;
                texture.filterMode = FilterMode.Bilinear;
                sprite = Sprite.Create(texture, new Rect(0f, 0f, texture.width, texture.height),
                    new Vector2(0.5f, 0f), 100f);
                sprite.name = "__lommod__" + package.Id + "__" + content.Id + "__" + key;
                actor.Textures.Add(texture);
                actor.Sprites.Add(key, sprite);
                character.Portraits.Add(sprite);
            }
            Sprite loadedSprite = sprite;
            if (character.State.holder != null
                && !character.State.allPortraits.Exists(image => image != null && image.sprite == loadedSprite))
            {
                if (AddPortraitObject == null)
                    throw new InvalidOperationException("原版缺少 StoryCharacterController.AddPortraitObject(Sprite)");
                AddPortraitObject.Invoke(character, new object[] { sprite });
            }
            ConfigureImages(character);
            return true;
        }

        internal static bool TryPortraitName(StoryCharacterController character, string key, out string name)
        {
            name = null;
            Sprite sprite;
            if (!TryPortrait(character, key, out sprite)) return false;
            if (OriginPortrait == null)
                throw new InvalidOperationException("原版缺少 StoryCharacterController._originPortrait");
            OriginPortrait.SetValue(character, key);
            name = sprite.name;
            return true;
        }

        internal static void ConfigureImages(Character character)
        {
            Actor actor;
            var controller = character as StoryCharacterController;
            if (controller == null || !Actors.TryGetValue(controller, out actor)) return;
            float bodyScale = actor.Content.Scale / 100f;
            bool artLeft = actor.Content.ArtFacing != "right";
            bool nativeLeft = character.PortraitsFace == FacingDirection.Left;
            foreach (Image image in character.State.allPortraits)
            {
                if (image == null || !actor.Sprites.ContainsValue(image.sprite)) continue;
                string key = null;
                foreach (var portrait in actor.Sprites) if (portrait.Value == image.sprite) { key = portrait.Key; break; }
                PortraitCalibration calibration = actor.Content.PortraitLayout(key);
                float scale = bodyScale * calibration.Scale / 100f;
                RectTransform rect = image.rectTransform;
                rect.anchorMin = Vector2.zero;
                rect.anchorMax = Vector2.one;
                rect.pivot = new Vector2(0.5f, 0.5f);
                rect.sizeDelta = Vector2.zero;
                float height = character.State.holder != null ? character.State.holder.rect.height : 1080f;
                // Keep native centre-contain for wide images, while the scaled
                // full slot still grows upward from the actor's foot pivot.
                float width = character.State.holder != null ? character.State.holder.rect.width : 1920f;
                rect.anchoredPosition = new Vector2((artLeft == nativeLeft ? 1f : -1f) * width * calibration.X / 1920f * bodyScale,
                    height * ((scale - 1f) * 0.5f + calibration.Y / 1080f * bodyScale));
                rect.localScale = new Vector3(artLeft == nativeLeft ? scale : -scale, scale, 1f);
                image.raycastTarget = false;
            }
        }

        internal static void Clear()
        {
            _clearing = true;
            try
            {
                foreach (Actor actor in Actors.Values)
                {
                    StoryCharacterController character = actor.Character;
                    if (character != null)
                    {
                        // A MOD can replace the first portrait during Setup. Load
                        // its native counterpart before removing that replacement.
                        if (character.Data != null && character.Data.PortraitResourceList != null
                            && character.Data.PortraitResourceList.Length != 0)
                        {
                            character.LoadPortraitSync(character.Data.PortraitResourceList[0].Mapping.Value);
                            if (OriginPortrait != null)
                                OriginPortrait.SetValue(character, character.Data.PortraitResourceList[0].Mapping.Value);
                        }
                        var images = character.State.allPortraits;
                        Image native = images.Find(image => image != null && !actor.Sprites.ContainsValue(image.sprite));
                        if (character.State.portraitImage != null
                            && actor.Sprites.ContainsValue(character.State.portraitImage.sprite))
                        {
                            character.State.portraitImage = native;
                            if (native != null)
                            {
                                Color color = native.color;
                                color.a = 0f;
                                native.color = color;
                                native.gameObject.SetActive(false);
                            }
                        }
                        for (int i = images.Count - 1; i >= 0; i--)
                        {
                            Image image = images[i];
                            if (image == null || !actor.Sprites.ContainsValue(image.sprite)) continue;
                            images.RemoveAt(i);
                            image.gameObject.SetActive(false);
                            UnityEngine.Object.Destroy(image.gameObject);
                        }
                        foreach (Sprite sprite in actor.Sprites.Values) character.Portraits.Remove(sprite);
                    }
                    foreach (Sprite sprite in actor.Sprites.Values)
                        if (sprite != null) UnityEngine.Object.Destroy(sprite);
                    foreach (Texture2D texture in actor.Textures)
                        if (texture != null) UnityEngine.Object.Destroy(texture);
                }
            }
            finally
            {
                Actors.Clear();
                _owner = null;
                _clearing = false;
            }
        }

        internal static IEnumerator Loaded() { yield break; }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "LoadPortraitSync", new[] { typeof(string) })]
    internal static class OfficialPortraitLoadSyncPatch
    {
        private static bool Prefix(StoryCharacterController __instance, string key)
        {
            Sprite sprite;
            return !OfficialPortraitRuntime.TryPortrait(__instance, key, out sprite);
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "LoadPortrait", new[] { typeof(string) })]
    internal static class OfficialPortraitLoadPatch
    {
        private static bool Prefix(StoryCharacterController __instance, string key, ref IEnumerator __result)
        {
            Sprite sprite;
            if (!OfficialPortraitRuntime.TryPortrait(__instance, key, out sprite)) return true;
            __result = OfficialPortraitRuntime.Loaded();
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryCharacterController), "GetPortraitName", new[] { typeof(string) })]
    internal static class OfficialPortraitNamePatch
    {
        private static bool Prefix(StoryCharacterController __instance, string name, ref string __result)
        {
            string result;
            if (!OfficialPortraitRuntime.TryPortraitName(__instance, name, out result)) return true;
            __result = result;
            return false;
        }
    }

    [HarmonyPatch(typeof(PortraitController), "SetupPortrait", new[] { typeof(PortraitOptions) })]
    internal static class OfficialPortraitSetupPatch
    {
        private static void Postfix(PortraitOptions options)
        {
            OfficialPortraitRuntime.ConfigureImages(options.character);
        }
    }
}
