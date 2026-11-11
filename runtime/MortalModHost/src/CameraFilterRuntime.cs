using System;
using System.Collections.Generic;
using Fungus;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.UI;

namespace MortalModHost
{
    // Keep the native holders in place: Fungus movement uses their world coordinates.
    // Only their graphics are mirrored onto the existing UI camera after stage
    // post-processing; that camera's Volume mask excludes the stage filter layer.
    internal static class CameraFilterRuntime
    {
        private static readonly Dictionary<string, HashSet<string>> Exclusions = new Dictionary<string, HashSet<string>>();
        private static readonly HashSet<string> ManagedKeys = new HashSet<string>(StringComparer.Ordinal);
        private static readonly Dictionary<string, float> Releasing = new Dictionary<string, float>();
        private static readonly Dictionary<string, Transform> Holders = new Dictionary<string, Transform>();
        private static readonly Dictionary<Image, Mirror> Mirrors = new Dictionary<Image, Mirror>();
        private static Canvas _canvas;
        private static CameraFilterRenderer _renderer;
        private static readonly Vector3[] Corners = new Vector3[4];
        private static readonly Vector2[] LocalCorners = new Vector2[4];
        private static readonly HashSet<string> Selected = new HashSet<string>(StringComparer.Ordinal);
        private static readonly List<string> ExpiredKeys = new List<string>();
        private static readonly List<Image> Sources = new List<Image>();
        private static readonly List<Image> RemovedImages = new List<Image>();
        private static readonly List<CanvasGroup> Groups = new List<CanvasGroup>();

        private sealed class Mirror
        {
            internal Image Image;
            internal string Character;
            internal bool Enabled;
        }

        internal static void Set(string key, bool active, float duration, IList<string> characters)
        {
            if (ModOverlay.CurrentPackage == null) throw new InvalidOperationException("滤镜扩展只能用于 MOD 演出");
            StoryCamera camera = UnityEngine.Object.FindObjectOfType<StoryCamera>();
            if (camera == null) throw new InvalidOperationException("当前场景没有剧情滤镜相机");
            if (float.IsNaN(duration) || float.IsInfinity(duration) || duration < 0f)
                throw new ArgumentException("滤镜渐变时长必须是非负有限数值");
            var items = Traverse.Create(camera).Field("_postProcessingVolume").GetValue<PostProcessingVolumeItem[]>();
            PostProcessingVolumeItem item = Array.Find(items, value => value.Key == key);
            if (item == null || item.Volume == null) throw new ArgumentException("当前场景没有滤镜：" + key);
            ManagedKeys.Add(key);
            if (active)
            {
                if (characters.Count > 0) EnsureCanvas();
                Exclusions[key] = new HashSet<string>(characters, StringComparer.Ordinal);
                Releasing.Remove(key);
            }
            else if (duration > 0f)
                Releasing[key] = Time.time + duration;
            else
            {
                Exclusions.Remove(key);
                Releasing.Remove(key);
            }
            if (duration > 0f) camera.FadeVolume(key, active ? 1 : 0, duration);
            else
            {
                camera.ActiveVolume(key, active ? 1 : 0);
            }
            Tick();
        }

        internal static void OnImmediate(StoryCamera camera, string key, int active)
        {
            if (ModOverlay.CurrentPackage == null || !ManagedKeys.Contains(key)) return;
            var items = Traverse.Create(camera).Field("_postProcessingVolume").GetValue<PostProcessingVolumeItem[]>();
            var item = Array.Find(items, value => value.Key == key);
            if (item != null && item.Volume != null)
            {
                // Native FadeVolume kills an older weight tween, including a
                // fade interrupted by an ordinary legacy on/off node.
                camera.FadeVolume(key, active, 0f);
                item.Volume.weight = 1f;
            }
            Releasing.Remove(key);
            if (active != 1) Exclusions.Remove(key);
        }

        private static void EnsureCanvas()
        {
            if (_canvas != null) { _renderer.enabled = true; return; }
            Canvas reference = null;
            foreach (var dialog in Resources.FindObjectsOfTypeAll<CharacterSayDialog>())
            {
                if (dialog == null || !dialog.gameObject.scene.IsValid() || !dialog.gameObject.scene.isLoaded) continue;
                // SayDialog deactivates its GameObject between lines. Its
                // serialized canvas and UI-camera reference remain valid.
                Canvas candidate = dialog.GetComponentInParent<Canvas>(true);
                if (candidate != null && candidate.worldCamera != null) { reference = candidate; break; }
            }
            if (reference == null) throw new InvalidOperationException("当前场景没有可用于原彩人物的对白相机画布");
            GameObject root = new GameObject("LoMModkit Unfiltered Characters", typeof(RectTransform), typeof(Canvas), typeof(CameraFilterRenderer));
            root.layer = reference.gameObject.layer;
            _canvas = root.GetComponent<Canvas>();
            _renderer = root.GetComponent<CameraFilterRenderer>();
            _canvas.renderMode = RenderMode.ScreenSpaceCamera;
            _canvas.worldCamera = reference.worldCamera;
            _canvas.planeDistance = reference.planeDistance;
            _canvas.sortingLayerID = reference.sortingLayerID;
            _canvas.sortingOrder = reference.sortingOrder - 1;
            _canvas.targetDisplay = reference.targetDisplay;
        }

        internal static void Tick()
        {
            ExpiredKeys.Clear();
            foreach (var pair in Releasing)
                if (Time.time >= pair.Value) ExpiredKeys.Add(pair.Key);
            foreach (string key in ExpiredKeys) { Exclusions.Remove(key); Releasing.Remove(key); }
            Selected.Clear();
            foreach (var set in Exclusions.Values) Selected.UnionWith(set);
            if (Selected.Count == 0 && Mirrors.Count == 0)
            {
                if (_renderer != null && Releasing.Count == 0) _renderer.enabled = false;
                return;
            }
            Stage stage = Stage.GetActiveStage();
            if (stage != null && stage.CharactersOnStage != null)
                foreach (var character in stage.CharactersOnStage)
                {
                    var actor = character as StoryCharacterController;
                    if (actor != null && actor.Data != null && actor.State != null && actor.State.holder != null)
                        Holders[actor.Data.Id] = actor.State.holder;
                }
            foreach (string id in Selected)
            {
                Transform holder;
                if (id.StartsWith("user:", StringComparison.Ordinal))
                {
                    GameObject custom = CustomCharacterRuntime.FilterHolder(id);
                    holder = custom == null ? null : custom.transform;
                }
                else Holders.TryGetValue(id, out holder);
                if (holder == null || _canvas == null) continue;
                Sources.Clear();
                holder.GetComponentsInChildren<Image>(true, Sources);
                foreach (Image source in Sources)
                {
                    Mirror mirror;
                    if (!Mirrors.TryGetValue(source, out mirror))
                    {
                        var obj = new GameObject("Original color " + id, typeof(RectTransform), typeof(CanvasRenderer), typeof(Image));
                        obj.layer = _canvas.gameObject.layer;
                        obj.transform.SetParent(_canvas.transform, false);
                        mirror = new Mirror { Image = obj.GetComponent<Image>(), Character = id, Enabled = source.enabled };
                        mirror.Image.raycastTarget = false;
                        Mirrors.Add(source, mirror);
                    }
                    // Custom SetPortrait enables its source Image again. This
                    // layer owns rendering until the exclusion is removed.
                    source.enabled = false;
                    Copy(source, mirror);
                }
            }
            RemovedImages.Clear();
            foreach (var pair in Mirrors)
                if (pair.Key == null || !Selected.Contains(pair.Value.Character))
                {
                    if (pair.Key != null) pair.Key.enabled = pair.Value.Enabled;
                    if (pair.Value.Image != null) UnityEngine.Object.Destroy(pair.Value.Image.gameObject);
                    RemovedImages.Add(pair.Key);
                }
            foreach (Image source in RemovedImages) Mirrors.Remove(source);
        }

        private static void Copy(Image source, Mirror mirror)
        {
            Image target = mirror.Image;
            if (target == null) return;
            Canvas origin = source.canvas;
            target.gameObject.SetActive(source.gameObject.activeInHierarchy && mirror.Enabled
                && origin != null && origin.isActiveAndEnabled);
            if (!target.gameObject.activeSelf) return;
            target.sprite = source.sprite;
            target.material = source.material;
            target.type = source.type;
            target.preserveAspect = source.preserveAspect;
            target.fillAmount = source.fillAmount;
            target.fillMethod = source.fillMethod;
            target.fillOrigin = source.fillOrigin;
            target.fillClockwise = source.fillClockwise;
            Color color = source.color;
            Groups.Clear();
            source.GetComponentsInParent<CanvasGroup>(false, Groups);
            foreach (var group in Groups)
            {
                color.a *= group.alpha;
                if (group.ignoreParentGroups) break;
            }
            target.color = color;
            source.rectTransform.GetWorldCorners(Corners);
            Camera fromCamera = origin != null && origin.renderMode != RenderMode.ScreenSpaceOverlay ? origin.worldCamera : null;
            for (int i = 0; i < 4; i++)
                RectTransformUtility.ScreenPointToLocalPointInRectangle((RectTransform)_canvas.transform,
                    RectTransformUtility.WorldToScreenPoint(fromCamera, Corners[i]), _canvas.worldCamera, out LocalCorners[i]);
            Vector2 x = LocalCorners[3] - LocalCorners[0], y = LocalCorners[1] - LocalCorners[0];
            RectTransform rect = target.rectTransform;
            rect.anchorMin = rect.anchorMax = rect.pivot = new Vector2(0.5f, 0.5f);
            rect.anchoredPosition = (LocalCorners[0] + LocalCorners[2]) * 0.5f;
            rect.sizeDelta = new Vector2(x.magnitude, y.magnitude);
            rect.localRotation = Quaternion.Euler(0f, 0f, Mathf.Atan2(x.y, x.x) * Mathf.Rad2Deg);
            rect.localScale = new Vector3(1f, x.x * y.y - x.y * y.x < 0f ? -1f : 1f, 1f);
        }

        internal static void Clear()
        {
            foreach (var pair in Mirrors)
                if (pair.Key != null) pair.Key.enabled = pair.Value.Enabled;
            Mirrors.Clear(); Holders.Clear(); Exclusions.Clear(); Releasing.Clear(); ManagedKeys.Clear();
            Selected.Clear(); ExpiredKeys.Clear(); Sources.Clear(); RemovedImages.Clear(); Groups.Clear();
            if (_canvas != null) UnityEngine.Object.Destroy(_canvas.gameObject);
            _canvas = null;
            _renderer = null;
        }
    }

    internal sealed class CameraFilterRenderer : MonoBehaviour
    {
        private void LateUpdate() { CameraFilterRuntime.Tick(); }
    }

    [HarmonyPatch(typeof(StoryCamera), "ActiveVolume", new[] { typeof(string), typeof(int) })]
    internal static class CameraFilterImmediatePatch
    {
        private static void Prefix(StoryCamera __instance, string key, int active) { CameraFilterRuntime.OnImmediate(__instance, key, active); }
    }
}
