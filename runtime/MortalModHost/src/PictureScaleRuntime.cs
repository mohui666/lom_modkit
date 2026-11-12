using System;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    // Scale runtime Image instances, leaving official assets and frame geometry untouched.
    internal static class PictureScaleRuntime
    {
        private static readonly Dictionary<string, float> Next = new Dictionary<string, float>(StringComparer.Ordinal);
        private static readonly Dictionary<RectTransform, Vector3> Originals = new Dictionary<RectTransform, Vector3>();

        internal static void Configure(string kind, float percent)
        {
            if (kind != "picture" && kind != "item" && kind != "event" && kind != "big")
                throw new ArgumentException("不支持缩放的图片类型：" + kind);
            if (float.IsNaN(percent) || float.IsInfinity(percent) || percent <= 0f)
                throw new ArgumentOutOfRangeException("percent", "图片大小必须为正有限百分比");
            Next[kind] = percent / 100f;
        }

        private static float Consume(string kind)
        {
            float scale;
            if (!Next.TryGetValue(kind, out scale)) return 1f;
            Next.Remove(kind);
            return scale;
        }

        private static Vector3 Original(RectTransform image)
        {
            Vector3 scale;
            if (!Originals.TryGetValue(image, out scale))
            {
                scale = image.localScale;
                Originals.Add(image, scale);
            }
            return scale;
        }

        internal static void ApplyClone(string kind, RectTransform image, RectTransform originalImage)
        {
            image.localScale = Original(originalImage) * Consume(kind);
        }

        internal static void BeforeShow(StoryPicture picture)
        {
            StoryMainUI main = StoryMainUI.Instance;
            if (main == null) return;
            string kind = null;
            foreach (var slot in new[] { "_picture", "_itemPicture", "_bigPicture" })
                if (Traverse.Create(main).Field(slot).GetValue<StoryPicture>() == picture)
                { kind = slot == "_picture" ? "picture" : slot == "_itemPicture" ? "item" : "big"; break; }
            if (kind == null) return;
            Image image = Traverse.Create(picture).Field("_image").GetValue<Image>();
            if (image == null) throw new InvalidOperationException("图片界面缺少图像");
            image.rectTransform.localScale = Original(image.rectTransform) * Consume(kind);
        }

        internal static void Clear()
        {
            foreach (var pair in Originals)
                if (pair.Key != null) pair.Key.localScale = pair.Value;
            Originals.Clear();
            Next.Clear();
        }
    }

    [HarmonyPatch(typeof(StoryPicture), "Show")]
    internal static class StoryPictureScalePatch
    {
        private static void Prefix(StoryPicture __instance)
        {
            if (ModOverlay.CurrentPackage != null) PictureScaleRuntime.BeforeShow(__instance);
        }
    }
}
