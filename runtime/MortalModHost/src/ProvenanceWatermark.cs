using System;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// Host-owned algorithm-v1 provenance carrier. It is intentionally internal and
    /// has no Lua registration: only the trusted MOD playback boundary may enable it.
    /// This is a best-effort screenshot marker, not a signature or DRM mechanism.
    /// </summary>
    internal static class ProvenanceWatermark
    {
        private const int SortingOrder = 32767;

        private static string _modId;
        private static string _fingerprint;
        private static string _rootName;
        private static string _imageName;
        private static string _textureName;
        private static byte[] _tileRgba;
        private static GameObject _root;
        private static Canvas _canvas;
        private static RawImage _image;
        private static Texture2D _texture;

        internal static bool Active { get; private set; }
        internal static string LastError { get; private set; }

        internal static void Enable(string modId, string fingerprint)
        {
            byte[] packet = ProvenanceWatermarkProtocol.Encode(
                modId, ProvenanceWatermarkCodec.AlgorithmVersion);
            byte[] tile = ProvenanceWatermarkCodec.BuildTileRgba(packet);

            if (!string.Equals(_modId, modId, StringComparison.Ordinal)
                || !string.Equals(_fingerprint, fingerprint, StringComparison.OrdinalIgnoreCase))
            {
                DestroyVisuals();
                _modId = modId;
                _fingerprint = fingerprint;
                _rootName = DisclosureIntegrity.ProtectedObjectName(
                    "provenance-root", fingerprint);
                _imageName = DisclosureIntegrity.ProtectedObjectName(
                    "provenance-carrier", fingerprint);
                _textureName = DisclosureIntegrity.ProtectedObjectName(
                    "provenance-texture", fingerprint);
                _tileRgba = tile;
            }
            else if (_tileRgba == null)
            {
                _tileRgba = tile;
            }

            Active = true;
            LastError = null;
            if (!Maintain())
                throw new InvalidOperationException(
                    "无法建立来源水印载体：" + (LastError ?? "未知错误"));
        }

        internal static bool Maintain()
        {
            if (!Active) return true;
            try
            {
                if (_tileRgba == null || _tileRgba.Length
                    != ProvenanceWatermarkCodec.TileWidth
                        * ProvenanceWatermarkCodec.TileHeight * 4)
                    throw new InvalidOperationException("水印像素载荷缺失");

                EnsureTexture();
                EnsureOverlay();
                RepairOverlay();
                LastError = null;
                return true;
            }
            catch (Exception ex)
            {
                LastError = ex.Message;
                return false;
            }
        }

        internal static void Disable()
        {
            Active = false;
            LastError = null;
            _modId = null;
            _fingerprint = null;
            _rootName = null;
            _imageName = null;
            _textureName = null;
            _tileRgba = null;
            DestroyVisuals();
        }

        private static void EnsureTexture()
        {
            if (_texture != null
                && _texture.width == ProvenanceWatermarkCodec.TileWidth
                && _texture.height == ProvenanceWatermarkCodec.TileHeight)
            {
                _texture.wrapMode = TextureWrapMode.Repeat;
                _texture.filterMode = FilterMode.Bilinear;
                return;
            }

            if (_texture != null) UnityEngine.Object.Destroy(_texture);
            _texture = new Texture2D(
                ProvenanceWatermarkCodec.TileWidth,
                ProvenanceWatermarkCodec.TileHeight,
                TextureFormat.RGBA32,
                false,
                true);
            _texture.name = _textureName;
            _texture.hideFlags = HideFlags.DontSave;
            _texture.wrapMode = TextureWrapMode.Repeat;
            _texture.filterMode = FilterMode.Bilinear;
            _texture.LoadRawTextureData(_tileRgba);
            // Keep the native texture immutable through ordinary scripting APIs.
            _texture.Apply(false, true);
        }

        private static void EnsureOverlay()
        {
            if (_root == null)
            {
                _root = new GameObject(_rootName, typeof(RectTransform), typeof(Canvas),
                    typeof(CanvasGroup));
                UnityEngine.Object.DontDestroyOnLoad(_root);
            }
            _canvas = _root.GetComponent<Canvas>();
            if (_canvas == null) _canvas = _root.AddComponent<Canvas>();

            if (_image == null || _image.transform.parent != _root.transform)
            {
                Transform existing = _root.transform.Find(_imageName);
                if (existing != null) _image = existing.GetComponent<RawImage>();
                if (_image == null)
                {
                    var imageObject = new GameObject(
                        _imageName, typeof(RectTransform), typeof(CanvasRenderer),
                        typeof(RawImage), typeof(CanvasGroup));
                    imageObject.transform.SetParent(_root.transform, false);
                    _image = imageObject.GetComponent<RawImage>();
                }
            }
        }

        private static void RepairOverlay()
        {
            _root.name = _rootName;
            _root.SetActive(true);
            _root.transform.SetParent(null, false);
            _root.transform.localPosition = Vector3.zero;
            _root.transform.localRotation = Quaternion.identity;
            _root.transform.localScale = Vector3.one;
            _root.transform.SetAsLastSibling();

            RectTransform rootRect = _root.GetComponent<RectTransform>();
            rootRect.anchorMin = Vector2.zero;
            rootRect.anchorMax = Vector2.one;
            rootRect.pivot = new Vector2(0.5f, 0.5f);
            rootRect.offsetMin = Vector2.zero;
            rootRect.offsetMax = Vector2.zero;

            _canvas.enabled = true;
            _canvas.renderMode = RenderMode.ScreenSpaceOverlay;
            _canvas.overrideSorting = true;
            _canvas.sortingLayerID = 0;
            _canvas.sortingOrder = SortingOrder;
            _canvas.targetDisplay = 0;
            _canvas.worldCamera = null;
            _canvas.pixelPerfect = false;

            RepairCanvasGroups(_root);

            GameObject imageObject = _image.gameObject;
            imageObject.name = _imageName;
            imageObject.SetActive(true);
            _image.transform.SetParent(_root.transform, false);
            _image.transform.SetAsLastSibling();
            RectTransform imageRect = _image.rectTransform;
            imageRect.anchorMin = Vector2.zero;
            imageRect.anchorMax = Vector2.one;
            imageRect.pivot = new Vector2(0.5f, 0.5f);
            imageRect.offsetMin = Vector2.zero;
            imageRect.offsetMax = Vector2.zero;
            imageRect.localRotation = Quaternion.identity;
            imageRect.localScale = Vector3.one;

            RepairCanvasGroups(imageObject);
            _image.enabled = true;
            _image.raycastTarget = false;
            _image.maskable = false;
            _image.material = null;
            _image.texture = _texture;
            _image.color = Color.white;
            _image.uvRect = new Rect(
                0f,
                0f,
                Mathf.Max(1f, Screen.width / (float)ProvenanceWatermarkCodec.TileWidth),
                Mathf.Max(1f, Screen.height / (float)ProvenanceWatermarkCodec.TileHeight));
            _image.canvasRenderer.SetAlpha(1f);
            _image.canvasRenderer.SetColor(Color.white);
            _image.SetAllDirty();
        }

        private static void RepairCanvasGroups(GameObject owner)
        {
            CanvasGroup[] groups = owner.GetComponents<CanvasGroup>();
            if (groups.Length == 0) groups = new[] { owner.AddComponent<CanvasGroup>() };
            for (int i = 0; i < groups.Length; i++)
            {
                groups[i].alpha = 1f;
                groups[i].interactable = false;
                groups[i].blocksRaycasts = false;
                groups[i].ignoreParentGroups = true;
                groups[i].enabled = true;
            }
        }

        private static void DestroyVisuals()
        {
            if (_root != null) UnityEngine.Object.Destroy(_root);
            if (_texture != null) UnityEngine.Object.Destroy(_texture);
            _root = null;
            _canvas = null;
            _image = null;
            _texture = null;
        }
    }
}
