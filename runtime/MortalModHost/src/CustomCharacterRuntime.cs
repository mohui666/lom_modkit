using System;
using System.Collections;
using System.Collections.Generic;
using BepInEx.Logging;
using Fungus;
using HarmonyLib;
using Mortal.Story;
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
            public string Title;
            public float BodyScale = 1f;
            public UserContent Content;
            public float LayoutScale = 1f;
            public Vector2 CanvasSize;
            public string ArtFacing = "left";
            public string Portrait = "normal";
            public string Facing = "right";
            public string Slot;
            public GameObject Holder;
            public RectTransform HolderRect;
            public Image Image;
            public RectTransform ImageRect;
            public bool Visible;
            public bool Dimmed;
            public CharacterMoodPanel Mood;
            public Vector2 MoodAnchor;
            public Vector2 MoodLayoutAnchor;
            public Vector2 MoodOffset;
            public string MoodIcon;
            public string ShownMood;
            public float MoodFacing;
            public Coroutine FadeRoutine;
            public Coroutine MotionRoutine;
            public Coroutine ShockRoutine;
            public Coroutine DimRoutine;
            public Vector2 ShockOrigin;
            public bool Shocking;
            public readonly Dictionary<string, Sprite> Sprites = new Dictionary<string, Sprite>(StringComparer.Ordinal);
            public readonly List<Texture2D> Textures = new List<Texture2D>();
        }

        public static void Init(MonoBehaviour host)
        {
            _host = host;
        }

        public static void ClearAll()
        {
            ShowScales.Clear();
            RenderPriorityRuntime.Clear();
            CameraFilterRuntime.Clear();
            CharacterTitleRuntime.CustomSpeaker = null;
            AutoDialogueRuntime.Clear();
            CharacterPresentation.NarrativeLeft(false);
            CharacterDimming.Clear();
            CharacterLayoutRuntime.Clear();
            CharacterRotationRuntime.Clear();
            EffectRuntime.Clear();
            EffectPreviewCapture.Cancel();
            MoodControl.ClearOverrides();
            List<string> keys = new List<string>(Actors.Keys);
            for (int i = 0; i < keys.Count; i++)
                DestroyActor(keys[i]);
            Actors.Clear();
        }

        /// <summary>
        /// 隐藏全部舞台人物；省略时长时保留切章等调用的立即清理行为。
        /// </summary>
        public static void HideAllOnStage(float fadeDuration = 0f)
        {
            if (fadeDuration <= 0f)
                ClearAll();
            else
            {
                // Generated Lua waits for these fades, then invokes immediate
                // clearing before advancing to another story step.
                foreach (string raw in new List<string>(Actors.Keys))
                    Hide(raw, fadeDuration);
                MoodControl.ClearOverrides();
            }
            try
            {
                StoryStageController[] stages = Resources.FindObjectsOfTypeAll<StoryStageController>();
                if (stages == null)
                    return;
                for (int s = 0; s < stages.Length; s++)
                {
                    StoryStageController stage = stages[s];
                    if (stage == null || stage.CharactersOnStage == null)
                        continue;
                    List<Character> onStage = new List<Character>(stage.CharactersOnStage);
                    for (int i = 0; i < onStage.Count; i++)
                    {
                        Character ch = onStage[i];
                        if (ch == null)
                            continue;
                        PortraitOptions opt = new PortraitOptions();
                        opt.character = ch;
                        opt.fadeDuration = fadeDuration;
                        opt.useDefaultSettings = false;
                        opt.waitUntilFinished = false;
                        try
                        {
                            stage.Hide(opt);
                        }
                        catch (Exception ex)
                        {
                            if (Log != null)
                                Log.LogWarning("隐藏官方角色失败：" + ex.Message);
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                if (Log != null)
                    Log.LogWarning("HideAllOnStage 失败：" + ex.Message);
            }
        }

        private static readonly Dictionary<string, float> ShowScales = new Dictionary<string, float>();
        internal static void PrepareShowScale(string raw, float percent) { ShowScales[raw] = percent / 100f; }

        public static bool Show(string raw, string portrait, string position, string facing, float fade, float move)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            float requestedScale;
            actor.LayoutScale = ShowScales.TryGetValue(raw, out requestedScale) ? requestedScale : 1f;
            ShowScales.Remove(raw);
            if (!ApplyPortrait(actor, portrait))
                return false;
            // A previous offset/move may still be yielding its final frame.
            // Re-entry owns the new position; an old routine must not overwrite it.
            StopRoutine(ref actor.MotionRoutine);
            StopRoutine(ref actor.FadeRoutine);
            Place(actor, position);
            ApplyFacing(actor, facing);
            actor.Holder.SetActive(true);
            actor.Visible = true;
            Focus(raw);
            if (_host != null && fade > 0f)
                actor.FadeRoutine = _host.StartCoroutine(FadeImage(actor.Image, 0f, 1f, fade));
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
            StopRoutine(ref actor.FadeRoutine);
            actor.Visible = false;
            actor.MoodIcon = null;
            actor.MoodOffset = Vector2.zero;
            if (actor.Mood != null) actor.Mood.Hide();
            actor.ShownMood = null;
            if (_host != null && fade > 0f && actor.Image != null)
            {
                actor.FadeRoutine = _host.StartCoroutine(FadeThenHide(actor, fade));
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
                StopRoutine(ref actor.FadeRoutine);
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
            StopRoutine(ref actor.MotionRoutine);
            if (_host != null && duration > 0f)
                actor.MotionRoutine = _host.StartCoroutine(MoveTo(actor, dest, duration));
            else
                FitHolderToSprite(actor, dest);
            return true;
        }

        internal static List<string> ActiveCharacterIds()
        {
            var result = new List<string>();
            foreach (var pair in Actors)
                if (pair.Value != null && pair.Value.Visible)
                    result.Add(pair.Key);
            result.Sort(StringComparer.Ordinal);
            return result;
        }

        internal static void SetMood(string raw, string icon, float x, float y)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw, out actor) || !actor.Visible)
                throw new InvalidOperationException("指定气泡的人物尚未登场：" + raw);
            EnsureMood(actor);
            actor.MoodOffset = new Vector2(x, y);
            actor.Mood.SetPosition(x * actor.BodyScale, y * actor.BodyScale);
            actor.MoodIcon = icon == "auto" ? null : icon;
            UpdateMoods();
        }

        internal static void ClearMoodOverrides()
        {
            foreach (var actor in Actors.Values)
            {
                actor.MoodIcon = null;
                actor.MoodOffset = Vector2.zero;
                actor.ShownMood = null;
                if (actor.Mood != null)
                {
                    actor.Mood.Hide();
                    actor.Mood.SetPosition(0f, 0f);
                }
            }
        }

        internal static Action BeginDialogueMood(string raw, string icon)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw, out actor) || !actor.Visible)
                throw new InvalidOperationException("指定气泡的人物尚未登场：" + raw);
            string previous = actor.MoodIcon;
            SetMood(raw, icon, actor.MoodOffset.x, actor.MoodOffset.y);
            // A scoped auto choice follows the portrait even when chapter auto bubbles are off.
            actor.MoodIcon = icon;
            UpdateMoods();
            return () =>
            {
                Actor current;
                if (!Actors.TryGetValue(raw, out current) || current != actor || !actor.Visible) return;
                actor.MoodIcon = previous;
                UpdateMoods();
            };
        }

        private static void EnsureMood(Actor actor)
        {
            if (actor.Mood != null) return;
            actor.Mood = MoodControl.CreatePanel(actor.Holder.transform);
            var rect = actor.Mood.GetComponent<RectTransform>();
            rect.anchorMin = actor.MoodLayoutAnchor;
            rect.anchorMax = actor.MoodLayoutAnchor;
            actor.Mood.SetPosition(0f, 0f);
            rect.localScale = Vector3.one * actor.BodyScale;
        }

        internal static GameObject FilterHolder(string raw)
        {
            Actor actor;
            return Actors.TryGetValue(raw, out actor) ? actor.Holder : null;
        }

        internal static void UpdateMoods()
        {
            Stage stage = Stage.GetActiveStage();
            RectTransform canvas = stage != null && stage.PortraitCanvas != null
                ? stage.PortraitCanvas.transform as RectTransform : null;
            foreach (var actor in Actors.Values)
            {
                if (canvas != null && actor.CanvasSize != canvas.rect.size)
                {
                    Vector2 anchored = actor.HolderRect.anchoredPosition;
                    FitHolderToSprite(actor);
                    actor.HolderRect.anchoredPosition = anchored;
                }
                string icon = actor.MoodIcon == "auto" ? actor.Portrait : actor.MoodIcon ?? (MoodControl.Disabled ? null : actor.Portrait);
                if (!actor.Visible || actor.Dimmed || actor.Image.color.a < 0.999f || icon == "hide") icon = null;
                if (icon == null || icon == "normal")
                {
                    if (actor.Mood != null && actor.ShownMood != null) actor.Mood.Hide();
                    actor.ShownMood = null;
                    continue;
                }
                EnsureMood(actor);
                actor.Mood.SetPosition(actor.MoodOffset.x * actor.BodyScale, actor.MoodOffset.y * actor.BodyScale);
                float facing = actor.HolderRect.localScale.x < 0f ? -1f : 1f;
                if (icon != actor.ShownMood || facing != actor.MoodFacing)
                {
                    actor.Mood.Show(icon, facing);
                    actor.ShownMood = icon;
                    actor.MoodFacing = facing;
                }
            }
        }

        /// <summary>按原版 MoveOffsetCoroutine 语义累加 anchoredPosition。</summary>
        public static bool Offset(string raw, float x, float y, float duration)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null
                || actor.HolderRect == null || !actor.Visible)
                return false;
            StopRoutine(ref actor.MotionRoutine);
            Vector2 target = actor.HolderRect.anchoredPosition + new Vector2(x, y);
            if (_host != null && duration > 0f)
                actor.MotionRoutine = _host.StartCoroutine(OffsetTo(actor, target, duration));
            else
                actor.HolderRect.anchoredPosition = target;
            return true;
        }

        internal static bool Layout(string raw, float x, float y, float scale, bool absolute, float duration)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor.HolderRect == null || !actor.Visible)
                return false;
            Vector2 target = actor.HolderRect.anchoredPosition + new Vector2(x, y);
            if (absolute)
            {
                RectTransform slot = FindPosition(actor.Slot);
                if (slot == null) throw new InvalidOperationException("找不到人物的原始站位：" + raw);
                target = slot.anchoredPosition + new Vector2(x, y);
            }
            if (scale != -1f) actor.LayoutScale = scale / 100f;
            ApplyFacing(actor, actor.Facing);
            Vector2 delta = target - actor.HolderRect.anchoredPosition;
            return Offset(raw, delta.x, delta.y, duration);
        }

        internal static bool ResetLayout(string raw)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor.HolderRect == null || !actor.Visible)
                return false;
            actor.LayoutScale = 1f;
            ApplyFacing(actor, actor.Facing);
            return true;
        }

        /// <summary>对立绘子层做衰减抖动，结束时精确恢复位置。</summary>
        public static bool Shock(string raw, float duration)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null
                || actor.ImageRect == null || !actor.Visible)
                return false;
            StopShock(actor);
            actor.ShockOrigin = actor.ImageRect.anchoredPosition;
            actor.Shocking = true;
            if (_host != null && duration > 0f)
                actor.ShockRoutine = _host.StartCoroutine(ShakeImage(actor, duration));
            else
                StopShock(actor);
            return true;
        }

        /// <summary>使用当前官方舞台的 DimColor / FadeDuration 压暗立绘。</summary>
        public static bool Dim(string raw, bool dimmed)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null || actor.Image == null)
                return false;
            actor.Dimmed = dimmed;
            Stage stage = Stage.GetActiveStage();
            Color target = dimmed && stage != null ? stage.DimColor : Color.white;
            target.a = actor.Image.color.a;
            float duration = stage != null ? stage.FadeDuration : 0f;
            StopRoutine(ref actor.DimRoutine);
            if (_host != null && duration > 0f)
                actor.DimRoutine = _host.StartCoroutine(FadeColor(actor, target, duration));
            else
                actor.Image.color = target;
            return true;
        }

        /// <summary>按原版 Rotate 语义旋转到绝对 Z 角度。</summary>
        public static bool Rotate(string raw, float angle, float duration)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw ?? "", out actor) || actor == null
                || actor.HolderRect == null || !actor.Visible)
                return false;
            CharacterRotationRuntime.Rotate(actor.HolderRect, 0f, angle, duration, false);
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
            RenderPriorityRuntime.TrackUi(actor.Holder, "character", raw);
            return true;
        }

        public static bool SetPortrait(string raw, string portrait)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            return ApplyPortrait(actor, portrait);
        }

        internal static Image TransitionPortrait(string raw, string portrait, bool reveal)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw, out actor) || !actor.Visible || actor.Image == null)
                throw new InvalidOperationException("换表情的人物尚未登场：" + raw);
            if (GetSprite(actor, portrait) == null)
                throw new InvalidOperationException("找不到人物表情：" + raw + "/" + portrait);
            if (reveal)
            {
                if (!ApplyPortrait(actor, portrait))
                    throw new InvalidOperationException("无法切换人物表情：" + raw + "/" + portrait);
            }
            return actor.Image;
        }

        public static bool SetSpeaker(string raw)
        {
            Actor actor;
            if (!TryGetOrCreate(raw, out actor))
                return false;
            CharacterDimming.BeginCustomDialogue(raw);
            CharacterTitleRuntime.CustomSpeaker = raw;
            try
            {
                SayDialog dialog = SayDialog.GetSayDialog();
                if (dialog != null)
                {
                    dialog.SetCharacterName(actor.Name, Color.white);
                    CharacterTitleRuntime.Apply(dialog, raw, actor.Title);
                    CharacterNameRuntime.Apply(dialog, raw);
                }
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogDebug("设置自定义角色发言名失败：" + ex.Message);
            }
            return true;
        }

        internal static void RefreshName(string raw, SayDialog dialog)
        {
            Actor actor;
            if (!Actors.TryGetValue(raw, out actor)) return;
            dialog.SetCharacterName(actor.Name, Color.white);
            CharacterTitleRuntime.Apply(dialog, raw, actor.Title);
            CharacterNameRuntime.Apply(dialog, raw);
        }

        internal static void RefreshTitle(string raw, SayDialog dialog)
        {
            Actor actor;
            if (Actors.TryGetValue(raw, out actor)) CharacterTitleRuntime.Apply(dialog, raw, actor.Title);
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

            float percent = content.Scale;
            if (float.IsNaN(percent) || float.IsInfinity(percent) || percent <= 0f)
                throw new InvalidOperationException("人物缩放必须是正有限数字：" + parsed.Raw);
            string artFacing = string.Equals(content.ArtFacing, "right", StringComparison.OrdinalIgnoreCase)
                ? "right"
                : "left";
            actor = new Actor
            {
                RawId = parsed.Raw,
                Name = string.IsNullOrEmpty(content.Name) ? parsed.LocalId : content.Name,
                Title = content.Title,
                BodyScale = percent / 100f,
                Content = content,
                ArtFacing = artFacing,
                MoodAnchor = new Vector2(content.MoodX / 100f, content.MoodY / 100f)
            };
            GameObject holder = new GameObject("lom_char_" + parsed.ContentId, typeof(RectTransform));
            holder.transform.SetParent(stage.PortraitCanvas.transform, false);
            actor.Holder = holder;
            actor.HolderRect = holder.GetComponent<RectTransform>();
            holder.SetActive(false);

            GameObject imageObj = new GameObject("portrait", typeof(RectTransform), typeof(CanvasRenderer), typeof(Image));
            imageObj.transform.SetParent(holder.transform, false);
            actor.Image = imageObj.GetComponent<Image>();
            actor.ImageRect = imageObj.GetComponent<RectTransform>();
            actor.Image.preserveAspect = true;
            actor.Image.raycastTarget = false;
            actor.Image.material = null;
            actor.Image.type = Image.Type.Simple;
            actor.Image.color = new Color(1f, 1f, 1f, 0f);
            actor.Image.enabled = false;
            RectTransform imageRect = actor.ImageRect;
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
            Vector3 position = actor.HolderRect.position;
            actor.Portrait = portrait;
            if (actor.Image != null)
            {
                actor.Image.sprite = sprite;
                actor.Image.enabled = true;
                Stage stage = Stage.GetActiveStage();
                Color color = actor.Dimmed && stage != null ? stage.DimColor : Color.white;
                color.a = 1f;
                actor.Image.color = color;
            }
            FitHolderToSprite(actor);
            // Dialogue also calls this method. Resize for the new portrait while
            // retaining the current (possibly offscreen) position of a visible actor.
            if (actor.Visible)
                actor.HolderRect.position = position;
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

            float canvasH = 1080f;
            float canvasW = 1920f;
            Stage stage = Stage.GetActiveStage();
            if (stage != null && stage.PortraitCanvas != null)
            {
                RectTransform canvasRect = stage.PortraitCanvas.transform as RectTransform;
                if (canvasRect != null && canvasRect.rect.height > 1f)
                {
                    canvasH = canvasRect.rect.height;
                    canvasW = canvasRect.rect.width;
                    actor.CanvasSize = canvasRect.rect.size;
                }
            }
            if (canvasH < 1f)
                canvasH = 1080f;

            // PortraitController.CreatePortraitObject / SetRectTransform 用完整站位矩形
            // 配合 preserveAspect。当前场景 M/L1/R1 均为 1920x1080。
            // 与原版采用同一 contain 规则，不能再按 56% 屏高缩小用户图。
            float boxH = canvasH;
            float boxW = canvasW;
            Vector2 pivot = new Vector2(0.5f, 0f);
            if (dest != null)
            {
                float slotH = Mathf.Abs(dest.rect.height);
                float slotW = Mathf.Abs(dest.rect.width);
                if (slotH > 1f && slotW > 1f)
                {
                    boxH = slotH;
                    boxW = slotW;
                    pivot = dest.pivot;
                }
            }

            string reference = actor.Content.PortraitReference;
            Sprite anchorSprite = !string.IsNullOrEmpty(reference) ? GetSprite(actor, reference) : sprite;
            if (anchorSprite != null) { sw = anchorSprite.rect.width; sh = anchorSprite.rect.height; }
            PortraitCalibration anchorLayout = actor.Content.PortraitLayout(reference ?? actor.Portrait);
            float anchorScale = anchorLayout.Scale / 100f;
            float contain = Mathf.Min(boxW / sw, boxH / sh) * anchorScale;
            actor.MoodLayoutAnchor = new Vector2(
                0.5f + anchorLayout.X / 1920f + (actor.MoodAnchor.x - 0.5f) * sw * contain / boxW,
                0.5f + (anchorScale - 1f) * 0.5f + anchorLayout.Y / 1080f + (actor.MoodAnchor.y - 0.5f) * sh * contain / boxH);
            if (actor.Mood != null)
            {
                RectTransform mood = actor.Mood.GetComponent<RectTransform>();
                mood.anchorMin = actor.MoodLayoutAnchor;
                mood.anchorMax = actor.MoodLayoutAnchor;
            }

            actor.HolderRect.anchorMin = dest != null ? dest.anchorMin : new Vector2(0.5f, 1f);
            actor.HolderRect.anchorMax = dest != null ? dest.anchorMax : new Vector2(0.5f, 1f);
            actor.HolderRect.pivot = pivot;
            actor.HolderRect.sizeDelta = new Vector2(boxW * actor.BodyScale, boxH * actor.BodyScale);
            PortraitCalibration calibration = actor.Content.PortraitLayout(actor.Portrait);
            float portraitScale = calibration.Scale / 100f;
            actor.ImageRect.localScale = new Vector3(portraitScale, portraitScale, 1f);
            actor.ImageRect.anchoredPosition = new Vector2(
                actor.HolderRect.rect.width * calibration.X / 1920f,
                actor.HolderRect.rect.height * (calibration.Y / 1080f + (portraitScale - 1f) * 0.5f));
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
            // 原版立绘默认朝左。原图朝左时，节点 facing=left 不翻、right 才翻。
            bool wantLeft = actor.Facing == "left";
            bool artLeft = !string.Equals(actor.ArtFacing, "right", StringComparison.OrdinalIgnoreCase);
            float sx = wantLeft == artLeft ? 1f : -1f;
            actor.HolderRect.localScale = new Vector3(sx * actor.LayoutScale, actor.LayoutScale, 1f);
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
            actor.MotionRoutine = null;
        }

        private static IEnumerator OffsetTo(Actor actor, Vector2 target, float seconds)
        {
            if (actor.HolderRect == null)
                yield break;
            Vector2 start = actor.HolderRect.anchoredPosition;
            float elapsed = 0f;
            while (elapsed < seconds && actor.HolderRect != null)
            {
                elapsed += Time.unscaledDeltaTime;
                actor.HolderRect.anchoredPosition = Vector2.Lerp(
                    start, target, Mathf.Clamp01(elapsed / seconds));
                yield return null;
            }
            if (actor.HolderRect != null)
                actor.HolderRect.anchoredPosition = target;
            actor.MotionRoutine = null;
        }

        private static IEnumerator ShakeImage(Actor actor, float seconds)
        {
            float elapsed = 0f;
            while (elapsed < seconds && actor.ImageRect != null)
            {
                elapsed += Time.unscaledDeltaTime;
                float strength = 12f * (1f - Mathf.Clamp01(elapsed / seconds));
                float phase = elapsed * 70f;
                actor.ImageRect.anchoredPosition = actor.ShockOrigin
                    + new Vector2(Mathf.Sin(phase) * strength, Mathf.Cos(phase * 0.73f) * strength * 0.45f);
                yield return null;
            }
            if (actor.ImageRect != null)
                actor.ImageRect.anchoredPosition = actor.ShockOrigin;
            actor.Shocking = false;
            actor.ShockRoutine = null;
        }

        private static IEnumerator FadeColor(Actor actor, Color target, float seconds)
        {
            if (actor.Image == null)
                yield break;
            Color start = actor.Image.color;
            float elapsed = 0f;
            while (elapsed < seconds && actor.Image != null)
            {
                elapsed += Time.unscaledDeltaTime;
                actor.Image.color = Color.Lerp(start, target, Mathf.Clamp01(elapsed / seconds));
                yield return null;
            }
            if (actor.Image != null)
                actor.Image.color = target;
            actor.DimRoutine = null;
        }

        private static void StopRoutine(ref Coroutine routine)
        {
            if (routine != null && _host != null)
                _host.StopCoroutine(routine);
            routine = null;
        }

        private static void StopShock(Actor actor)
        {
            if (actor == null)
                return;
            StopRoutine(ref actor.ShockRoutine);
            if (actor.Shocking && actor.ImageRect != null)
                actor.ImageRect.anchoredPosition = actor.ShockOrigin;
            actor.Shocking = false;
        }

        private static void DestroyActor(string key)
        {
            Actor actor;
            if (!Actors.TryGetValue(key, out actor) || actor == null)
                return;
            StopRoutine(ref actor.MotionRoutine);
            StopShock(actor);
            StopRoutine(ref actor.DimRoutine);
            CharacterRotationRuntime.Forget(actor.HolderRect);
            StopRoutine(ref actor.FadeRoutine);
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
            actor.ImageRect = null;
            Actors.Remove(key);
        }
    }
}
