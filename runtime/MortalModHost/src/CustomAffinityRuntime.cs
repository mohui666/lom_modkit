using System;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using OBB.Framework.Extensions;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;

namespace MortalModHost
{
    // Values belong to the current package's save sidecar, never to official RelationshipStat assets.
    internal static class CustomAffinityRuntime
    {
        private static readonly Dictionary<UserContent, Sprite> Avatars = new Dictionary<UserContent, Sprite>();
        private static readonly Dictionary<UserContent, Sprite> Portraits = new Dictionary<UserContent, Sprite>();
        private static readonly Dictionary<string, AsyncOperationHandle<Sprite>> StoryPortraits =
            new Dictionary<string, AsyncOperationHandle<Sprite>>(StringComparer.Ordinal);

        internal static bool UsesSidecar(string character)
        {
            return ContentRef.IsUserRef(character) || ContentRef.IsStoryAffinityRef(character);
        }

        internal static UserContent RequireCharacter(ModPackage package, string character)
        {
            ContentRef reference; string error; UserContent content;
            if (package == null || !ContentRef.TryParse(character, out reference, out error)
                || !package.TryGetUserContent(reference.ContentId, out content)
                || content.Type != "character" || !string.IsNullOrEmpty(content.Extends))
                throw new ArgumentException("本作品中找不到独立自定义人物：" + character);
            return content;
        }

        internal static int Value(ModPackage package, string character)
        {
            int initial;
            if (ContentRef.IsStoryAffinityRef(character))
            {
                if (!ContentRef.IsAffinityKey(character))
                    throw new ArgumentException("原版剧情人物的作品好感 ID 无效：" + character);
                initial = 0;
            }
            else initial = RequireCharacter(package, character).InitialAffinity;
            return PersistentModState.CharacterAffinity(package, character, initial);
        }

        internal static void Change(ModPackage package, string character, int value, bool set, bool display)
        {
            int current = Value(package, character);
            int next = set ? Mathf.Clamp(value, 0, 100) : (int)Math.Max(0L, Math.Min(100L, (long)current + value));
            PersistentModState.SetCharacterAffinity(package, character, next);
            if (!display || next == current || SystemMessageScope.Suppressed) return;
            StoryMainUI ui = StoryMainUI.Instance;
            if (ui == null) throw new InvalidOperationException("好感提示的剧情界面尚未就绪");
            RelationshipModifyMessagePanel panel = Traverse.Create(ui).Field("_relationshipMessagePanel").GetValue<RelationshipModifyMessagePanel>();
            if (panel == null) throw new InvalidOperationException("剧情界面缺少好感提示面板");
            Sprite avatar = ContentRef.IsStoryAffinityRef(character)
                ? StoryAvatar(package, character.Substring(ContentRef.StoryAffinityPrefix.Length))
                : Avatar(RequireCharacter(package, character));
            panel.DisplayRelationshipModification(avatar, next - current);
        }

        private static Sprite StoryAvatar(ModPackage package, string id)
        {
            UserContent extension;
            if (package.CharacterExtensions.TryGetValue(id, out extension)
                && !string.IsNullOrEmpty(extension.AvatarFile)) return Avatar(extension);

            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            var config = placeholder == null ? null
                : Traverse.Create(placeholder).Field("_config").GetValue<StoryCharacterConfig>();
            StoryCharacterData data = config == null ? null : config.Get(id);
            if (data == null) throw new InvalidOperationException("找不到剧情好感提示的人物：" + id);
            string address = null;
            foreach (StoryCharaterImageItem portrait in data.PortraitResourceList ?? Array.Empty<StoryCharaterImageItem>())
            {
                if (portrait == null || portrait.Mapping == null || string.IsNullOrEmpty(portrait.AddressKey)) continue;
                if (portrait.Mapping.Value == "normal")
                {
                    address = portrait.AddressKey;
                    break;
                }
                if (address == null) address = portrait.AddressKey;
            }
            if (address == null) return null;
            AsyncOperationHandle<Sprite> handle;
            if (!StoryPortraits.TryGetValue(address, out handle))
            {
                handle = Addressables.LoadAssetAsync<Sprite>(address);
                StoryPortraits.Add(address, handle);
            }
            return handle.WaitForCompletion();
        }

        internal static Sprite Avatar(UserContent content)
        {
            Sprite sprite;
            if (Avatars.TryGetValue(content, out sprite) && sprite != null) return sprite;
            string file = string.IsNullOrEmpty(content.AvatarFile) ? content.MainFile : content.AvatarFile;
            sprite = LoadSprite(content, file);
            Avatars[content] = sprite;
            return sprite;
        }

        internal static Sprite Portrait(UserContent content)
        {
            if (string.IsNullOrEmpty(content.AvatarFile) || content.AvatarFile == content.MainFile) return Avatar(content);
            Sprite sprite;
            if (Portraits.TryGetValue(content, out sprite) && sprite != null) return sprite;
            sprite = LoadSprite(content, content.MainFile);
            Portraits[content] = sprite;
            return sprite;
        }

        private static Sprite LoadSprite(UserContent content, string file)
        {
            byte[] bytes;
            if (content.Files == null || !content.Files.TryGetValue(file, out bytes) || bytes == null)
                throw new InvalidOperationException("人物头像文件缺失：" + content.Id + "/" + file);
            var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(bytes))
            {
                UnityEngine.Object.Destroy(texture);
                throw new InvalidOperationException("人物头像图片无法解码：" + content.Id + "/" + file);
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            return Sprite.Create(texture, new Rect(0, 0, texture.width, texture.height), new Vector2(.5f, .5f));
        }

        internal static Sprite MessageAvatar(RelationshipStat relationship)
        {
            if (relationship == null || ModOverlay.CurrentPackage == null) return null;
            string id = relationship.Type.GetStringValue();
            UserContent extension;
            if (ModOverlay.CurrentPackage.CharacterExtensions.TryGetValue(id, out extension)
                && !string.IsNullOrEmpty(extension.AvatarFile)) return Avatar(extension);
            if (id == "player" && relationship.Icon == null) return PlayerNormalAvatarRuntime.Get();
            return relationship.Icon;
        }

        internal static void Clear()
        {
            PlayerNormalAvatarRuntime.Clear();
            foreach (var handle in StoryPortraits.Values)
                if (handle.IsValid()) Addressables.Release(handle);
            StoryPortraits.Clear();
            foreach (Sprite sprite in Portraits.Values)
                if (sprite != null)
                {
                    UnityEngine.Object.Destroy(sprite.texture);
                    UnityEngine.Object.Destroy(sprite);
                }
            Portraits.Clear();
            foreach (Sprite sprite in Avatars.Values)
                if (sprite != null)
                {
                    UnityEngine.Object.Destroy(sprite.texture);
                    UnityEngine.Object.Destroy(sprite);
                }
            Avatars.Clear();
        }
    }

    [HarmonyPatch(typeof(StatModifyManager), "Character", new[] { typeof(string), typeof(int), typeof(int) })]
    internal static class CustomAffinityChangePatch
    {
        private static bool Prefix(string name, int value, int display)
        {
            if (!CustomAffinityRuntime.UsesSidecar(name)) return true;
            CustomAffinityRuntime.Change(ModOverlay.CurrentPackage, name, value, false, display == 1);
            return false;
        }
    }

    [HarmonyPatch(typeof(StatModifyManager), "SetCharacter", new[] { typeof(string), typeof(int) })]
    internal static class CustomAffinitySetPatch
    {
        private static bool Prefix(string name, int value)
        {
            if (!CustomAffinityRuntime.UsesSidecar(name)) return true;
            CustomAffinityRuntime.Change(ModOverlay.CurrentPackage, name, value, true, false);
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryMainUI), "DisplayRelationshipModification", new[] { typeof(RelationshipStat), typeof(int) })]
    internal static class RelationshipAvatarPatch
    {
        // The original method still records the correct official character in the story log.
        private static void Prefix(ref RelationshipStat relationship, out RelationshipStat __state)
        {
            __state = null;
            Sprite avatar = CustomAffinityRuntime.MessageAvatar(relationship);
            if (avatar == null || avatar == relationship.Icon) return;
            __state = UnityEngine.Object.Instantiate(relationship);
            Traverse.Create(__state).Field("_icon").SetValue(avatar);
            relationship = __state;
        }
        private static Exception Finalizer(RelationshipStat __state, Exception __exception)
        {
            if (__state != null) UnityEngine.Object.Destroy(__state);
            return __exception;
        }
    }

    [HarmonyPatch(typeof(CustomGameplayRuntime), "Clear")]
    internal static class CustomAffinityResourceClearPatch
    {
        private static void Prefix() { CustomSocialRuntime.Clear(); CustomAffinityRuntime.Clear(); }
    }
}
