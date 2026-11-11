using System;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;

namespace MortalModHost
{
    // The notification uses a square head crop of the official normal portrait.
    // Only our Sprite and Addressables reference are owned here; never its texture.
    internal static class PlayerNormalAvatarRuntime
    {
        private static AsyncOperationHandle<Sprite> _handle;
        private static Sprite _head;
        internal static Sprite Source { get; private set; }
        internal static string Address { get; private set; }

        internal static Sprite Get()
        {
            if (_head != null) return _head;
            var placeholder = CharacterPlaceholder.Instance;
            var config = placeholder != null ? Traverse.Create(placeholder).Field("_config").GetValue<StoryCharacterConfig>() : null;
            var data = config != null ? config.Get("player") : null;
            if (data == null) throw new InvalidOperationException("赵活普通头像缺少原版 StoryCharacterConfig");
            string address = null;
            foreach (StoryCharaterImageItem portrait in data.PortraitResourceList)
                if (portrait != null && portrait.Mapping != null && portrait.Mapping.Value == "normal")
                {
                    address = portrait.AddressKey;
                    break;
                }
            if (string.IsNullOrEmpty(address)) throw new InvalidOperationException("赵活原版配置缺少 normal 立绘地址");
            _handle = Addressables.LoadAssetAsync<Sprite>(address);
            Source = _handle.WaitForCompletion();
            if (Source == null) throw new InvalidOperationException("赵活原版 normal 立绘加载失败：" + address);
            Address = address;
            // Measured against the current normal sprite's content rectangle:
            // retain hair, the neutral face and the top of the collar.
            Rect content = Source.textureRect;
            float side = Mathf.Round(content.width * .67f);
            // The measured GPU readback uses the opposite vertical direction.
            // Convert its top-based coordinate to Sprite.Create's texture pixels.
            float top = Mathf.Round(content.y + content.height * .31f);
            var crop = new Rect(Mathf.Round(content.x + content.width * .18f),
                Source.texture.height - top - side, side, side);
            _head = Sprite.Create(Source.texture, crop, new Vector2(.5f, .5f), 100f, 0, SpriteMeshType.FullRect);
            _head.name = "lom_player_normal_head";
            return _head;
        }

        internal static void Clear()
        {
            if (_head != null) UnityEngine.Object.Destroy(_head);
            _head = null;
            Source = null;
            Address = null;
            if (_handle.IsValid()) Addressables.Release(_handle);
            _handle = default(AsyncOperationHandle<Sprite>);
        }
    }
}
