using System;
using Fungus;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    internal static class CharacterPresentation
    {
        private static Text _narrativeText;
        private static TextAnchor _originalAlignment;
        private static Vector2 _originalAnchorMin, _originalAnchorMax;
        private static Vector2 _originalOffsetMin, _originalOffsetMax;

        // Only the current line overrides the live dialog component.
        internal static void NarrativeLeft(bool enabled)
        {
            if (_narrativeText != null)
            {
                _narrativeText.alignment = _originalAlignment;
                RectTransform previous = _narrativeText.rectTransform;
                previous.anchorMin = _originalAnchorMin;
                previous.anchorMax = _originalAnchorMax;
                previous.offsetMin = _originalOffsetMin;
                previous.offsetMax = _originalOffsetMax;
            }
            _narrativeText = null;
            if (!enabled) return;
            SayDialog dialog = SayDialog.GetSayDialog();
            Text text = dialog != null ? dialog.StoryTextObject : null;
            if (text == null) throw new InvalidOperationException("当前旁白缺少正文 Text 组件");
            _narrativeText = text;
            _originalAlignment = text.alignment;
            RectTransform rect = text.rectTransform;
            _originalAnchorMin = rect.anchorMin;
            _originalAnchorMax = rect.anchorMax;
            _originalOffsetMin = rect.offsetMin;
            _originalOffsetMax = rect.offsetMax;
            float margin = ((RectTransform)rect.parent).rect.width * 0.05f;
            rect.anchorMin = new Vector2(0f, _originalAnchorMin.y);
            rect.anchorMax = new Vector2(1f, _originalAnchorMax.y);
            rect.offsetMin = new Vector2(margin, _originalOffsetMin.y);
            rect.offsetMax = new Vector2(-margin, _originalOffsetMax.y);
            text.alignment = (TextAnchor)((int)text.alignment / 3 * 3);
        }

        // Lua waits between the two phases. No stage Show/Hide call: those
        // reset the holder transform, discarding author offsets and rotation.
        internal static void PortraitTransition(string id, string portrait, float seconds, bool reveal)
        {
            Image image;
            if (id.StartsWith("user:", StringComparison.Ordinal))
            {
                image = CustomCharacterRuntime.TransitionPortrait(id, portrait, reveal);
            }
            else
            {
                CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
                Character character = placeholder != null ? placeholder.Get(id) : null;
                if (character == null || !character.State.onScreen || character.State.portraitImage == null)
                    throw new InvalidOperationException("换表情的人物尚未登场：" + id);
                placeholder.LoadCharacterPortrait(id, portrait);
                Sprite sprite = character.GetPortrait(placeholder.GetPortrait(id, portrait));
                Image next = character.State.allPortraits.Find(item => item != null && item.sprite == sprite);
                if (sprite == null || next == null)
                    throw new InvalidOperationException("找不到人物表情：" + id + "/" + portrait);
                image = character.State.portraitImage;
                StoryCharacterController controller = character as StoryCharacterController;
                if (!reveal && controller != null && controller.Mood != null) controller.Mood.Hide();
                if (reveal)
                {
                    Color color = image.color;
                    Stage stage = Stage.GetActiveStage();
                    if (stage != null) color = character.State.dimmed ? stage.DimColor : Color.white;
                    if (image != next) image.gameObject.SetActive(false);
                    character.State.SetPortraitImageBySprite(sprite);
                    image = character.State.portraitImage;
                    image.color = color;
                    image.gameObject.SetActive(true);
                }
            }
            // Replace alpha only; preserve RGB dimming and holder motion.
            foreach (LTDescr tween in LeanTween.descriptions(image.gameObject))
                if (tween.type == TweenAction.CANVAS_ALPHA) LeanTween.cancel(tween.uniqueId);
            Color current = image.color;
            if (reveal) current.a = 0f;
            if (seconds <= 0f) current.a = reveal ? 1f : 0f;
            image.color = current;
            if (seconds > 0f)
                LeanTween.alpha(image.rectTransform, reveal ? 1f : 0f, seconds).setRecursive(false);
        }
    }
}
