using System;
using System.Collections;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    // Live API: StoryMainUI.ShowPicture/ShowItemPicture delegate to StoryPicture.Show.
    // Clone each native panel so its frame, placement and reveal remain instance-owned.
    internal static class CustomPictureRuntime
    {
        private sealed class Visual
        {
            internal GameObject Root;
            internal Texture2D Texture;
            internal Sprite Sprite;
            internal CanvasGroup Group;
            internal Image Image;
            internal float HideDuration;

            internal void Destroy()
            {
                if (Root != null) { Root.SetActive(false); UObject.Destroy(Root); }
                if (Sprite != null) UObject.Destroy(Sprite);
                if (Texture != null) UObject.Destroy(Texture);
            }
        }

        private static readonly Dictionary<string, Visual> Pictures = new Dictionary<string, Visual>();

        internal static bool TryShow(StoryMainUI main, string slot, string key, out IEnumerator routine)
        {
            if (slot == "_picture") EventPictureRuntime.Clear();
            routine = null;
            if (ModOverlay.CurrentPackage != null && key != null && key.StartsWith("user:", StringComparison.Ordinal))
            {
                routine = Show(main, slot, key, ModOverlay.CurrentPackage);
                return true;
            }
            Clear(slot);
            return false;
        }

        private static IEnumerator Show(StoryMainUI main, string slot, string key, ModPackage package)
        {
            Clear(slot);
            StoryPicture original = Traverse.Create(main).Field(slot).GetValue<StoryPicture>();
            if (original == null) throw new InvalidOperationException("当前剧情没有对应的图片界面：" + slot);
            UserContent content = CustomGameplayRuntime.ResolveContent(package, key);
            if (!content.IsStoryPicture || content.Bytes == null)
                throw new InvalidOperationException("图片需要已导入的图片、物品或功法：" + key);
            yield return original.Hide();

            var visual = new Visual();
            float duration = Traverse.Create(original).Field("_imageFadeInDuration").GetValue<float>();
            Vector2 origin = Traverse.Create(original).Field("_originAnchoredPosition").GetValue<Vector2>();
            Vector2 offset = Traverse.Create(original).Field("_moveOffset").GetValue<Vector2>();
            try
            {
                visual.Texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                if (!visual.Texture.LoadImage(content.Bytes)) throw new InvalidOperationException("图片解码失败：" + key);
                visual.Texture.wrapMode = TextureWrapMode.Clamp;
                visual.Texture.filterMode = FilterMode.Bilinear;
                visual.Sprite = Sprite.Create(visual.Texture,
                    new Rect(0, 0, visual.Texture.width, visual.Texture.height), new Vector2(.5f, .5f));
                visual.Root = UObject.Instantiate(original.gameObject, original.transform.parent, false);
                visual.Root.name = "lom_custom" + slot;
                var controller = visual.Root.GetComponent<StoryPicture>();
                controller.enabled = false;
                visual.Image = Traverse.Create(controller).Field("_image").GetValue<Image>();
                visual.Image.sprite = visual.Sprite;
                PictureScaleRuntime.ApplyClone(slot == "_itemPicture" ? "item" : "picture", visual.Image.rectTransform,
                    Traverse.Create(original).Field("_image").GetValue<Image>().rectTransform);
                visual.Image.type = Image.Type.Simple;
                visual.Image.preserveAspect = true;
                visual.Image.raycastTarget = false;
                visual.Image.color = new Color(1, 1, 1, 0);
                visual.Image.rectTransform.anchoredPosition = origin;
                RectTransform imageRect = visual.Image.rectTransform;
                Vector3 frameCenter = imageRect.TransformPoint(imageRect.rect.center);
                imageRect.pivot = new Vector2(.5f, .5f);
                imageRect.position = frameCenter;
                origin = imageRect.anchoredPosition;
                visual.Group = visual.Root.GetComponent<CanvasGroup>();
                visual.Group.alpha = 0;
                visual.Group.blocksRaycasts = false;
                visual.Group.interactable = false;
                visual.HideDuration = Traverse.Create(original).Field("_fadeoutDuration").GetValue<float>();
                Pictures[slot] = visual;
                visual.Root.SetActive(true);
            }
            catch { visual.Destroy(); Pictures.Remove(slot); throw; }

            yield return Fade(slot, visual, 0, 1, duration);
            float elapsed = 0;
            while (IsCurrent(slot, visual))
            {
                elapsed += Time.deltaTime;
                float t = duration <= 0 ? 1 : Mathf.Clamp01(elapsed / duration);
                float eased = 1 - (1 - t) * (1 - t);
                visual.Image.color = new Color(1, 1, 1, eased);
                visual.Image.rectTransform.anchoredPosition = origin + offset * eased;
                if (t >= 1) yield break;
                yield return null;
            }
        }

        private static bool IsCurrent(string slot, Visual visual)
        {
            Visual active;
            return Pictures.TryGetValue(slot, out active) && active == visual && visual.Root != null;
        }

        private static IEnumerator Fade(string slot, Visual visual, float from, float to, float duration)
        {
            float elapsed = 0;
            while (IsCurrent(slot, visual))
            {
                elapsed += Time.deltaTime;
                float t = duration <= 0 ? 1 : Mathf.Clamp01(elapsed / duration);
                float eased = 1 - (1 - t) * (1 - t);
                visual.Group.alpha = Mathf.Lerp(from, to, eased);
                if (t >= 1) yield break;
                yield return null;
            }
        }

        internal static bool TryHide(string slot, out IEnumerator routine)
        {
            Visual visual;
            routine = Pictures.TryGetValue(slot, out visual) ? Hide(slot, visual) : null;
            return routine != null;
        }

        private static IEnumerator Hide(string slot, Visual visual)
        {
            if (!IsCurrent(slot, visual)) yield break;
            yield return Fade(slot, visual, visual.Group.alpha, 0, visual.HideDuration);
            if (IsCurrent(slot, visual)) Clear(slot);
        }

        private static void Clear(string slot)
        {
            Visual visual;
            if (!Pictures.TryGetValue(slot, out visual)) return;
            Pictures.Remove(slot);
            visual.Destroy();
        }

        internal static void ClearAll()
        {
            foreach (string slot in new List<string>(Pictures.Keys)) Clear(slot);
        }

        internal static void ClearPicture() { Clear("_picture"); }
    }

    [HarmonyPatch(typeof(StoryMainUI), "ShowPicture")]
    internal static class CustomPictureShowPatch
    {
        private static bool Prefix(StoryMainUI __instance, string key, ref IEnumerator __result)
        {
            IEnumerator routine;
            if (!CustomPictureRuntime.TryShow(__instance, "_picture", key, out routine)) return true;
            __result = routine;
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryMainUI), "ShowItemPicture")]
    internal static class CustomItemPictureShowPatch
    {
        private static bool Prefix(StoryMainUI __instance, string key, ref IEnumerator __result)
        {
            IEnumerator routine;
            if (!CustomPictureRuntime.TryShow(__instance, "_itemPicture", key, out routine)) return true;
            __result = routine;
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryMainUI), "HidePicture")]
    internal static class CustomPictureHidePatch
    {
        private static bool Prefix(ref IEnumerator __result)
        {
            IEnumerator routine;
            if (!CustomPictureRuntime.TryHide("_picture", out routine)) return true;
            __result = routine;
            return false;
        }
    }

    [HarmonyPatch(typeof(StoryMainUI), "HideItemPicture")]
    internal static class CustomItemPictureHidePatch
    {
        private static bool Prefix(ref IEnumerator __result)
        {
            IEnumerator routine;
            if (!CustomPictureRuntime.TryHide("_itemPicture", out routine)) return true;
            __result = routine;
            return false;
        }
    }
}
