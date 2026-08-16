using System;
using System.Collections;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// 人物介绍卡扩展。官方人物仍直接执行 intropanel.Show(key)，完全沿用游戏的
    /// RelationshipStat、官方头像与本地化文本。自定义人物先由 Lua 全局函数准备文本，
    /// 再用特殊 key 进入同一个 CharacterIntroPanel；Harmony 只接管这个特殊 key。
    /// </summary>
    internal static class CharacterIntroSupport
    {
        internal const string CustomKey = "__lommod_custom_intro__";
        internal static ManualLogSource Log;

        private static IntroData _pending;

        internal sealed class IntroData
        {
            internal readonly string Title;
            internal readonly string Name;
            internal readonly string Intro;
            internal readonly string ImagePath;
            internal readonly float ImageScale;
            internal readonly float ImageX;
            internal readonly float ImageY;

            internal IntroData(
                string title,
                string name,
                string intro,
                string imagePath,
                float imageScale,
                float imageX,
                float imageY)
            {
                Title = title ?? "";
                Name = name ?? "";
                Intro = intro ?? "";
                ImagePath = imagePath ?? "";
                ImageScale = Mathf.Clamp(imageScale, 40f, 160f);
                ImageX = Mathf.Clamp(imageX, -30f, 30f);
                ImageY = Mathf.Clamp(imageY, -30f, 30f);
            }
        }

        internal static void Prepare(
            string title,
            string name,
            string intro,
            string imagePath,
            float imageScale = 100f,
            float imageX = 0f,
            float imageY = 0f)
        {
            _pending = new IntroData(
                title, name, intro, imagePath, imageScale, imageX, imageY);
        }

        internal static void Clear()
        {
            _pending = null;
        }

        internal static bool TryTake(string key, out IntroData data)
        {
            data = null;
            if (key != CustomKey || _pending == null) return false;
            data = _pending;
            _pending = null;
            return true;
        }

        internal static IEnumerator ShowCustom(CharacterIntroPanel panel, IntroData data)
        {
            Traverse fields = Traverse.Create(panel);
            CommonPanel common = fields.Field("_commonPanel").GetValue<CommonPanel>();
            Image avatar = fields.Field("_avatarImage").GetValue<Image>();
            Image avatarBack1 = fields.Field("_avatarImageBack1").GetValue<Image>();
            Image avatarBack2 = fields.Field("_avatarImageBack2").GetValue<Image>();
            Image frame = fields.Field("_frameImage").GetValue<Image>();
            Image brush = fields.Field("_brushImage").GetValue<Image>();
            Text titleText = fields.Field("_titleText").GetValue<Text>();
            Component nameText = fields.Field("_nameText").GetValue<Component>();
            Text introText = fields.Field("_introText").GetValue<Text>();
            Text countText = fields.Field("_countText").GetValue<Text>();
            Button continueButton = fields.Field("_continueButton").GetValue<Button>();
            GameObject continueObject = fields.Field("_continue").GetValue<GameObject>();

            bool avatarActive = IsActive(avatar);
            bool back1Active = IsActive(avatarBack1);
            bool back2Active = IsActive(avatarBack2);
            bool countActive = IsActive(countText);
            bool buttonActive = IsActive(continueButton);
            bool continueActive = continueObject != null && continueObject.activeSelf;
            Sprite avatarSprite = avatar != null ? avatar.sprite : null;
            Sprite back1Sprite = avatarBack1 != null ? avatarBack1.sprite : null;
            Sprite back2Sprite = avatarBack2 != null ? avatarBack2.sprite : null;
            bool avatarPreserve = avatar != null && avatar.preserveAspect;
            bool back1Preserve = avatarBack1 != null && avatarBack1.preserveAspect;
            bool back2Preserve = avatarBack2 != null && avatarBack2.preserveAspect;
            Color avatarColor = avatar != null ? avatar.color : Color.white;
            Color back1Color = avatarBack1 != null ? avatarBack1.color : Color.white;
            Color back2Color = avatarBack2 != null ? avatarBack2.color : Color.white;
            Material avatarMaterial = avatar != null ? avatar.material : null;
            Material back1Material = avatarBack1 != null ? avatarBack1.material : null;
            Material back2Material = avatarBack2 != null ? avatarBack2.material : null;
            Image.Type avatarType = avatar != null ? avatar.type : Image.Type.Simple;
            Image.Type back1Type = avatarBack1 != null ? avatarBack1.type : Image.Type.Simple;
            Image.Type back2Type = avatarBack2 != null ? avatarBack2.type : Image.Type.Simple;
            float avatarFill = avatar != null ? avatar.fillAmount : 1f;
            float back1Fill = avatarBack1 != null ? avatarBack1.fillAmount : 1f;
            float back2Fill = avatarBack2 != null ? avatarBack2.fillAmount : 1f;
            float avatarAlpha = avatar != null ? avatar.canvasRenderer.GetAlpha() : 1f;
            float back1Alpha = avatarBack1 != null ? avatarBack1.canvasRenderer.GetAlpha() : 1f;
            float back2Alpha = avatarBack2 != null ? avatarBack2.canvasRenderer.GetAlpha() : 1f;
            Vector3 back1Position = avatarBack1 != null
                ? avatarBack1.transform.localPosition : Vector3.zero;
            Vector3 back2Position = avatarBack2 != null
                ? avatarBack2.transform.localPosition : Vector3.zero;
            Vector3 avatarScale = avatar != null
                ? avatar.transform.localScale : Vector3.one;
            Vector3 back1Scale = avatarBack1 != null
                ? avatarBack1.transform.localScale : Vector3.one;
            Vector3 back2Scale = avatarBack2 != null
                ? avatarBack2.transform.localScale : Vector3.one;
            RectTransform avatarRect = avatar != null ? avatar.rectTransform : null;
            Vector2 avatarAnchorMin = avatarRect != null
                ? avatarRect.anchorMin : Vector2.zero;
            Vector2 avatarAnchorMax = avatarRect != null
                ? avatarRect.anchorMax : Vector2.one;
            Vector2 avatarPivot = avatarRect != null
                ? avatarRect.pivot : new Vector2(0.5f, 0.5f);
            Vector2 avatarSize = avatarRect != null
                ? avatarRect.sizeDelta : Vector2.zero;
            Vector2 avatarAnchoredPosition = avatarRect != null
                ? avatarRect.anchoredPosition : Vector2.zero;
            Quaternion avatarRotation = avatarRect != null
                ? avatarRect.localRotation : Quaternion.identity;
            Texture2D customTexture = null;
            Sprite customSprite = TryLoadImage(data.ImagePath, out customTexture);

            panel.StopAllCoroutines();
            fields.Field("_pressClose").SetValue(false);
            fields.Field("_currentKey").SetValue(CustomKey);
            try
            {
                if (common != null) common.Show(true);
                bool hasImage = customSprite != null;
                SetImage(avatar, customSprite, hasImage);
                // 原版头像使用带 Cutoff/发光参数的专用材质，面板上一次动画也可能
                // 留下透明度、Filled 或背景位移。普通 PNG 没有这些材质约定，
                // 自定义图统一重置成普通 UI Image；原版的两层发光残影对普通图片
                // 会造成重影，因此自定义卡只显示主图。
                SetActive(avatarBack1, false);
                SetActive(avatarBack2, false);
                if (hasImage)
                {
                    Canvas.ForceUpdateCanvases();
                    ApplyCustomLayout(avatar, customSprite, data);
                }
                if (frame != null) frame.fillAmount = 1f;
                if (brush != null) brush.fillAmount = 1f;
                if (titleText != null) titleText.text = data.Title;
                SetText(nameText, data.Name);
                if (introText != null) introText.text = data.Intro;
                SetActive(countText, false);
                SetActive(continueButton, true);
                if (continueObject != null) continueObject.SetActive(true);

                while (!fields.Field("_pressClose").GetValue<bool>())
                    yield return null;
            }
            finally
            {
                if (common != null) common.Show(false);
                SetActive(avatar, avatarActive);
                SetActive(avatarBack1, back1Active);
                SetActive(avatarBack2, back2Active);
                RestoreImage(avatar, avatarSprite, avatarPreserve);
                RestoreImage(avatarBack1, back1Sprite, back1Preserve);
                RestoreImage(avatarBack2, back2Sprite, back2Preserve);
                RestoreVisualState(avatar, avatarColor, avatarMaterial, avatarType, avatarFill, avatarAlpha);
                RestoreVisualState(avatarBack1, back1Color, back1Material, back1Type, back1Fill, back1Alpha);
                RestoreVisualState(avatarBack2, back2Color, back2Material, back2Type, back2Fill, back2Alpha);
                if (avatarBack1 != null) avatarBack1.transform.localPosition = back1Position;
                if (avatarBack2 != null) avatarBack2.transform.localPosition = back2Position;
                if (avatar != null) avatar.transform.localScale = avatarScale;
                if (avatarBack1 != null) avatarBack1.transform.localScale = back1Scale;
                if (avatarBack2 != null) avatarBack2.transform.localScale = back2Scale;
                if (avatarRect != null)
                {
                    avatarRect.anchorMin = avatarAnchorMin;
                    avatarRect.anchorMax = avatarAnchorMax;
                    avatarRect.pivot = avatarPivot;
                    avatarRect.sizeDelta = avatarSize;
                    avatarRect.anchoredPosition = avatarAnchoredPosition;
                    avatarRect.localRotation = avatarRotation;
                }
                SetActive(countText, countActive);
                SetActive(continueButton, buttonActive);
                if (continueObject != null) continueObject.SetActive(continueActive);
                if (customSprite != null) UnityEngine.Object.Destroy(customSprite);
                if (customTexture != null) UnityEngine.Object.Destroy(customTexture);
            }
        }

        private static Sprite TryLoadImage(string imagePath, out Texture2D texture)
        {
            texture = null;
            if (string.IsNullOrEmpty(imagePath)) return null;
            string key = imagePath.Replace('\\', '/');
            ModPackage package = ModOverlay.CurrentPackage;
            byte[] bytes;
            if (package == null || !package.Assets.TryGetValue(key, out bytes))
            {
                Log?.LogWarning("自定义人物介绍卡找不到包内图片：" + key);
                return null;
            }
            try
            {
                texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                if (!texture.LoadImage(bytes))
                {
                    UnityEngine.Object.Destroy(texture);
                    texture = null;
                    Log?.LogWarning("自定义人物介绍卡图片解码失败：" + key);
                    return null;
                }
                Log?.LogInfo("自定义人物介绍卡图片已加载：" + key
                    + "（" + texture.width + "x" + texture.height + "）");
                return Sprite.Create(
                    texture,
                    new Rect(0f, 0f, texture.width, texture.height),
                    new Vector2(0.5f, 0.5f));
            }
            catch (Exception ex)
            {
                if (texture != null) UnityEngine.Object.Destroy(texture);
                texture = null;
                Log?.LogWarning("自定义人物介绍卡图片解码异常：" + ex.Message);
                return null;
            }
        }

        private static void SetImage(Image image, Sprite sprite, bool active)
        {
            if (image == null) return;
            image.sprite = sprite;
            image.material = null;
            image.color = Color.white;
            image.canvasRenderer.SetAlpha(1f);
            image.type = Image.Type.Simple;
            image.fillAmount = 1f;
            image.preserveAspect = true;
            image.gameObject.SetActive(active);
        }

        private static void RestoreImage(Image image, Sprite sprite, bool preserveAspect)
        {
            if (image == null) return;
            image.sprite = sprite;
            image.preserveAspect = preserveAspect;
        }

        private static void RestoreVisualState(
            Image image,
            Color color,
            Material material,
            Image.Type type,
            float fillAmount,
            float alpha)
        {
            if (image == null) return;
            image.color = color;
            image.material = material;
            image.type = type;
            image.fillAmount = fillAmount;
            image.canvasRenderer.SetAlpha(alpha);
        }

        private static void ApplyCustomLayout(
            Image image,
            Sprite sprite,
            IntroData data)
        {
            if (image == null || sprite == null) return;
            RectTransform rect = image.rectTransform;
            RectTransform parent = rect.parent as RectTransform;
            if (parent == null) return;
            float spriteWidth = Mathf.Max(1f, sprite.rect.width);
            float spriteHeight = Mathf.Max(1f, sprite.rect.height);
            Canvas canvas = image.canvas;
            float canvasScale = canvas != null ? Mathf.Max(0.01f, canvas.scaleFactor) : 1f;
            float userScale = data.ImageScale / 100f;
            float maxWidth = Screen.width * 0.30f / canvasScale * userScale;
            float maxHeight = Screen.height * 0.62f / canvasScale * userScale;
            float contain = Mathf.Min(maxWidth / spriteWidth, maxHeight / spriteHeight);

            // 不再围绕原版下方锚点做 localScale。直接给出最终尺寸，并将图片中心
            // 固定到屏幕左侧安全区；这样方图、横图、竖图和各种分辨率都不会漂移。
            rect.anchorMin = new Vector2(0.5f, 0.5f);
            rect.anchorMax = new Vector2(0.5f, 0.5f);
            rect.pivot = new Vector2(0.5f, 0.5f);
            rect.sizeDelta = new Vector2(spriteWidth * contain, spriteHeight * contain);
            rect.localScale = Vector3.one;
            rect.localRotation = Quaternion.identity;

            Vector2 screenPoint = new Vector2(
                Screen.width * (0.31f + data.ImageX / 100f),
                Screen.height * (0.50f + data.ImageY / 100f));
            Camera camera = canvas != null && canvas.renderMode != RenderMode.ScreenSpaceOverlay
                ? canvas.worldCamera : null;
            Vector2 localPoint;
            if (RectTransformUtility.ScreenPointToLocalPointInRectangle(
                parent, screenPoint, camera, out localPoint))
            {
                float z = rect.localPosition.z;
                rect.localPosition = new Vector3(localPoint.x, localPoint.y, z);
            }
            Log?.LogInfo("自定义人物介绍卡图片布局：scale="
                + data.ImageScale.ToString("0") + "% x="
                + data.ImageX.ToString("0") + "% y="
                + data.ImageY.ToString("0") + "% size="
                + rect.sizeDelta.x.ToString("0") + "x"
                + rect.sizeDelta.y.ToString("0"));
        }

        private static bool IsActive(Component component)
        {
            return component != null && component.gameObject.activeSelf;
        }

        private static void SetActive(Component component, bool active)
        {
            if (component != null) component.gameObject.SetActive(active);
        }

        private static void SetText(Component component, string text)
        {
            if (component == null) return;
            var property = component.GetType().GetProperty("text");
            if (property != null && property.CanWrite) property.SetValue(component, text, null);
        }
    }

    [HarmonyPatch(typeof(CharacterIntroPanel), "Show")]
    internal static class CharacterIntroPanelPatch
    {
        private static bool Prefix(
            CharacterIntroPanel __instance,
            string key,
            ref IEnumerator __result)
        {
            CharacterIntroSupport.IntroData data;
            if (!CharacterIntroSupport.TryTake(key, out data)) return true;
            CharacterIntroSupport.Log?.LogInfo("显示 mod 自定义人物介绍卡：" + data.Name);
            __result = CharacterIntroSupport.ShowCustom(__instance, data);
            return false;
        }

        private static void Postfix(CharacterIntroPanel __instance, ref IEnumerator __result)
        {
            if (ModDisclosure.Active)
            {
                Transform anchor = __instance.transform;
                try
                {
                    Text title = Traverse.Create(__instance).Field("_titleText").GetValue<Text>();
                    if (title != null)
                        anchor = title.rectTransform.parent != null ? title.rectTransform.parent : title.rectTransform;
                }
                catch { }
                if (ModDisclosure.AttachToPanel(anchor) == null)
                {
                    (__result as IDisposable)?.Dispose();
                    __result = ModDisclosure.EmptyRoutine();
                    LuaManagerPatch.AbortActivePlayback(
                        "人物介绍卡无法附加强制玩家内容标记",
                        null, null, "mandatory_disclosure");
                }
            }
        }
    }
}
