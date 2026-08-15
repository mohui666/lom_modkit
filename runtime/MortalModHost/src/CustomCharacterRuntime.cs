using System;
using System.Collections;
using System.Collections.Generic;
using BepInEx.Logging;
using Fungus;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// 自定义角色立绘：不注册进官方 Addressables / CharacterPlaceholder。
    /// 在官方 Stage 画布上自建 Image，按 position 名字对齐舞台站位。
    /// </summary>
    internal static class CustomCharacterRuntime
    {
        internal static ManualLogSource Log;

        private static MonoBehaviour _host;
        private static readonly Dictionary<string, Actor> Actors = new Dictionary<string, Actor>(StringComparer.Ordinal);

        private sealed class Actor
        {
            public string RawId;
            public string Name;
            public string Portrait = "normal";
            public string Facing = "right";
            public string Slot;
            public GameObject Holder;
            public RectTransform HolderRect;
            public Image Image;
            public bool Visible;
            public readonly Dictionary<string, Sprite> Sprites = new Dictionary<string, Sprite>(StringComparer.Ordinal);
            public readonly List<Texture2D> Textures = new List<Texture2D>();
        }

        public static void Init(MonoBehaviour host)
        {
            _host = host;
        }

        public static void ClearAll()
        {
            List<string> keys = new List<string>(Actors.Keys);
            for (int i = 0; i < keys.Count; i++)
                DestroyActor(keys[i]);
            Actors.Clear();
        }

        public static bool Show(string raw, string portrait, string position, string facing, float fade, float move)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            if (!ApplyPortrait(actor, portrait))
                return false;
            Place(actor, position);
            ApplyFacing(actor, facing);
            actor.Holder.SetActive(true);
            actor.Visible = true;
            Focus(raw);
            if (_host != null && fade > 0f)
                _host.StartCoroutine(FadeImage(actor.Image, 0f, 1f, fade));
            else if (actor.Image != null)
                SetAlpha(actor.Image, 1f);
            if (Log != null)
                Log.LogInfo("自定义角色登场：" + raw + " @" + position + " / " + actor.Portrait);
            return true;
        }

        public static bool Hide(string raw, float fade)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null)
                return true;
            if (_host != null && fade > 0f && actor.Image != null)
            {
                _host.StartCoroutine(FadeThenHide(actor, fade));
                return true;
            }
            if (actor.Holder != null)
                actor.Holder.SetActive(false);
            actor.Visible = false;
            return true;
        }

        public static bool Move(string raw, string from, string to, float duration)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            if (!actor.Visible)
            {
                Place(actor, string.IsNullOrEmpty(from) ? to : from);
                actor.Holder.SetActive(true);
                actor.Visible = true;
            }
            RectTransform dest = FindPosition(to);
            if (dest == null)
            {
                if (Log != null) Log.LogWarning("自定义角色找不到站位 " + to);
                return false;
            }
            actor.Slot = to;
            if (_host != null && duration > 0f)
                _host.StartCoroutine(MoveTo(actor, dest, duration));
            else
                FitHolderToSprite(actor, dest);
            return true;
        }

        public static bool Face(string raw, string facing)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null)
                return false;
            ApplyFacing(actor, facing);
            return true;
        }

        public static bool Focus(string raw)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null || actor.Holder == null)
                return false;
            actor.Holder.transform.SetAsLastSibling();
            return true;
        }

        public static bool SetPortrait(string raw, string portrait)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            return ApplyPortrait(actor, portrait);
        }

        public static bool SetSpeaker(string raw)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            try
            {
                SayDialog dialog = SayDialog.GetSayDialog();
                if (dialog != null)
                    dialog.SetCharacterName(actor.Name, Color.white);
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogDebug("设置自定义角色发言名失败：" + ex.Message);
            }
            return true;
        }

        private static bool TryGetOrCreate(string raw, out Actor actor)
        {
            actor = null;
            ContentRef parsed;
            string error;
            if (!ContentRef.TryParse(raw, out parsed, out error))
            {
                if (Log != null) Log.LogWarning("自定义角色引用无效：" + error);
                return false;
            }
            if (Actors.TryGetValue(parsed.Raw, out actor) && actor != null && actor.Holder != null)
                return true;

            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            if (package == null || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null || content.Type != "character")
            {
                if (Log != null)
                    Log.LogWarning("当前 Mod 包内找不到自定义角色 " + parsed.Raw);
                return false;
            }

            Stage stage = Stage.GetActiveStage();
            if (stage == null || stage.PortraitCanvas == null)
            {
                if (Log != null) Log.LogWarning("当前没有官方舞台，无法显示自定义角色");
                return false;
            }

            actor = new Actor
            {
                RawId = parsed.Raw,
                Name = string.IsNullOrEmpty(content.Name) ? parsed.LocalId : content.Name
            };
            GameObject holder = new GameObject("lom_char_" + parsed.ContentId, typeof(RectTransform));
            holder.transform.SetParent(stage.PortraitCanvas.transform, false);
            actor.Holder = holder;
            actor.HolderRect = holder.GetComponent<RectTransform>();
            holder.SetActive(false);

            GameObject imageObj = new GameObject("portrait", typeof(RectTransform), typeof(CanvasRenderer), typeof(Image));
            imageObj.transform.SetParent(holder.transform, false);
            actor.Image = imageObj.GetComponent<Image>();
            actor.Image.preserveAspect = true;
            actor.Image.raycastTarget = false;
            actor.Image.material = null;
            actor.Image.type = Image.Type.Simple;
            actor.Image.color = new Color(1f, 1f, 1f, 0f);
            actor.Image.enabled = false;
            RectTransform imageRect = imageObj.GetComponent<RectTransform>();
            imageRect.anchorMin = Vector2.zero;
            imageRect.anchorMax = Vector2.one;
            imageRect.offsetMin = Vector2.zero;
            imageRect.offsetMax = Vector2.zero;
            imageRect.pivot = new Vector2(0.5f, 0.5f);

            Actors[parsed.Raw] = actor;
            return true;
        }

        private static bool ApplyPortrait(Actor actor, string portrait)
        {
            if (string.IsNullOrEmpty(portrait))
                portrait = "normal";
            Sprite sprite = GetSprite(actor, portrait);
            if (sprite == null)
                return false;
            actor.Portrait = portrait;
            if (actor.Image != null)
            {
                actor.Image.sprite = sprite;
                actor.Image.enabled = true;
                actor.Image.color = Color.white;
            }
            FitHolderToSprite(actor);
            return true;
        }

        private static Sprite GetSprite(Actor actor, string portrait)
        {
            Sprite cached;
            if (actor.Sprites.TryGetValue(portrait, out cached) && cached != null)
                return cached;

            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            ContentRef parsed;
            string ignore;
            if (!ContentRef.TryParse(actor.RawId, out parsed, out ignore)
                || package == null
                || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null)
            {
                if (Log != null) Log.LogWarning("自定义角色立绘找不到包：" + actor.RawId);
                return null;
            }

            string filename = null;
            if (content.Portraits != null)
                content.Portraits.TryGetValue(portrait, out filename);
            if (string.IsNullOrEmpty(filename))
                filename = content.MainFile;
            byte[] bytes = null;
            if (content.Files != null && !string.IsNullOrEmpty(filename))
                content.Files.TryGetValue(filename, out bytes);
            if (bytes == null)
                bytes = content.Bytes;
            if (bytes == null || bytes.Length == 0)
            {
                if (Log != null)
                    Log.LogWarning("自定义角色 " + actor.RawId + " 没有表情 " + portrait);
                return null;
            }

            Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(bytes))
            {
                UnityEngine.Object.Destroy(texture);
                if (Log != null) Log.LogWarning("自定义角色立绘解码失败：" + actor.RawId + "/" + portrait);
                return null;
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Sprite.Create(
                texture,
                new Rect(0f, 0f, texture.width, texture.height),
                new Vector2(0.5f, 0f),
                100f);
            actor.Textures.Add(texture);
            actor.Sprites[portrait] = sprite;
            return sprite;
        }

        private static void Place(Actor actor, string position)
        {
            actor.Slot = position;
            RectTransform dest = FindPosition(position);
            if (dest == null)
            {
                if (Log != null) Log.LogWarning("自定义角色找不到站位 " + position + "，使用默认位");
                FitHolderToSprite(actor, null);
                return;
            }
            actor.HolderRect.rotation = dest.rotation;
            FitHolderToSprite(actor, dest);
        }

        private static void FitHolderToSprite(Actor actor)
        {
            FitHolderToSprite(actor, string.IsNullOrEmpty(actor.Slot) ? null : FindPosition(actor.Slot));
        }

        private static void FitHolderToSprite(Actor actor, RectTransform dest)
        {
            if (actor == null || actor.HolderRect == null || actor.Image == null)
                return;
            Sprite sprite = actor.Image.sprite;
            float sw = sprite != null ? sprite.rect.width : 531f;
            float sh = sprite != null ? sprite.rect.height : 1039f;
            if (sw < 1f) sw = 531f;
            if (sh < 1f) sh = 1039f;

            float canvasH = Screen.height;
            Stage stage = Stage.GetActiveStage();
            if (stage != null && stage.PortraitCanvas != null)
            {
                Rect pixel = stage.PortraitCanvas.pixelRect;
                if (pixel.height > 1f)
                    canvasH = pixel.height;
            }
            if (canvasH < 1f)
                canvasH = 1080f;

            // 官方立绘是塞进站位槽里的，槽大约半个画面高。不能按原图像素
            // 或 90% 屏高来摆，师姐那张 1039px 图会顶满整个舞台。
            float boxH = canvasH * 0.56f;
            float boxW = boxH * (sw / sh);
            Vector2 pivot = new Vector2(0.5f, 0f);
            if (dest != null)
            {
                float slotH = Mathf.Abs(dest.rect.height);
                float slotW = Mathf.Abs(dest.rect.width);
                bool usable = slotH >= canvasH * 0.2f && slotH <= canvasH * 0.72f
                    && slotW >= 80f && slotW <= canvasH * 0.5f;
                if (usable)
                {
                    boxH = slotH;
                    boxW = slotW;
                    pivot = dest.pivot;
                }
            }

            float scale = Mathf.Min(boxW / sw, boxH / sh);
            if (scale <= 0f)
                scale = 1f;

            actor.HolderRect.anchorMin = pivot;
            actor.HolderRect.anchorMax = pivot;
            actor.HolderRect.pivot = pivot;
            actor.HolderRect.sizeDelta = new Vector2(sw * scale, sh * scale);
            if (dest != null)
                actor.HolderRect.position = dest.position;
        }

        private static RectTransform FindPosition(string position)
        {
            Stage stage = Stage.GetActiveStage();
            if (stage == null)
                return null;
            if (!string.IsNullOrEmpty(position))
            {
                RectTransform found = stage.GetPosition(position);
                if (found != null)
                    return found;
            }
            return stage.DefaultPosition != null ? stage.DefaultPosition.rectTransform : null;
        }

        private static void ApplyFacing(Actor actor, string facing)
        {
            if (actor == null || actor.HolderRect == null)
                return;
            actor.Facing = string.Equals(facing, "left", StringComparison.OrdinalIgnoreCase) ? "left" : "right";
            float sx = actor.Facing == "left" ? -1f : 1f;
            Vector3 scale = actor.HolderRect.localScale;
            scale.x = sx * Mathf.Abs(scale.x < 0.001f ? 1f : scale.x);
            actor.HolderRect.localScale = scale;
        }

        private static void SetAlpha(Image image, float alpha)
        {
            if (image == null)
                return;
            Color c = image.color;
            c.a = alpha;
            image.color = c;
        }

        private static IEnumerator FadeImage(Image image, float from, float to, float seconds)
        {
            if (image == null)
                yield break;
            SetAlpha(image, from);
            float elapsed = 0f;
            while (elapsed < seconds && image != null)
            {
                elapsed += Time.unscaledDeltaTime;
                SetAlpha(image, Mathf.Lerp(from, to, Mathf.Clamp01(elapsed / seconds)));
                yield return null;
            }
            SetAlpha(image, to);
        }

        private static IEnumerator FadeThenHide(Actor actor, float seconds)
        {
            yield return FadeImage(actor.Image, 1f, 0f, seconds);
            if (actor.Holder != null)
                actor.Holder.SetActive(false);
            actor.Visible = false;
        }

        private static IEnumerator MoveTo(Actor actor, RectTransform dest, float seconds)
        {
            if (actor.HolderRect == null || dest == null)
                yield break;
            Vector3 start = actor.HolderRect.position;
            Vector3 end = dest.position;
            float elapsed = 0f;
            while (elapsed < seconds && actor.HolderRect != null)
            {
                elapsed += Time.unscaledDeltaTime;
                actor.HolderRect.position = Vector3.Lerp(start, end, Mathf.Clamp01(elapsed / seconds));
                yield return null;
            }
            actor.HolderRect.position = dest.position;
            FitHolderToSprite(actor);
            ApplyFacing(actor, actor.Facing);
        }

        private static void DestroyActor(string key)
        {
            Actor actor;
            if (!Actors.TryGetValue(key, out actor) || actor == null)
                return;
            foreach (var pair in actor.Sprites)
            {
                if (pair.Value != null)
                    UnityEngine.Object.Destroy(pair.Value);
            }
            actor.Sprites.Clear();
            for (int i = 0; i < actor.Textures.Count; i++)
            {
                if (actor.Textures[i] != null)
                    UnityEngine.Object.Destroy(actor.Textures[i]);
            }
            actor.Textures.Clear();
            if (actor.Holder != null)
                UnityEngine.Object.Destroy(actor.Holder);
            actor.Holder = null;
            actor.Image = null;
            Actors.Remove(key);
        }
    }
}
