using System;
using System.Collections;
using System.Collections.Generic;
using BepInEx.Logging;
using Fungus;
using Mortal.Core;
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
        private static string _backgroundReference = "";
        private static string _pendingBackgroundReference = "";
        private static Coroutine _backgroundRestore;
        private static GameObject _cgRoot;
        private static Image _cgImage;
        private static Sprite _cgSprite;
        private static Texture2D _cgTexture;
        private static Coroutine _cgFade;
        private sealed class OverlayVisual
        {
            internal GameObject Root;
            internal Image Image;
            internal Sprite Sprite;
            internal Texture2D Texture;
            internal Coroutine Fade;
        }
        private static readonly Dictionary<string, OverlayVisual> Overlays =
            new Dictionary<string, OverlayVisual>(StringComparer.Ordinal);

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
            _backgroundReference = raw;
            float duration = Mathf.Max(0f, seconds);
            SetAlpha(image, duration > 0f ? 0f : 1f);
            if (_host != null && duration > 0f)
                _fade = _host.StartCoroutine(FadeTo(1f, duration, false));
            if (Log != null) Log.LogInfo("显示自定义背景：" + raw);
            return true;
        }

        public static void ClearBackground(float seconds)
        {
            _backgroundReference = "";
            if (_root == null)
                return;
            StopFade();
            float duration = Mathf.Max(0f, seconds);
            if (_host != null && duration > 0f && _image != null)
                _fade = _host.StartCoroutine(FadeTo(0f, duration, true));
            else
                DestroyVisual();
        }

        public static bool ShowCg(
            string raw, float seconds, float scalePercent, float xPercent, float yPercent)
        {
            ContentRef parsed;
            string error;
            if (!ContentRef.TryParse(raw, out parsed, out error))
            {
                Warn("自定义 CG 引用无效：" + error);
                return false;
            }
            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            if (package == null || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null || !string.Equals(content.Type, "image", StringComparison.Ordinal)
                || content.Bytes == null || content.Bytes.Length == 0)
            {
                Warn("自定义 CG 不存在或不是图片：" + raw);
                return false;
            }
            Stage stage = Stage.GetActiveStage();
            if (stage == null || stage.PortraitCanvas == null)
            {
                Warn("当前没有可用的剧情舞台，无法显示自定义 CG：" + raw);
                return false;
            }

            StopCgFade();
            DestroyCgVisual();
            Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(content.Bytes))
            {
                UnityEngine.Object.Destroy(texture);
                Warn("自定义 CG 解码失败：" + raw);
                return false;
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Sprite.Create(
                texture,
                new Rect(0f, 0f, texture.width, texture.height),
                new Vector2(0.5f, 0.5f),
                100f);

            GameObject root = new GameObject("lom_custom_cg", typeof(RectTransform));
            root.transform.SetParent(stage.PortraitCanvas.transform, false);
            root.transform.SetAsLastSibling();
            RectTransform rootRect = root.GetComponent<RectTransform>();
            rootRect.anchorMin = Vector2.zero;
            rootRect.anchorMax = Vector2.one;
            rootRect.offsetMin = Vector2.zero;
            rootRect.offsetMax = Vector2.zero;
            rootRect.pivot = new Vector2(0.5f, 0.5f);
            float scale = Mathf.Clamp(scalePercent, 10f, 300f) / 100f;
            rootRect.localScale = new Vector3(scale, scale, 1f);
            Rect parentRect = ((RectTransform)stage.PortraitCanvas.transform).rect;
            rootRect.anchoredPosition = new Vector2(
                parentRect.width * Mathf.Clamp(xPercent, -100f, 100f) / 100f,
                parentRect.height * Mathf.Clamp(yPercent, -100f, 100f) / 100f);

            GameObject imageObject = new GameObject(
                "image", typeof(RectTransform), typeof(CanvasRenderer),
                typeof(Image), typeof(AspectRatioFitter));
            imageObject.transform.SetParent(root.transform, false);
            RectTransform imageRect = imageObject.GetComponent<RectTransform>();
            imageRect.anchorMin = Vector2.zero;
            imageRect.anchorMax = Vector2.one;
            imageRect.offsetMin = Vector2.zero;
            imageRect.offsetMax = Vector2.zero;
            Image image = imageObject.GetComponent<Image>();
            image.sprite = sprite;
            image.type = Image.Type.Simple;
            image.preserveAspect = false;
            image.raycastTarget = false;
            image.material = null;
            AspectRatioFitter fitter = imageObject.GetComponent<AspectRatioFitter>();
            fitter.aspectMode = AspectRatioFitter.AspectMode.FitInParent;
            fitter.aspectRatio = texture.height > 0 ? (float)texture.width / texture.height : 16f / 9f;

            _cgRoot = root;
            _cgImage = image;
            _cgSprite = sprite;
            _cgTexture = texture;
            float duration = Mathf.Max(0f, seconds);
            SetAlpha(image, duration > 0f ? 0f : 1f);
            if (_host != null && duration > 0f)
                _cgFade = _host.StartCoroutine(FadeCgTo(1f, duration, false));
            if (Log != null) Log.LogInfo("显示自定义 CG：" + raw);
            return true;
        }

        public static void HideCg(float seconds)
        {
            if (_cgRoot == null)
                return;
            StopCgFade();
            float duration = Mathf.Max(0f, seconds);
            if (_host != null && duration > 0f && _cgImage != null)
                _cgFade = _host.StartCoroutine(FadeCgTo(0f, duration, true));
            else
                DestroyCgVisual();
        }

        public static bool ShowOverlay(
            string slot, string raw, string position, float scalePercent,
            float opacityPercent, string layer, float seconds)
        {
            if (string.IsNullOrEmpty(slot))
            {
                Warn("插图槽位不能为空");
                return false;
            }
            ContentRef parsed;
            string error;
            if (!ContentRef.TryParse(raw, out parsed, out error))
            {
                Warn("插图引用无效：" + error);
                return false;
            }
            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            if (package == null || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null || !string.Equals(content.Type, "image", StringComparison.Ordinal)
                || content.Bytes == null || content.Bytes.Length == 0)
            {
                Warn("插图不存在或不是图片：" + raw);
                return false;
            }
            Stage stage = Stage.GetActiveStage();
            if (stage == null || stage.PortraitCanvas == null)
            {
                Warn("当前没有可用的剧情舞台，无法显示插图：" + raw);
                return false;
            }

            DestroyOverlay(slot);
            Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(content.Bytes))
            {
                UnityEngine.Object.Destroy(texture);
                Warn("插图解码失败：" + raw);
                return false;
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Sprite.Create(
                texture, new Rect(0f, 0f, texture.width, texture.height),
                new Vector2(0.5f, 0.5f), 100f);

            GameObject root = new GameObject(
                "lom_overlay_" + slot, typeof(RectTransform), typeof(Canvas));
            root.transform.SetParent(stage.PortraitCanvas.transform, false);
            Canvas canvas = root.GetComponent<Canvas>();
            bool front = !string.Equals(layer, "back", StringComparison.Ordinal);
            canvas.overrideSorting = front;
            canvas.sortingOrder = front ? 10 : 0;
            if (front)
                root.transform.SetAsLastSibling();
            else
                root.transform.SetSiblingIndex(Mathf.Min(1, root.transform.parent.childCount - 1));
            RectTransform rootRect = root.GetComponent<RectTransform>();
            rootRect.anchorMin = Vector2.zero;
            rootRect.anchorMax = Vector2.one;
            rootRect.offsetMin = Vector2.zero;
            rootRect.offsetMax = Vector2.zero;
            rootRect.pivot = new Vector2(0.5f, 0.5f);
            float scale = Mathf.Clamp(scalePercent, 10f, 300f) / 100f;
            rootRect.localScale = new Vector3(scale, scale, 1f);
            rootRect.anchoredPosition = OverlayPosition(position, ((RectTransform)stage.PortraitCanvas.transform).rect);

            GameObject imageObject = new GameObject(
                "image", typeof(RectTransform), typeof(CanvasRenderer),
                typeof(Image), typeof(AspectRatioFitter));
            imageObject.transform.SetParent(root.transform, false);
            RectTransform imageRect = imageObject.GetComponent<RectTransform>();
            imageRect.anchorMin = Vector2.zero;
            imageRect.anchorMax = Vector2.one;
            imageRect.offsetMin = Vector2.zero;
            imageRect.offsetMax = Vector2.zero;
            Image image = imageObject.GetComponent<Image>();
            image.sprite = sprite;
            image.type = Image.Type.Simple;
            image.preserveAspect = false;
            image.raycastTarget = false;
            image.material = null;
            AspectRatioFitter fitter = imageObject.GetComponent<AspectRatioFitter>();
            fitter.aspectMode = AspectRatioFitter.AspectMode.FitInParent;
            fitter.aspectRatio = texture.height > 0 ? (float)texture.width / texture.height : 1f;

            OverlayVisual visual = new OverlayVisual
            {
                Root = root, Image = image, Sprite = sprite, Texture = texture
            };
            Overlays[slot] = visual;
            float target = Mathf.Clamp01(opacityPercent / 100f);
            float duration = Mathf.Max(0f, seconds);
            SetAlpha(image, duration > 0f ? 0f : target);
            if (_host != null && duration > 0f)
                visual.Fade = _host.StartCoroutine(FadeOverlayTo(slot, visual, target, duration, false));
            if (Log != null) Log.LogInfo("显示插图槽位 " + slot + "：" + raw);
            return true;
        }

        public static void HideOverlay(string slot, float seconds)
        {
            OverlayVisual visual;
            if (!Overlays.TryGetValue(slot ?? "", out visual))
                return;
            StopOverlayFade(visual);
            float duration = Mathf.Max(0f, seconds);
            if (_host != null && duration > 0f && visual.Image != null)
                visual.Fade = _host.StartCoroutine(FadeOverlayTo(slot, visual, 0f, duration, true));
            else
                DestroyOverlay(slot);
        }

        public static void ClearAll()
        {
            StopFade();
            DestroyVisual();
            StopCgFade();
            DestroyCgVisual();
            foreach (string slot in new List<string>(Overlays.Keys))
                DestroyOverlay(slot);
        }

        /// <summary>
        /// 原版 GameSave 不记录包内图片背景。保存时只记录已验证过的 user:image
        /// 引用，读档后等新的 Story Stage 就绪再重新挂载，不能把旧场景的对象留下。
        /// </summary>
        internal static string ActiveBackgroundReference
        {
            get { return _root != null ? _backgroundReference : ""; }
        }

        internal static void RestoreBackgroundWhenStageReady(string raw)
        {
            _pendingBackgroundReference = raw ?? "";
            if (_backgroundRestore != null && _host != null)
                _host.StopCoroutine(_backgroundRestore);
            _backgroundRestore = null;
            if (string.IsNullOrEmpty(_pendingBackgroundReference) || _host == null)
                return;
            Stage previous = Stage.GetActiveStage();
            _backgroundRestore = _host.StartCoroutine(
                RestoreBackgroundAfterStageChange(previous));
        }

        /// <summary>
        /// 同一 MOD 的 end.next_script 不会切换 Story 场景。保留已显示的作者背景，
        /// 但只在当前 Stage 已完成加载时立即挂回；读档路径仍使用
        /// RestoreBackgroundWhenStageReady，不能把图片挂到即将卸载的旧舞台。
        /// </summary>
        internal static void RestoreBackgroundForScriptContinuation(string raw)
        {
            if (string.IsNullOrEmpty(raw) || _host == null) return;
            SceneController scenes = SceneController.Instance;
            bool sceneLoading = scenes != null && (scenes.IsPrepare || scenes.IsLoading);
            Stage stage = Stage.GetActiveStage();
            if (sceneLoading || stage == null || stage.PortraitCanvas == null
                || ModOverlay.CurrentPackage == null)
            {
                RestoreBackgroundWhenStageReady(raw);
                return;
            }
            _pendingBackgroundReference = "";
            if (_backgroundRestore != null)
                _host.StopCoroutine(_backgroundRestore);
            _backgroundRestore = null;
            ShowBackground(raw, 0f);
        }

        private static IEnumerator RestoreBackgroundAfterStageChange(Stage previous)
        {
            float deadline = Time.unscaledTime + 12f;
            bool observedSceneLoad = false;
            while (Time.unscaledTime < deadline)
            {
                SceneController scenes = SceneController.Instance;
                bool sceneLoading = scenes != null && (scenes.IsPrepare || scenes.IsLoading);
                if (sceneLoading) observedSceneLoad = true;
                Stage stage = Stage.GetActiveStage();
                if (stage != null && stage.PortraitCanvas != null
                    && ModOverlay.CurrentPackage != null && !sceneLoading
                    && (stage != previous || observedSceneLoad))
                {
                    string raw = _pendingBackgroundReference;
                    _pendingBackgroundReference = "";
                    _backgroundRestore = null;
                    ShowBackground(raw, 0f);
                    yield break;
                }
                yield return null;
            }
            _backgroundRestore = null;
            Warn("剧情背景恢复超时，未找到新的 Story 舞台");
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

        private static IEnumerator FadeCgTo(float target, float seconds, bool destroy)
        {
            if (_cgImage == null)
                yield break;
            float start = _cgImage.color.a;
            float elapsed = 0f;
            while (elapsed < seconds && _cgImage != null)
            {
                elapsed += Time.unscaledDeltaTime;
                SetAlpha(_cgImage, Mathf.Lerp(start, target, Mathf.Clamp01(elapsed / seconds)));
                yield return null;
            }
            if (_cgImage != null) SetAlpha(_cgImage, target);
            _cgFade = null;
            if (destroy) DestroyCgVisual();
        }

        private static IEnumerator FadeOverlayTo(
            string slot, OverlayVisual visual, float target, float seconds, bool destroy)
        {
            if (visual == null || visual.Image == null)
                yield break;
            float start = visual.Image.color.a;
            float elapsed = 0f;
            while (elapsed < seconds && visual.Image != null)
            {
                elapsed += Time.unscaledDeltaTime;
                SetAlpha(visual.Image, Mathf.Lerp(start, target, Mathf.Clamp01(elapsed / seconds)));
                yield return null;
            }
            if (visual.Image != null) SetAlpha(visual.Image, target);
            visual.Fade = null;
            if (destroy) DestroyOverlay(slot);
        }

        private static void StopFade()
        {
            if (_fade != null && _host != null)
                _host.StopCoroutine(_fade);
            _fade = null;
        }

        private static void StopCgFade()
        {
            if (_cgFade != null && _host != null)
                _host.StopCoroutine(_cgFade);
            _cgFade = null;
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

        private static void DestroyCgVisual()
        {
            if (_cgRoot != null) UnityEngine.Object.Destroy(_cgRoot);
            if (_cgSprite != null) UnityEngine.Object.Destroy(_cgSprite);
            if (_cgTexture != null) UnityEngine.Object.Destroy(_cgTexture);
            _cgRoot = null;
            _cgImage = null;
            _cgSprite = null;
            _cgTexture = null;
        }

        private static void StopOverlayFade(OverlayVisual visual)
        {
            if (visual != null && visual.Fade != null && _host != null)
                _host.StopCoroutine(visual.Fade);
            if (visual != null) visual.Fade = null;
        }

        private static void DestroyOverlay(string slot)
        {
            OverlayVisual visual;
            if (!Overlays.TryGetValue(slot ?? "", out visual))
                return;
            StopOverlayFade(visual);
            if (visual.Root != null) UnityEngine.Object.Destroy(visual.Root);
            if (visual.Sprite != null) UnityEngine.Object.Destroy(visual.Sprite);
            if (visual.Texture != null) UnityEngine.Object.Destroy(visual.Texture);
            Overlays.Remove(slot);
        }

        private static Vector2 OverlayPosition(string position, Rect parent)
        {
            float x = 0f;
            float y = 0f;
            switch (position ?? "center")
            {
                case "left": x = -0.32f; break;
                case "right": x = 0.32f; break;
                case "top": y = 0.32f; break;
                case "bottom": y = -0.32f; break;
                case "top_left": x = -0.32f; y = 0.32f; break;
                case "top_right": x = 0.32f; y = 0.32f; break;
                case "bottom_left": x = -0.32f; y = -0.32f; break;
                case "bottom_right": x = 0.32f; y = -0.32f; break;
            }
            return new Vector2(parent.width * x, parent.height * y);
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
