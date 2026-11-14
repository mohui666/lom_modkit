using System;
using System.Collections;
using System.Globalization;
using System.Text;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;
using UnityEngine.UI;

namespace MortalModHost
{
    // Clone StoryMainUI._picture so custom captions and images stay instance-owned.
    internal static class EventPictureRuntime
    {
        private static GameObject _root;
        private static Texture2D _texture;
        private static Sprite _sprite;
        private static AsyncOperationHandle<Sprite> _handle;
        private static MonoBehaviour _host;
        private static Coroutine _fade;
        private static UserContent _cachedContent;
        internal static bool IsFading { get { return _fade != null; } }

        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static void Show(string key, string title, float fadeIn = 0f)
        {
            if (float.IsNaN(fadeIn) || float.IsInfinity(fadeIn) || fadeIn < 0f)
                throw new ArgumentOutOfRangeException("fadeIn", "事件图淡入时间必须为非负有限数值");
            ClearVisual();
            CustomPictureRuntime.ClearPicture();
            try
            {
                StoryMainUI main = StoryMainUI.Instance;
                StoryPicture original = main == null ? null
                    : Traverse.Create(main).Field("_picture").GetValue<StoryPicture>();
                if (original == null)
                    throw new InvalidOperationException("当前剧情没有可用的书页事件界面");

                Sprite picture;
                if (key.StartsWith("user:", StringComparison.Ordinal))
                {
                    ContentRef reference;
                    string error;
                    if (!ContentRef.TryParse(key, out reference, out error))
                        throw new InvalidOperationException(error);
                    UserContent content;
                    ModPackage package = ModOverlay.CurrentPackage;
                    if (package == null || !package.TryGetUserContent(reference.ContentId, out content)
                        || !content.IsStoryPicture || content.Bytes == null)
                        throw new InvalidOperationException("事件图片不存在：" + key);
                    if (!ReferenceEquals(_cachedContent, content) || _texture == null || _sprite == null)
                    {
                        ReleaseCachedPicture();
                        _texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                        if (!_texture.LoadImage(content.Bytes, true))
                            throw new InvalidOperationException("事件图片解码失败：" + key);
                        _texture.wrapMode = TextureWrapMode.Clamp;
                        _texture.filterMode = FilterMode.Bilinear;
                        _sprite = Sprite.Create(_texture,
                            new Rect(0, 0, _texture.width, _texture.height), new Vector2(0.5f, 0.5f));
                        _cachedContent = content;
                    }
                    picture = _sprite;
                }
                else if (!OfficialItemPictureRuntime.TryResolve(key, out picture))
                {
                    _handle = Addressables.LoadAssetAsync<Sprite>("pic_" + key);
                    picture = _handle.WaitForCompletion();
                    if (picture == null)
                        throw new InvalidOperationException("原版事件图片加载失败：" + key);
                }

                _root = UnityEngine.Object.Instantiate(original.gameObject, original.transform.parent, false);
                _root.name = "lom_event_picture";
                StoryPicture controller = _root.GetComponent<StoryPicture>();
                controller.enabled = false;
                Image image = Traverse.Create(controller).Field("_image").GetValue<Image>();
                if (image == null)
                    throw new InvalidOperationException("原版事件图框不可用");
                image.sprite = picture;
                image.color = Color.white;
                image.type = Image.Type.Simple;
                image.preserveAspect = true;
                image.raycastTarget = false;
                RectTransform rect = image.rectTransform;
                rect.anchorMin = Vector2.zero;
                rect.anchorMax = Vector2.one;
                rect.pivot = new Vector2(0.5f, 0.5f);
                rect.offsetMin = Vector2.zero;
                rect.offsetMax = Vector2.zero;
                PictureScaleRuntime.ApplyClone("event", rect, Traverse.Create(original).Field("_image").GetValue<Image>().rectTransform);

                Text caption = _root.transform.Find("Text").GetComponent<Text>();
                foreach (MonoBehaviour component in caption.GetComponents<MonoBehaviour>())
                    if (component.GetType().Name == "LeanLocalizedText")
                        component.enabled = false;
                string captionText = string.IsNullOrEmpty(title) ? "事件" : title;
                TextElementEnumerator letters = StringInfo.GetTextElementEnumerator(captionText.Replace("\r", "").Replace("\n", ""));
                StringBuilder verticalText = new StringBuilder();
                while (letters.MoveNext()) verticalText.AppendLine(letters.GetTextElement());
                caption.text = verticalText.ToString().TrimEnd('\r', '\n');
                caption.resizeTextForBestFit = true;
                caption.resizeTextMinSize = 12;
                caption.resizeTextMaxSize = caption.fontSize;
                CanvasGroup group = _root.GetComponent<CanvasGroup>();
                group.alpha = fadeIn > 0f ? 0f : 1f;
                group.blocksRaycasts = false;
                group.interactable = false;
                _root.SetActive(true);
                if (fadeIn > 0f)
                {
                    if (_host == null) throw new InvalidOperationException("事件图淡入尚未准备，请更新游戏接入组件后重新试玩");
                    _fade = _host.StartCoroutine(FadeIn(group, fadeIn));
                }
            }
            catch
            {
                Clear();
                throw;
            }
        }

        private static IEnumerator FadeIn(CanvasGroup group, float seconds)
        {
            float started = Time.timeSinceLevelLoad;
            float elapsed = 0f;
            while (group != null && elapsed < seconds)
            {
                // Match the exact clock used by the installed Lua/fungus wait().
                // A skip-speed change must affect the wait and reveal together.
                elapsed = Time.timeSinceLevelLoad - started;
                group.alpha = Mathf.Clamp01(elapsed / seconds);
                yield return null;
            }
            if (group != null) group.alpha = 1f;
            _fade = null;
        }

        internal static void Hide(float fadeOut)
        {
            if (float.IsNaN(fadeOut) || float.IsInfinity(fadeOut) || fadeOut < 0f)
                throw new ArgumentOutOfRangeException("fadeOut", "事件图淡出时间必须为非负有限数值");
            if (fadeOut == 0f || _root == null)
            {
                ClearVisual();
                return;
            }
            if (_host == null)
                throw new InvalidOperationException("事件图淡出尚未准备，请更新游戏接入组件后重新试玩");
            CanvasGroup group = _root.GetComponent<CanvasGroup>();
            if (group == null)
                throw new InvalidOperationException("当前事件图缺少淡出所需的 CanvasGroup");
            if (_fade != null) _host.StopCoroutine(_fade);
            _fade = _host.StartCoroutine(FadeOut(group, fadeOut));
        }

        private static IEnumerator FadeOut(CanvasGroup group, float seconds)
        {
            float start = group.alpha;
            float started = Time.timeSinceLevelLoad;
            float elapsed = 0f;
            while (group != null && elapsed < seconds)
            {
                elapsed = Time.timeSinceLevelLoad - started;
                group.alpha = start * (1f - Mathf.Clamp01(elapsed / seconds));
                yield return null;
            }
            if (group != null) group.alpha = 0f;
            _fade = null;
            ClearVisual();
        }

        internal static void Clear()
        {
            ClearVisual();
            ReleaseCachedPicture();
        }

        private static void ReleaseCachedPicture()
        {
            if (_sprite != null) UnityEngine.Object.Destroy(_sprite);
            if (_texture != null) UnityEngine.Object.Destroy(_texture);
            _sprite = null;
            _texture = null;
            _cachedContent = null;
        }

        private static void ClearVisual()
        {
            if (_fade != null && _host != null) _host.StopCoroutine(_fade);
            _fade = null;
            if (_root != null)
            {
                _root.SetActive(false);
                UnityEngine.Object.Destroy(_root);
            }
            _root = null;
            if (_handle.IsValid()) Addressables.Release(_handle);
            _handle = default(AsyncOperationHandle<Sprite>);
        }
    }
}
