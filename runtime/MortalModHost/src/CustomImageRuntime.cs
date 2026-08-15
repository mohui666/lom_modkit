using System;
using System.Collections;
using BepInEx.Logging;
using Fungus;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// 包内 user:image 的舞台背景层。只挂在当前官方 Stage 的 PortraitCanvas，
    /// 不注册或替换原版 View 资源；换章、换场景时由宿主统一销毁。
    /// </summary>
    internal static class CustomImageRuntime
    {
        internal static ManualLogSource Log;

        private static MonoBehaviour _host;
        private static GameObject _root;
        private static Image _image;
        private static Sprite _sprite;
        private static Texture2D _texture;
        private static Coroutine _fade;

        public static void Init(MonoBehaviour host)
        {
            _host = host;
        }

        public static bool ShowBackground(string raw, float seconds)
        {
            ContentRef parsed;
            string error;
            if (!ContentRef.TryParse(raw, out parsed, out error))
            {
                Warn("自定义背景引用无效：" + error);
                return false;
            }
            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            if (package == null || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null || !string.Equals(content.Type, "image", StringComparison.Ordinal)
                || content.Bytes == null || content.Bytes.Length == 0)
            {
                Warn("自定义背景不存在或不是图片：" + raw);
                return false;
            }
            Stage stage = Stage.GetActiveStage();
            if (stage == null || stage.PortraitCanvas == null)
            {
                Warn("当前没有可用的剧情舞台，无法显示自定义背景：" + raw);
                return false;
            }

            StopFade();
            DestroyVisual();
            Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(content.Bytes))
            {
                UnityEngine.Object.Destroy(texture);
                Warn("自定义背景解码失败：" + raw);
                return false;
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Sprite.Create(
                texture,
                new Rect(0f, 0f, texture.width, texture.height),
                new Vector2(0.5f, 0.5f),
                100f);

            GameObject root = new GameObject(
                "lom_custom_background", typeof(RectTransform), typeof(CanvasRenderer),
                typeof(Image), typeof(AspectRatioFitter));
            root.transform.SetParent(stage.PortraitCanvas.transform, false);
            root.transform.SetAsFirstSibling();
            RectTransform rect = root.GetComponent<RectTransform>();
            rect.anchorMin = Vector2.zero;
            rect.anchorMax = Vector2.one;
            rect.offsetMin = Vector2.zero;
            rect.offsetMax = Vector2.zero;
            rect.pivot = new Vector2(0.5f, 0.5f);
            Image image = root.GetComponent<Image>();
            image.sprite = sprite;
            image.type = Image.Type.Simple;
            image.preserveAspect = false;
            image.raycastTarget = false;
            image.material = null;
            AspectRatioFitter fitter = root.GetComponent<AspectRatioFitter>();
            fitter.aspectMode = AspectRatioFitter.AspectMode.EnvelopeParent;
            fitter.aspectRatio = texture.height > 0 ? (float)texture.width / texture.height : 16f / 9f;

            _root = root;
            _image = image;
            _sprite = sprite;
            _texture = texture;
            float duration = Mathf.Max(0f, seconds);
            SetAlpha(image, duration > 0f ? 0f : 1f);
            if (_host != null && duration > 0f)
                _fade = _host.StartCoroutine(FadeTo(1f, duration, false));
            if (Log != null) Log.LogInfo("显示自定义背景：" + raw);
            return true;
        }

        public static void ClearBackground(float seconds)
        {
            if (_root == null)
                return;
            StopFade();
            float duration = Mathf.Max(0f, seconds);
            if (_host != null && duration > 0f && _image != null)
                _fade = _host.StartCoroutine(FadeTo(0f, duration, true));
            else
                DestroyVisual();
        }

        public static void ClearAll()
        {
            StopFade();
            DestroyVisual();
        }

        private static IEnumerator FadeTo(float target, float seconds, bool destroy)
        {
            if (_image == null)
                yield break;
            float start = _image.color.a;
            float elapsed = 0f;
            while (elapsed < seconds && _image != null)
            {
                elapsed += Time.unscaledDeltaTime;
                SetAlpha(_image, Mathf.Lerp(start, target, Mathf.Clamp01(elapsed / seconds)));
                yield return null;
            }
            if (_image != null) SetAlpha(_image, target);
            _fade = null;
            if (destroy) DestroyVisual();
        }

        private static void StopFade()
        {
            if (_fade != null && _host != null)
                _host.StopCoroutine(_fade);
            _fade = null;
        }

        private static void DestroyVisual()
        {
            if (_root != null) UnityEngine.Object.Destroy(_root);
            if (_sprite != null) UnityEngine.Object.Destroy(_sprite);
            if (_texture != null) UnityEngine.Object.Destroy(_texture);
            _root = null;
            _image = null;
            _sprite = null;
            _texture = null;
        }

        private static void SetAlpha(Image image, float alpha)
        {
            if (image == null) return;
            Color color = image.color;
            color.a = alpha;
            image.color = color;
        }

        private static void Warn(string message)
        {
            if (Log != null) Log.LogWarning(message);
        }
    }
}
