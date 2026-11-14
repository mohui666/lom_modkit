using System;
using System.Collections;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;
using UnityEngine.UI;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    // Preserve the native two-stage reveal: background first, then image/motion.
    // Blank pictures stop after the background, without loading a placeholder.
    internal static class BigPictureRuntime
    {
        private static GameObject _root;
        private static Sprite _sprite;
        private static Texture2D _texture;
        private static AsyncOperationHandle<Sprite> _handle;
        private static float? _nextFade;
        private static float? _nextHideFade;
        private static float _hideDuration;
        internal static bool Active { get { return _root != null; } }

        internal static void Configure(float fade)
        {
            if (float.IsNaN(fade) || float.IsInfinity(fade) || fade < 0f)
                throw new ArgumentOutOfRangeException("fade", "大图淡入时间必须为非负有限数值");
            _nextFade = fade;
        }

        internal static void ConfigureHide(float fade)
        {
            if (float.IsNaN(fade) || float.IsInfinity(fade) || fade < 0f)
                throw new ArgumentOutOfRangeException("fade", "大图淡出时间必须为非负有限数值");
            _nextHideFade = fade;
        }

        internal static bool TryShow(string key, int color, out IEnumerator routine)
        {
            float? fade = _nextFade;
            _nextFade = null;
            bool extended = fade.HasValue || string.IsNullOrEmpty(key) || key.StartsWith("user:", StringComparison.Ordinal)
                || OfficialItemPictureRuntime.IsReference(key);
            routine = extended ? Show(key, color, fade) : null;
            if (!extended) Clear();
            return extended;
        }

        private static IEnumerator Show(string key, int color, float? fade)
        {
            Clear();
            StoryMainUI main = StoryMainUI.Instance;
            StoryPicture original = main == null ? null : Traverse.Create(main).Field("_bigPicture").GetValue<StoryPicture>();
            if (original == null) throw new InvalidOperationException("当前剧情没有原版大图界面");
            // A native picture may precede this one. Release it through its own API.
            yield return original.Hide();
            Image originalBackground = Traverse.Create(main).Field("_bigPictureBackground").GetValue<Image>();
            if (originalBackground == null || !originalBackground.transform.IsChildOf(original.transform))
                throw new InvalidOperationException("原版大图底色不在大图实例中，无法使用此布局");
            float seconds = fade ?? Traverse.Create(original).Field("_imageFadeInDuration").GetValue<float>();
            _hideDuration = Traverse.Create(original).Field("_fadeoutDuration").GetValue<float>();
            try
            {
                Sprite picture = null;
                bool custom = !string.IsNullOrEmpty(key) && key.StartsWith("user:", StringComparison.Ordinal);
                bool inventory = OfficialItemPictureRuntime.TryResolve(key, out picture);
                if (custom)
                {
                    UserContent content = CustomGameplayRuntime.ResolveContent(ModOverlay.CurrentPackage, key);
                    if (!content.IsStoryPicture || content.Bytes == null) throw new InvalidOperationException("自定义大图不存在：" + key);
                    _texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                    if (!_texture.LoadImage(content.Bytes)) throw new InvalidOperationException("自定义大图解码失败：" + key);
                    _texture.wrapMode = TextureWrapMode.Clamp;
                    _texture.filterMode = FilterMode.Bilinear;
                    picture = _sprite = Sprite.Create(_texture, new Rect(0, 0, _texture.width, _texture.height), new Vector2(.5f, .5f));
                }
                else if (!inventory && !string.IsNullOrEmpty(key))
                {
                    _handle = Addressables.LoadAssetAsync<Sprite>("pic_" + key);
                    picture = _handle.WaitForCompletion();
                    if (picture == null) throw new InvalidOperationException("原版大图加载失败：" + key);
                }
                // Locate the background by child indices, preserving the original hierarchy.
                var indices = new Stack<int>();
                for (Transform item = originalBackground.transform; item != original.transform; item = item.parent)
                    indices.Push(item.GetSiblingIndex());
                _root = UObject.Instantiate(original.gameObject, original.transform.parent, false);
                _root.name = "lom_big_picture";
                StoryPicture controller = _root.GetComponent<StoryPicture>();
                controller.enabled = false;
                Transform backgroundTransform = _root.transform;
                while (indices.Count > 0) backgroundTransform = backgroundTransform.GetChild(indices.Pop());
                Image background = backgroundTransform.GetComponent<Image>();
                background.color = color == 1 ? Color.black : color == 2 ? Color.clear : Color.white;
                background.raycastTarget = false;
                Image image = Traverse.Create(controller).Field("_image").GetValue<Image>();
                image.sprite = picture;
                image.color = new Color(1, 1, 1, 0);
                image.raycastTarget = false;
                image.enabled = picture != null;
                image.rectTransform.anchoredPosition = Traverse.Create(original).Field("_originAnchoredPosition").GetValue<Vector2>();
                if (picture != null)
                {
                    if (custom || inventory)
                    {
                        RectTransform frame = _root.GetComponent<RectTransform>();
                        RectTransform viewport = image.canvas.rootCanvas.GetComponent<RectTransform>();
                        var corners = new Vector3[4];
                        viewport.GetWorldCorners(corners);
                        Vector3 lower = frame.InverseTransformPoint(corners[0]);
                        Vector3 upper = frame.InverseTransformPoint(corners[2]);
                        float width = Mathf.Min(frame.rect.width, Mathf.Abs(upper.x - lower.x));
                        float height = Mathf.Min(frame.rect.height, Mathf.Abs(upper.y - lower.y));
                        float scale = Mathf.Min(width / picture.rect.width, height / picture.rect.height);
                        // The native image stretches across its parent. A fitted sizeDelta
                        // would otherwise add the parent's size and crop the custom art.
                        image.rectTransform.anchorMin = image.rectTransform.anchorMax = new Vector2(.5f, .5f);
                        image.rectTransform.pivot = new Vector2(.5f, .5f);
                        image.rectTransform.sizeDelta = picture.rect.size * scale;
                        image.preserveAspect = true;
                    }
                    else if (Traverse.Create(original).Field("_setNativeSize").GetValue<bool>()) image.SetNativeSize();
                }
                CanvasGroup group = _root.GetComponent<CanvasGroup>();
                PictureScaleRuntime.ApplyClone("big", image.rectTransform, Traverse.Create(original).Field("_image").GetValue<Image>().rectTransform);
                group.alpha = 0f;
                group.blocksRaycasts = false;
                group.interactable = false;
                _root.SetActive(true);
            }
            catch { Clear(); throw; }
            GameObject shown = _root;
            CanvasGroup canvas = shown.GetComponent<CanvasGroup>();
            yield return Fade(canvas, 0, 1, seconds);
            if (_root != shown || canvas == null) yield break;
            Image contentImage = Traverse.Create(shown.GetComponent<StoryPicture>()).Field("_image").GetValue<Image>();
            if (!contentImage.enabled) yield break;
            Vector2 origin = contentImage.rectTransform.anchoredPosition;
            Vector2 offset = Traverse.Create(original).Field("_moveOffset").GetValue<Vector2>();
            float elapsed = 0;
            do
            {
                elapsed += Time.deltaTime;
                float t = seconds <= 0 ? 1 : Mathf.Clamp01(elapsed / seconds);
                // Ease out while revealing the image and its native movement offset.
                float eased = 1 - (1 - t) * (1 - t);
                contentImage.color = new Color(1, 1, 1, eased);
                contentImage.rectTransform.anchoredPosition = origin + offset * eased;
                if (t >= 1) break;
                yield return null;
            } while (_root == shown && contentImage != null);
        }

        private static IEnumerator Fade(CanvasGroup group, float from, float to, float seconds)
        {
            float elapsed = 0;
            while (group != null)
            {
                elapsed += Time.deltaTime;
                float t = seconds <= 0 ? 1 : Mathf.Clamp01(elapsed / seconds);
                float eased = 1 - (1 - t) * (1 - t);
                group.alpha = Mathf.Lerp(from, to, eased);
                if (t >= 1) yield break;
                yield return null;
            }
        }

        internal static bool TryHide(StoryMainUI main, out IEnumerator routine)
        {
            float? fade = _nextHideFade;
            _nextHideFade = null;
            if (Active) { routine = Hide(fade ?? _hideDuration); return true; }
            routine = null;
            if (!fade.HasValue) return false;
            StoryPicture original = Traverse.Create(main).Field("_bigPicture").GetValue<StoryPicture>();
            if (original == null) throw new InvalidOperationException("当前剧情没有原版大图界面");
            routine = HideOriginal(original, fade.Value);
            return true;
        }

        private static IEnumerator HideOriginal(StoryPicture original, float seconds)
        {
            Traverse duration = Traverse.Create(original).Field("_fadeoutDuration");
            float previous = duration.GetValue<float>();
            try
            {
                duration.SetValue(seconds);
                yield return original.Hide();
            }
            finally { if (original != null) duration.SetValue(previous); }
        }

        private static IEnumerator Hide(float seconds)
        {
            GameObject shown = _root;
            if (shown == null) yield break;
            CanvasGroup group = shown.GetComponent<CanvasGroup>();
            yield return Fade(group, group.alpha, 0, seconds);
            if (_root == shown) Clear();
        }

        internal static void Clear()
        {
            _nextFade = null;
            _nextHideFade = null;
            if (_root != null) { _root.SetActive(false); UObject.Destroy(_root); }
            _root = null;
            if (_sprite != null) UObject.Destroy(_sprite);
            if (_texture != null) UObject.Destroy(_texture);
            _sprite = null; _texture = null;
            if (_handle.IsValid()) Addressables.Release(_handle);
            _handle = default(AsyncOperationHandle<Sprite>);
        }
    }

    [HarmonyPatch(typeof(StoryMainUI), "ShowBigPicture", new[] { typeof(string), typeof(int) })]
    internal static class CustomBigPictureShowPatch
    {
        private static bool Prefix(string key, int color, ref IEnumerator __result)
        {
            if (ModOverlay.CurrentPackage == null) return true;
            IEnumerator routine;
            if (!BigPictureRuntime.TryShow(key, color, out routine)) return true;
            __result = routine;
            return false;
        }
    }
    [HarmonyPatch(typeof(StoryMainUI), "HideBigPicture")]
    internal static class CustomBigPictureHidePatch
    {
        private static bool Prefix(StoryMainUI __instance, ref IEnumerator __result)
        {
            IEnumerator routine;
            if (!BigPictureRuntime.TryHide(__instance, out routine)) return true;
            __result = routine;
            return false;
        }
    }
}
