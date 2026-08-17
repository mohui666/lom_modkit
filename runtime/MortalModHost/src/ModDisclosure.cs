using System;
using System.Collections;
using System.Collections.Generic;
using BepInEx.Logging;
using Fungus;
using Mortal.Core;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// 与 BepInEx Plugin GameObject 独立的披露守护。即使恶意 Lua 销毁宿主对象，
    /// 仍会在 Update / LateUpdate / IMGUI 继续自愈或 fail-closed。
    /// </summary>
    internal sealed class ModDisclosureGuardian : MonoBehaviour
    {
        private void Update()
        {
            ModDisclosure.MaintainFailClosed();
        }

        private void LateUpdate()
        {
            ModDisclosure.MaintainFailClosed();
        }

        private void OnGUI()
        {
            if (!ModDisclosure.DrawFailureGuard())
                ModDisclosure.DrawPersistentImGuiStamp();
        }
    }

    /// <summary>
    /// Host 强制的玩家制作内容披露。来源状态由 Host 的脚本注册表决定，MOD Lua 与 cfg
    /// 都没有关闭、改字、改透明度的入口。
    ///
    /// 同时覆盖屏幕边缘、对白框和关键结果卡；每帧校验并修复对象。任何必须标记无法
    /// 创建或修复时进入故障态，由 Plugin 立即中止本次演出并返回官方 Free 场景。
    /// </summary>
    internal static class ModDisclosure
    {
        private const string PrimaryObjectName = "primary";
        private const string DetailObjectName = "detail";
        private const string AccentObjectName = "accent";
        private const string DialogLabelObjectName = "dialog-label";
        private const string GuardianObjectName = "lom_disclosure_guardian";
        private const int EdgeMaxWidth = 480;
        private const int SurfaceMaxWidth = 430;
        private const int TopSortingOrder = 32767;

        private sealed class PanelBinding
        {
            public Transform Parent;
            public GameObject Chip;
            public Vector2 Anchor = Vector2.one;
            public Vector2 Pivot = Vector2.one;
            public Vector2 Offset = new Vector2(-18f, -14f);
        }

        private sealed class DialogBinding
        {
            public SayDialog Host;
            public RectTransform Parent;
            public GameObject Watermark;
        }

        internal static ManualLogSource Log;

        internal static bool Active { get; private set; }
        internal static string ModId { get; private set; }
        internal static string PackageFingerprint { get; private set; }
        internal static string FailureReason { get; private set; }

        private static string _safeName;
        private static string _safeAuthor;
        private static string _shortFingerprint;
        private static string _lastPrimary;
        private static string _lastDetail;
        private static string _dialogWatermarkName;
        private static byte[] _sessionSeal;

        private static GameObject _edgeRoot;
        private static GameObject _guardianRoot;
        private static GameObject _edgeChip;
        private static readonly List<DialogBinding> DialogBindings = new List<DialogBinding>();
        private static readonly List<PanelBinding> PanelBindings = new List<PanelBinding>();
        private static Sprite _whiteSprite;
        private static Font _osFont;
        private static bool _insideRenderGuard;
        private static bool _developmentHotReloadPending;

        /// <summary>
        /// 事务式开启：先验证 Host 计算的包指纹，再确保屏幕常驻标记可渲染。
        /// 任一步失败都会清理半成品并抛出，由 LuaManagerPatch 阻止脚本启动。
        /// </summary>
        internal static void Enable(ModPackage package)
        {
            if (package == null)
                throw new InvalidOperationException("mod 包身份缺失");
            if (string.IsNullOrEmpty(package.Id))
                throw new InvalidOperationException("mod 包没有有效的 Host 身份 ID");
            if (!ModDisclosurePolicy.IsValidPackageFingerprint(package.PackageFingerprint))
                throw new InvalidOperationException("mod 包没有有效的 Host SHA-256 指纹");

            bool samePackage = Active
                && string.Equals(ModId, package.Id, StringComparison.Ordinal)
                && string.Equals(PackageFingerprint, package.PackageFingerprint, StringComparison.OrdinalIgnoreCase);
            bool developmentReplacement = Active
                && _developmentHotReloadPending
                && ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    Active, RuntimeTrace.Active, ModId, package);
            if (Active && !samePackage && !developmentReplacement)
                throw new InvalidOperationException(
                    "活动中的 MOD 来源会话试图切换到另一包；为防止跨包冒名，已拒绝嵌套演出");
            if (Active && !string.IsNullOrEmpty(FailureReason))
                throw new InvalidOperationException("强制披露已进入故障态：" + FailureReason);

            if (!samePackage || developmentReplacement)
            {
                // 先完成所有可能失败的身份派生，再切换活动状态，避免留下
                // Active=true 但尚无守护面的半初始化窗口。
                string nextModId = package.Id;
                string nextFingerprint = package.PackageFingerprint.ToUpperInvariant();
                string nextSafeName = ModDisclosurePolicy.SafePackageName(package);
                string nextSafeAuthor = ModDisclosurePolicy.SafePackageAuthor(package);
                string nextShortFingerprint = ModDisclosurePolicy.ShortFingerprint(package.PackageFingerprint);
                string nextWatermarkName = DisclosureIntegrity.ProtectedObjectName(
                    "dialog-watermark", nextFingerprint);
                byte[] nextSessionSeal = DisclosureIntegrity.CreateSessionSeal(
                    nextModId, nextFingerprint);

                ClearVisuals();
                Active = true;
                ModId = nextModId;
                PackageFingerprint = nextFingerprint;
                _safeName = nextSafeName;
                _safeAuthor = nextSafeAuthor;
                _shortFingerprint = nextShortFingerprint;
                _dialogWatermarkName = nextWatermarkName;
                _sessionSeal = nextSessionSeal;
                FailureReason = null;
                _lastPrimary = null;
                _lastDetail = null;
            }
            _developmentHotReloadPending = false;

            try
            {
                SubscribeTrustedBoundaryEvents();
                EnsureSessionIntegrity();
                EnsureEdgeOverlay();
                RefreshLabels(true);
                ProvenanceWatermark.Enable(ModId, PackageFingerprint);
                if (!IsChipStructurallyValid(_edgeChip))
                    throw new InvalidOperationException("屏幕常驻标记未成功创建");
                if (!ProvenanceWatermark.Maintain())
                    throw new InvalidOperationException(
                        "来源水印未成功创建：" + ProvenanceWatermark.LastError);
                Canvas.willRenderCanvases -= BeforeCanvasRender;
                Canvas.willRenderCanvases += BeforeCanvasRender;
            }
            catch (Exception ex)
            {
                // 即使初次 UI 创建失败也保持来源会话与包身份，让 Plugin 的独立 IMGUI
                // 安全遮罩接管，直到真正抵达 Title / Free；不能在这里 fail-open 清掉 Active。
                ClearVisuals();
                try { EnsureGuardian(); }
                catch { }
                ReportMandatorySurfaceFailure("强制披露初始化失败：" + ex.Message);
                throw new InvalidOperationException("无法建立强制玩家内容披露，已拒绝播放 MOD", ex);
            }

            Log?.LogInfo("已开启玩家制作内容披露（" + ModId + "，SHA-256 " + PackageFingerprint + "）");
        }

        internal static void Disable()
        {
            Canvas.willRenderCanvases -= BeforeCanvasRender;
            Camera.onPreCull -= BeforeCameraRender;
            SceneManager.sceneLoaded -= OnSceneLoaded;
            SceneManager.activeSceneChanged -= OnActiveSceneChanged;
            ProvenanceWatermark.Disable();
            ClearVisuals();
            Active = false;
            ModId = null;
            PackageFingerprint = null;
            FailureReason = null;
            _safeName = null;
            _safeAuthor = null;
            _shortFingerprint = null;
            _lastPrimary = null;
            _lastDetail = null;
            _dialogWatermarkName = null;
            _sessionSeal = null;
            _insideRenderGuard = false;
            _developmentHotReloadPending = false;
        }

        /// <summary>
        /// Host-only F5 boundary. The old disclosure stays visible while the old Lua
        /// environment is discarded; the next Enable may atomically replace identity
        /// only with the fixed editor preview package. Lua has no callback to this API.
        /// </summary>
        internal static void PrepareDevelopmentHotReload()
        {
            if (!Active || !RuntimeTrace.Active
                || !string.Equals(ModId, "lom_modkit_preview", StringComparison.Ordinal))
                throw new InvalidOperationException("当前会话不是可热重载的 F5 开发演出");
            if (!string.IsNullOrEmpty(FailureReason))
                throw new InvalidOperationException("强制披露已进入故障态：" + FailureReason);
            _developmentHotReloadPending = true;
        }

        /// <summary>在 Canvas 真正提交渲染前再校验一次，关闭 Update→Render 之间的单帧隐藏窗口。</summary>
        private static void BeforeCanvasRender()
        {
            if (!Active || _insideRenderGuard) return;
            _insideRenderGuard = true;
            try
            {
                MaintainFailClosed();
            }
            finally
            {
                _insideRenderGuard = false;
            }
        }

        /// <summary>
        /// 不依赖 Canvas 对象的最后渲染钩子。若恶意 Lua 同时销毁 Plugin、
        /// guardian、edge 并禁用原场景 Canvas，只要摄像机还在输出画面，
        /// 就会在提交画面前复活守护和强制标识。
        /// </summary>
        private static void BeforeCameraRender(Camera camera)
        {
            BeforeCanvasRender();
        }

        private static void SubscribeTrustedBoundaryEvents()
        {
            Camera.onPreCull -= BeforeCameraRender;
            Camera.onPreCull += BeforeCameraRender;
            SceneManager.sceneLoaded -= OnSceneLoaded;
            SceneManager.sceneLoaded += OnSceneLoaded;
            SceneManager.activeSceneChanged -= OnActiveSceneChanged;
            SceneManager.activeSceneChanged += OnActiveSceneChanged;
        }

        private static void OnSceneLoaded(Scene scene, LoadSceneMode mode)
        {
            TryCloseAtTrustedBoundary();
        }

        private static void OnActiveSceneChanged(Scene previous, Scene next)
        {
            TryCloseAtTrustedBoundary();
        }

        private static bool TryCloseAtTrustedBoundary()
        {
            if (!Active) return true;
            SceneController controller = SceneController.Instance;
            if (controller == null) return false;
            string scene = controller.CurrentScene;
            if (!string.Equals(scene, "Title", StringComparison.Ordinal)
                && !string.Equals(scene, "Free", StringComparison.Ordinal))
                return false;
            Disable();
            // Title/Free 是完整开发演出的生命周期边界。若只关披露而保留 Trace，
            // 下一次 F5 会把新试玩误判为旧 Story 会话热重载，并被披露层安全拒绝。
            RuntimeTrace.Reset();
            LuaManagerPatch.ResetAbortGuard();
            return true;
        }

        /// <summary>
        /// 可由 Plugin、独立 guardian 和渲染前回调重复调用的幂等安全检查。
        /// </summary>
        internal static bool MaintainFailClosed()
        {
            if (TryCloseAtTrustedBoundary()) return true;
            if (LuaManagerPatch.HasPendingAbort)
                LuaManagerPatch.RetryPendingAbort();
            if (!Active) return true;
            bool healthy = Tick();
            if (!healthy)
                LuaManagerPatch.AbortActivePlayback(
                    "强制玩家内容披露无法维持：" + (FailureReason ?? "未知错误"),
                    null, null, "mandatory_disclosure");
            return healthy;
        }

        /// <summary>Canvas 不可用时的独立全屏安全遮罩。</summary>
        internal static bool DrawFailureGuard()
        {
            if (!Active || string.IsNullOrEmpty(FailureReason)) return false;
            int oldDepth = GUI.depth;
            Color oldColor = GUI.color;
            Color oldContentColor = GUI.contentColor;
            Color oldBackgroundColor = GUI.backgroundColor;
            Matrix4x4 oldMatrix = GUI.matrix;
            bool oldEnabled = GUI.enabled;
            GUI.depth = -32768;
            GUI.matrix = Matrix4x4.identity;
            GUI.enabled = true;
            GUI.contentColor = Color.white;
            GUI.backgroundColor = Color.white;
            GUI.color = Color.black;
            GUI.DrawTexture(new Rect(0f, 0f, Screen.width, Screen.height), Texture2D.whiteTexture);
            GUI.color = Color.white;
            var style = new GUIStyle(GUI.skin.label)
            {
                alignment = TextAnchor.MiddleCenter,
                fontStyle = FontStyle.Bold,
                fontSize = Mathf.Clamp(Screen.height / 28, 20, 38),
                wordWrap = true
            };
            string shortHash = ModDisclosurePolicy.ShortFingerprint(PackageFingerprint);
            GUI.Label(
                new Rect(Screen.width * 0.1f, Screen.height * 0.25f, Screen.width * 0.8f, Screen.height * 0.5f),
                CurrentPrimaryText() + "\n\n" + I18n.T("disclosure.blocked")
                    + (string.IsNullOrEmpty(shortHash) ? "" : "\nSHA-256 " + shortHash),
                style);
            GUI.enabled = oldEnabled;
            GUI.matrix = oldMatrix;
            GUI.backgroundColor = oldBackgroundColor;
            GUI.contentColor = oldContentColor;
            GUI.color = oldColor;
            GUI.depth = oldDepth;
            return true;
        }

        /// <summary>
        /// 健康态也由独立 guardian 绘制一份不含作者字段的 IMGUI 固定章。
        /// 它与 Canvas 排序系统独立，避免恶意 Lua 把官方面板 Canvas 调到最高层后
        /// 盖住对白/卡片内标。
        /// </summary>
        internal static void DrawPersistentImGuiStamp()
        {
            if (!Active) return;
            int oldDepth = GUI.depth;
            Color oldColor = GUI.color;
            Color oldContentColor = GUI.contentColor;
            Color oldBackgroundColor = GUI.backgroundColor;
            Matrix4x4 oldMatrix = GUI.matrix;
            bool oldEnabled = GUI.enabled;
            GUI.depth = -32767;
            GUI.matrix = Matrix4x4.identity;
            GUI.enabled = true;
            GUI.contentColor = Color.white;
            GUI.backgroundColor = Color.white;

            // Canvas 边缘标已经承载完整作品身份并固定在右上角。独立 IMGUI 章只作
            // 跨 Canvas 的最后防线；对白区横跨屏幕底部时，章应沿对白框的水平中心
            // 对齐，避免在左侧头像、血条或正文起始处形成第二个视觉焦点。
            float width = Mathf.Min(360f, Mathf.Max(220f, Screen.width - 24f));
            float height = 34f;
            var rect = new Rect(
                Mathf.Max(12f, (Screen.width - width) * 0.5f),
                Mathf.Max(12f, Screen.height - height - 12f),
                width,
                height);
            GUI.color = new Color(0.08f, 0.06f, 0.05f, 0.78f);
            GUI.DrawTexture(rect, Texture2D.whiteTexture);
            GUI.color = Color.white;
            var style = new GUIStyle(GUI.skin.label)
            {
                alignment = TextAnchor.MiddleCenter,
                fontStyle = FontStyle.Bold,
                fontSize = Mathf.Clamp(Screen.height / 75, 11, 15),
                wordWrap = false,
                padding = new RectOffset(8, 8, 2, 2)
            };
            string shortHash = ModDisclosurePolicy.ShortFingerprint(PackageFingerprint);
            GUI.Label(rect, DisclosureIntegrity.FixedStamp() + "  ·  " + shortHash, style);

            GUI.enabled = oldEnabled;
            GUI.matrix = oldMatrix;
            GUI.backgroundColor = oldBackgroundColor;
            GUI.contentColor = oldContentColor;
            GUI.color = oldColor;
            GUI.depth = oldDepth;
        }

        internal static IEnumerator EmptyRoutine()
        {
            yield break;
        }

        /// <summary>
        /// 每帧检查三类标记。返回 false 表示无法保证披露，调用方必须立即终止演出；
        /// 不在这里自行 Disable，避免返回 Free 的 Loading 过渡出现无标记画面。
        /// </summary>
        internal static bool Tick()
        {
            if (!Active) return true;
            if (!string.IsNullOrEmpty(FailureReason))
            {
                // 故障态也要 best-effort 复活独立 guardian 和边缘标。否则恶意
                // Lua 可在销毁 Plugin 的同帧再销毁 UI，让转场遮罩一起消失。
                try
                {
                    EnsureGuardian();
                    EnsureEdgeOverlay();
                    RefreshLabels(true);
                    ProvenanceWatermark.Maintain();
                }
                catch (Exception ex)
                {
                    Log?.LogError("故障态披露守护重建失败：" + ex.Message);
                }
                return false;
            }
            try
            {
                EnsureSessionIntegrity();
                EnsureEdgeOverlay();
                RefreshLabels(false);
                if (!ProvenanceWatermark.Maintain())
                    throw new InvalidOperationException(
                        "来源水印刷新失败：" + ProvenanceWatermark.LastError);
                SyncDialogChips();
                SyncPanelChips();
                if (!IsChipStructurallyValid(_edgeChip))
                    throw new InvalidOperationException("屏幕常驻标记校验失败");
                return true;
            }
            catch (Exception ex)
            {
                ReportMandatorySurfaceFailure("强制披露刷新失败：" + ex.Message);
                return false;
            }
        }

        /// <summary>
        /// 把芯片挂到死亡、结局、人物介绍等关键面板上，并记住父面板以便被删后自愈。
        /// </summary>
        internal static GameObject AttachToPanel(Transform parent)
        {
            return AttachToPanel(
                parent,
                Vector2.one,
                Vector2.one,
                new Vector2(-18f, -14f));
        }

        /// <summary>
        /// 把来源标记挂到关键面板的指定局部位置。anchor/pivot/offset 都属于该面板
        /// 自身的 RectTransform 坐标，避免全屏父节点把所有卡片标记挤到屏幕右上角。
        /// </summary>
        internal static GameObject AttachToPanel(
            Transform parent,
            Vector2 anchor,
            Vector2 pivot,
            Vector2 offset)
        {
            if (!Active) return null;
            if (parent == null)
            {
                ReportMandatorySurfaceFailure("关键结果面板不存在，无法附加来源标记");
                return null;
            }

            try
            {
                PanelBinding binding = FindPanelBinding(parent);
                if (binding == null)
                {
                    binding = new PanelBinding { Parent = parent };
                    PanelBindings.Add(binding);
                }
                binding.Anchor = anchor;
                binding.Pivot = pivot;
                binding.Offset = offset;
                DestroyChip(ref binding.Chip);
                Transform existing = parent.Find(ModDisclosurePolicy.ChipObjectName);
                if (existing != null) UnityEngine.Object.Destroy(existing.gameObject);
                binding.Chip = BuildSurfaceChip(parent, ModDisclosurePolicy.ChipObjectName, FindFontOn(parent));
                RepairChip(
                    binding.Chip, parent, binding.Anchor, binding.Pivot, binding.Offset,
                    SurfaceMaxWidth, 15, 11, false);
                return binding.Chip;
            }
            catch (Exception ex)
            {
                ReportMandatorySurfaceFailure("关键结果面板来源标记创建失败：" + ex.Message);
                return null;
            }
        }

        internal static void ReportMandatorySurfaceFailure(string reason)
        {
            if (!Active || !string.IsNullOrEmpty(FailureReason)) return;
            FailureReason = string.IsNullOrEmpty(reason) ? "未知披露故障" : reason;
            Log?.LogError(FailureReason + "；将终止当前 MOD 演出");
        }

        private static void EnsureSessionIntegrity()
        {
            if (!DisclosureIntegrity.VerifySessionSeal(ModId, PackageFingerprint, _sessionSeal))
                throw new InvalidOperationException("披露会话身份完整性校验失败");
            if (!DisclosureIntegrity.IsFixedStampIntact())
                throw new InvalidOperationException("固定非官方标记完整性校验失败");
        }

        private static void EnsureEdgeOverlay()
        {
            EnsureGuardian();
            if (_edgeRoot == null)
            {
                _edgeRoot = new GameObject(ModDisclosurePolicy.EdgeRootName, typeof(RectTransform));
                _edgeRoot.hideFlags = HideFlags.DontSave;
                UnityEngine.Object.DontDestroyOnLoad(_edgeRoot);
            }

            _edgeRoot.transform.SetParent(null, false);
            _edgeRoot.SetActive(true);
            RectTransform rootRect = (RectTransform)_edgeRoot.transform;
            rootRect.anchorMin = Vector2.zero;
            rootRect.anchorMax = Vector2.one;
            rootRect.offsetMin = Vector2.zero;
            rootRect.offsetMax = Vector2.zero;
            rootRect.localScale = Vector3.one;
            rootRect.localRotation = Quaternion.identity;
            rootRect.anchoredPosition = Vector2.zero;
            Canvas canvas = _edgeRoot.GetComponent<Canvas>();
            if (canvas == null) canvas = _edgeRoot.AddComponent<Canvas>();
            canvas.enabled = true;
            canvas.renderMode = RenderMode.ScreenSpaceOverlay;
            canvas.overrideSorting = true;
            canvas.sortingOrder = TopSortingOrder;
            canvas.sortingLayerID = 0;
            canvas.targetDisplay = 0;

            CanvasScaler scaler = _edgeRoot.GetComponent<CanvasScaler>();
            if (scaler == null) scaler = _edgeRoot.AddComponent<CanvasScaler>();
            scaler.enabled = true;
            scaler.uiScaleMode = CanvasScaler.ScaleMode.ScaleWithScreenSize;
            scaler.referenceResolution = new Vector2(1920f, 1080f);
            scaler.matchWidthOrHeight = 0.5f;

            CanvasGroup[] rootGroups = _edgeRoot.GetComponents<CanvasGroup>();
            if (rootGroups.Length == 0)
                rootGroups = new CanvasGroup[] { _edgeRoot.AddComponent<CanvasGroup>() };
            for (int i = 0; i < rootGroups.Length; i++)
            {
                rootGroups[i].alpha = 1f;
                rootGroups[i].blocksRaycasts = false;
                rootGroups[i].interactable = false;
                rootGroups[i].ignoreParentGroups = true;
            }

            if (!IsChipStructurallyValid(_edgeChip))
            {
                DestroyChip(ref _edgeChip);
                _edgeChip = BuildChip(_edgeRoot.transform, "edge", ResolveFont(null), 16, 12, EdgeMaxWidth, true);
            }
            RepairChip(
                _edgeChip, _edgeRoot.transform, Vector2.one, Vector2.one,
                new Vector2(-20f, -18f), EdgeMaxWidth, 16, 12, true);
        }

        private static void EnsureGuardian()
        {
            if (_guardianRoot == null)
            {
                _guardianRoot = new GameObject(GuardianObjectName);
                _guardianRoot.hideFlags = HideFlags.DontSave;
                UnityEngine.Object.DontDestroyOnLoad(_guardianRoot);
            }
            _guardianRoot.transform.SetParent(null, false);
            _guardianRoot.transform.localPosition = Vector3.zero;
            _guardianRoot.transform.localScale = Vector3.one;
            _guardianRoot.transform.localRotation = Quaternion.identity;
            _guardianRoot.SetActive(true);
            ModDisclosureGuardian guardian = _guardianRoot.GetComponent<ModDisclosureGuardian>();
            if (guardian == null) guardian = _guardianRoot.AddComponent<ModDisclosureGuardian>();
            guardian.enabled = true;
        }

        private static void SyncDialogChips()
        {
            // LuaUtils.SetSayDialog 能改写 ActiveSayDialog，所以不把这个可变指针
            // 当成权威来源。扫描全部激活的 SayDialog，并保留已观察的可见宿主。
            SayDialog[] dialogs = UnityEngine.Object.FindObjectsOfType<SayDialog>();
            for (int i = 0; i < dialogs.Length; i++)
            {
                SayDialog dialog = dialogs[i];
                if (dialog == null || !dialog.gameObject.activeInHierarchy) continue;
                EnsureDialogBinding(dialog);
            }
            SayDialog current = SayDialog.ActiveSayDialog;
            if (current != null && current.gameObject.activeInHierarchy)
                EnsureDialogBinding(current);

            for (int i = DialogBindings.Count - 1; i >= 0; i--)
            {
                DialogBinding binding = DialogBindings[i];
                if (binding == null || binding.Parent == null)
                {
                    if (binding != null)
                    {
                        DestroyChip(ref binding.Watermark);
                    }
                    DialogBindings.RemoveAt(i);
                    continue;
                }
                // 父节点真正隐藏/销毁才退绑。即使 Lua 把指针设 null、
                // 换成诱饵对象或销毁 SayDialog 组件，仍保留真实 UI 上的内标。
                if (!binding.Parent.gameObject.activeInHierarchy)
                {
                    DestroyChip(ref binding.Watermark);
                    DialogBindings.RemoveAt(i);
                    continue;
                }
                Font font = ResolveFont(binding.Host != null && binding.Host.StoryTextObject != null
                    ? binding.Host.StoryTextObject.font : FindFontOn(binding.Parent));
                if (!IsDialogWatermarkStructurallyValid(binding.Watermark))
                {
                    DestroyChip(ref binding.Watermark);
                    binding.Watermark = BuildDialogWatermark(binding.Parent, font);
                }
                RectTransform storyRect = binding.Host != null
                    && binding.Host.StoryTextObject != null
                    ? binding.Host.StoryTextObject.rectTransform : null;
                RepairDialogWatermark(
                    binding.Watermark, binding.Parent, font, storyRect);
            }
        }

        private static void EnsureDialogBinding(SayDialog dialog)
        {
            RectTransform parent = FindDialogChrome(dialog);
            if (parent == null)
                throw new InvalidOperationException("当前对白框没有可附加标记的 RectTransform");
            for (int i = 0; i < DialogBindings.Count; i++)
            {
                DialogBinding existing = DialogBindings[i];
                if (existing == null || existing.Host != dialog) continue;
                if (existing.Parent != parent)
                {
                    existing.Parent = parent;
                    if (existing.Watermark != null)
                        existing.Watermark.transform.SetParent(parent, false);
                }
                return;
            }
            Font font = ResolveFont(dialog.StoryTextObject != null ? dialog.StoryTextObject.font : null);
            GameObject watermark = null;
            try
            {
                watermark = BuildDialogWatermark(parent, font);
                DialogBindings.Add(new DialogBinding
                {
                    Host = dialog,
                    Parent = parent,
                    Watermark = watermark
                });
            }
            catch
            {
                DestroyChip(ref watermark);
                throw;
            }
        }

        private static void SyncPanelChips()
        {
            for (int i = PanelBindings.Count - 1; i >= 0; i--)
            {
                PanelBinding binding = PanelBindings[i];
                if (binding == null || binding.Parent == null)
                {
                    PanelBindings.RemoveAt(i);
                    continue;
                }
                if (!binding.Parent.gameObject.activeInHierarchy) continue;
                if (!IsChipStructurallyValid(binding.Chip))
                {
                    DestroyChip(ref binding.Chip);
                    binding.Chip = BuildSurfaceChip(
                        binding.Parent,
                        ModDisclosurePolicy.ChipObjectName,
                        FindFontOn(binding.Parent));
                }
                RepairChip(
                    binding.Chip, binding.Parent, binding.Anchor, binding.Pivot, binding.Offset,
                    SurfaceMaxWidth, 15, 11, false);
            }
        }

        private static GameObject BuildSurfaceChip(Transform parent, string name, Font preferred)
        {
            return BuildChip(parent, name, ResolveFont(preferred), 15, 11, SurfaceMaxWidth, false);
        }

        private static GameObject BuildDialogWatermark(Transform parent, Font font)
        {
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            if (font == null) throw new InvalidOperationException("没有可用于对白水印的字体");
            string name = _dialogWatermarkName;
            if (string.IsNullOrEmpty(name))
                throw new InvalidOperationException("对白水印保护名称缺失");
            var go = new GameObject(name, typeof(RectTransform));
            RectTransform rect = (RectTransform)go.transform;
            rect.SetParent(parent, false);

            Canvas canvas = go.AddComponent<Canvas>();
            canvas.overrideSorting = true;
            canvas.sortingOrder = TopSortingOrder;
            CanvasGroup group = go.AddComponent<CanvasGroup>();
            group.alpha = 1f;
            group.interactable = false;
            group.blocksRaycasts = false;
            LayoutElement layout = go.AddComponent<LayoutElement>();
            layout.ignoreLayout = true;

            Image background = go.AddComponent<Image>();
            background.sprite = WhiteSprite();
            background.color = new Color(0.04f, 0.03f, 0.025f, 0.42f);
            background.raycastTarget = false;

            var labelObject = new GameObject(DialogLabelObjectName, typeof(RectTransform));
            RectTransform labelRect = (RectTransform)labelObject.transform;
            labelRect.SetParent(rect, false);
            Text label = labelObject.AddComponent<Text>();
            label.font = font;
            label.text = DisclosureIntegrity.VisibleWatermarkText(_shortFingerprint);
            label.fontSize = 12;
            label.fontStyle = FontStyle.Bold;
            label.alignment = TextAnchor.MiddleCenter;
            label.color = new Color(1f, 0.77f, 0.43f, 0.88f);
            label.horizontalOverflow = HorizontalWrapMode.Overflow;
            label.verticalOverflow = VerticalWrapMode.Truncate;
            label.resizeTextForBestFit = true;
            label.resizeTextMinSize = 9;
            label.resizeTextMaxSize = 12;
            label.raycastTarget = false;
            label.supportRichText = false;
            return go;
        }

        /// <summary>
        /// 始终挂到正文的直接 RectTransform 父级，再按正文实际边界计算底部居中位置。
        /// 不能把高度与 Screen 像素混算，也不能把标记挂到正文自身后伸出 Mask 区域。
        /// </summary>
        private static RectTransform FindDialogChrome(SayDialog dialog)
        {
            Text story = dialog.StoryTextObject;
            if (story == null) return dialog.transform as RectTransform;
            RectTransform storyRect = story.rectTransform;
            RectTransform parent = storyRect.parent as RectTransform;
            return parent != null ? parent : storyRect;
        }

        private static GameObject BuildChip(
            Transform parent,
            string name,
            Font font,
            int primarySize,
            int detailSize,
            float maxWidth,
            bool ignoreParentGroups)
        {
            if (parent == null) throw new ArgumentNullException(nameof(parent));
            if (font == null) throw new InvalidOperationException("没有可用于披露标记的字体");

            var go = new GameObject(name, typeof(RectTransform));
            RectTransform rect = (RectTransform)go.transform;
            rect.SetParent(parent, false);
            rect.localScale = Vector3.one;
            rect.localRotation = Quaternion.identity;
            rect.sizeDelta = new Vector2(260f, 50f);

            Image bg = go.AddComponent<Image>();
            bg.sprite = WhiteSprite();
            bg.color = new Color(0.08f, 0.06f, 0.05f, 0.92f);
            bg.raycastTarget = false;

            Canvas localCanvas = go.AddComponent<Canvas>();
            localCanvas.overrideSorting = true;
            localCanvas.sortingOrder = TopSortingOrder;
            CanvasGroup localGroup = go.AddComponent<CanvasGroup>();
            localGroup.alpha = 1f;
            localGroup.interactable = false;
            localGroup.blocksRaycasts = false;
            localGroup.ignoreParentGroups = ignoreParentGroups;
            LayoutElement layout = go.AddComponent<LayoutElement>();
            layout.ignoreLayout = true;

            var accentGo = new GameObject(AccentObjectName, typeof(RectTransform));
            RectTransform accentRect = (RectTransform)accentGo.transform;
            accentRect.SetParent(rect, false);
            accentRect.anchorMin = new Vector2(0f, 0f);
            accentRect.anchorMax = new Vector2(0f, 1f);
            accentRect.pivot = new Vector2(0f, 0.5f);
            accentRect.sizeDelta = new Vector2(5f, 0f);
            accentRect.anchoredPosition = Vector2.zero;
            Image accent = accentGo.AddComponent<Image>();
            accent.sprite = WhiteSprite();
            accent.color = new Color(0.92f, 0.57f, 0.16f, 1f);
            accent.raycastTarget = false;

            Text primary = AddLabel(rect, PrimaryObjectName, font, primarySize, FontStyle.Bold,
                new Color(1f, 0.91f, 0.76f, 1f), new Vector2(0f, 0.43f), Vector2.one);
            Text detail = AddLabel(rect, DetailObjectName, font, detailSize, FontStyle.Normal,
                new Color(0.92f, 0.89f, 0.83f, 0.98f), Vector2.zero, new Vector2(1f, 0.47f));
            primary.text = CurrentPrimaryText();
            detail.text = CurrentDetailText();
            ResizeChip(go, maxWidth);
            return go;
        }

        private static Text AddLabel(
            RectTransform parent,
            string name,
            Font font,
            int fontSize,
            FontStyle style,
            Color color,
            Vector2 anchorMin,
            Vector2 anchorMax)
        {
            var textGo = new GameObject(name, typeof(RectTransform));
            RectTransform textRect = (RectTransform)textGo.transform;
            textRect.SetParent(parent, false);
            textRect.anchorMin = anchorMin;
            textRect.anchorMax = anchorMax;
            textRect.offsetMin = new Vector2(14f, 1f);
            textRect.offsetMax = new Vector2(-9f, -1f);

            Text label = textGo.AddComponent<Text>();
            label.font = font;
            label.fontSize = fontSize;
            label.fontStyle = style;
            label.alignment = TextAnchor.MiddleLeft;
            label.color = color;
            label.horizontalOverflow = HorizontalWrapMode.Overflow;
            label.verticalOverflow = VerticalWrapMode.Truncate;
            label.resizeTextForBestFit = true;
            label.resizeTextMinSize = Math.Max(9, fontSize - 4);
            label.resizeTextMaxSize = fontSize;
            label.raycastTarget = false;
            label.supportRichText = false;
            Shadow shadow = textGo.AddComponent<Shadow>();
            shadow.effectColor = new Color(0f, 0f, 0f, 0.78f);
            shadow.effectDistance = new Vector2(1f, -1f);
            return label;
        }

        private static void ResizeChip(GameObject chip, float maxWidth)
        {
            if (chip == null) return;
            Text primary = FindLabel(chip, PrimaryObjectName);
            Text detail = FindLabel(chip, DetailObjectName);
            if (primary == null || detail == null) return;
            float preferred = Mathf.Max(primary.preferredWidth, detail.preferredWidth) + 28f;
            RectTransform rect = chip.transform as RectTransform;
            if (rect != null) rect.sizeDelta = new Vector2(Mathf.Clamp(preferred, 230f, maxWidth), 50f);
        }

        private static void RefreshLabels(bool force)
        {
            string primary = CurrentPrimaryText();
            string detail = CurrentDetailText();
            if (!force && primary == _lastPrimary && detail == _lastDetail) return;
            _lastPrimary = primary;
            _lastDetail = detail;
            SetChipLabels(_edgeChip, primary, detail, EdgeMaxWidth);
            for (int i = 0; i < DialogBindings.Count; i++)
            {
                DialogBinding binding = DialogBindings[i];
                Text label = binding != null ? FindDialogLabel(binding.Watermark) : null;
                if (label != null)
                    label.text = DisclosureIntegrity.VisibleWatermarkText(_shortFingerprint);
            }
            for (int i = 0; i < PanelBindings.Count; i++)
                SetChipLabels(PanelBindings[i].Chip, primary, detail, SurfaceMaxWidth);
        }

        private static string CurrentPrimaryText()
        {
            return I18n.T(ModDisclosurePolicy.LabelKey) + " / UNOFFICIAL";
        }

        private static string CurrentDetailText()
        {
            if (string.IsNullOrEmpty(_safeAuthor))
                return I18n.T(ModDisclosurePolicy.DetailWithoutAuthorKey, _safeName, _shortFingerprint);
            return I18n.T(ModDisclosurePolicy.DetailWithAuthorKey, _safeName, _safeAuthor, _shortFingerprint);
        }

        private static void SetChipLabels(GameObject chip, string primaryText, string detailText, float maxWidth)
        {
            if (chip == null) return;
            Text primary = FindLabel(chip, PrimaryObjectName);
            Text detail = FindLabel(chip, DetailObjectName);
            if (primary != null) primary.text = primaryText;
            if (detail != null) detail.text = detailText;
            ResizeChip(chip, maxWidth);
        }

        private static bool IsChipStructurallyValid(GameObject chip)
        {
            if (chip == null || !chip.activeSelf || !(chip.transform is RectTransform)) return false;
            Image bg = chip.GetComponent<Image>();
            Transform accent = chip.transform.Find(AccentObjectName);
            return bg != null && accent != null && accent.GetComponent<Image>() != null
                && FindLabel(chip, PrimaryObjectName) != null
                && FindLabel(chip, DetailObjectName) != null;
        }

        private static bool IsDialogWatermarkStructurallyValid(GameObject watermark)
        {
            return watermark != null && watermark.activeSelf
                && watermark.transform is RectTransform
                && watermark.GetComponent<Canvas>() != null
                && watermark.GetComponent<Image>() != null
                && FindDialogLabel(watermark) != null;
        }

        private static void RepairDialogWatermark(
            GameObject watermark, Transform expectedParent, Font font, RectTransform storyRect)
        {
            if (!IsDialogWatermarkStructurallyValid(watermark))
                throw new InvalidOperationException("对白区域水印结构不完整");
            if (expectedParent == null || font == null)
                throw new InvalidOperationException("对白区域水印宿主或字体不存在");

            if (string.IsNullOrEmpty(_dialogWatermarkName))
                throw new InvalidOperationException("对白水印保护名称缺失");
            watermark.name = _dialogWatermarkName;
            watermark.SetActive(true);
            RectTransform rect = (RectTransform)watermark.transform;
            if (rect.parent != expectedParent) rect.SetParent(expectedParent, false);
            RectTransform parentRect = expectedParent as RectTransform;
            rect.anchorMin = Vector2.zero;
            rect.anchorMax = Vector2.zero;
            rect.pivot = new Vector2(0.5f, 1f);
            Bounds storyBounds = storyRect != null && parentRect != null && storyRect != parentRect
                ? RectTransformUtility.CalculateRelativeRectTransformBounds(parentRect, storyRect)
                : new Bounds(
                    parentRect != null ? (Vector3)parentRect.rect.center : Vector3.zero,
                    parentRect != null ? (Vector3)parentRect.rect.size : new Vector3(360f, 80f, 0f));
            float parentWidth = parentRect != null ? parentRect.rect.width : 360f;
            float storyWidth = storyBounds.size.x > 1f ? storyBounds.size.x : parentWidth;
            rect.sizeDelta = new Vector2(
                Mathf.Clamp(Mathf.Min(parentWidth - 16f, storyWidth), 180f, 440f), 28f);
            float parentLeft = parentRect != null ? parentRect.rect.xMin : -180f;
            float parentBottom = parentRect != null ? parentRect.rect.yMin : -40f;
            float xFromBottomLeft = storyBounds.center.x - parentLeft;
            float topFromBottom = Mathf.Max(32f, storyBounds.min.y - parentBottom - 6f);
            rect.anchoredPosition = new Vector2(xFromBottomLeft, topFromBottom);
            rect.localScale = Vector3.one;
            rect.localRotation = Quaternion.identity;
            rect.SetAsLastSibling();

            Canvas canvas = watermark.GetComponent<Canvas>();
            canvas.enabled = true;
            canvas.overrideSorting = true;
            canvas.sortingOrder = TopSortingOrder;
            canvas.sortingLayerID = 0;
            canvas.targetDisplay = 0;
            CanvasGroup[] groups = watermark.GetComponents<CanvasGroup>();
            if (groups.Length == 0)
                groups = new CanvasGroup[] { watermark.AddComponent<CanvasGroup>() };
            for (int i = 0; i < groups.Length; i++)
            {
                groups[i].enabled = true;
                groups[i].alpha = 1f;
                groups[i].interactable = false;
                groups[i].blocksRaycasts = false;
                // 对白自身淡出时一起淡出，但额外加在水印对象上的组不能隐藏它。
                groups[i].ignoreParentGroups = false;
            }
            LayoutElement layout = watermark.GetComponent<LayoutElement>();
            if (layout == null) layout = watermark.AddComponent<LayoutElement>();
            layout.ignoreLayout = true;

            Image background = watermark.GetComponent<Image>();
            background.enabled = true;
            background.sprite = WhiteSprite();
            background.material = null;
            Color backgroundColor = new Color(0.04f, 0.03f, 0.025f, 0.42f);
            background.color = backgroundColor;
            background.raycastTarget = false;
            background.maskable = false;
            background.canvasRenderer.cull = false;
            background.canvasRenderer.SetAlpha(1f);
            background.canvasRenderer.SetColor(backgroundColor);
            background.SetAllDirty();

            Text label = FindDialogLabel(watermark);
            RectTransform labelRect = label.rectTransform;
            if (labelRect.parent != rect) labelRect.SetParent(rect, false);
            labelRect.anchorMin = Vector2.zero;
            labelRect.anchorMax = Vector2.one;
            labelRect.offsetMin = new Vector2(10f, 1f);
            labelRect.offsetMax = new Vector2(-10f, -1f);
            labelRect.localScale = Vector3.one;
            labelRect.localRotation = Quaternion.identity;
            label.gameObject.SetActive(true);
            label.enabled = true;
            label.font = font;
            label.fontSize = 12;
            label.fontStyle = FontStyle.Bold;
            label.alignment = TextAnchor.MiddleCenter;
            label.horizontalOverflow = HorizontalWrapMode.Overflow;
            label.verticalOverflow = VerticalWrapMode.Truncate;
            label.resizeTextForBestFit = true;
            label.resizeTextMinSize = 9;
            label.resizeTextMaxSize = 12;
            label.raycastTarget = false;
            label.supportRichText = false;
            label.maskable = false;
            label.material = null;
            label.text = DisclosureIntegrity.VisibleWatermarkText(_shortFingerprint);
            Color color = new Color(1f, 0.77f, 0.43f, 0.88f);
            label.color = color;
            label.canvasRenderer.cull = false;
            label.canvasRenderer.SetAlpha(1f);
            label.canvasRenderer.SetColor(color);
            label.SetAllDirty();
            CanvasGroup[] labelGroups = label.gameObject.GetComponents<CanvasGroup>();
            for (int i = 0; i < labelGroups.Length; i++)
            {
                labelGroups[i].enabled = true;
                labelGroups[i].alpha = 1f;
                labelGroups[i].interactable = false;
                labelGroups[i].blocksRaycasts = false;
                labelGroups[i].ignoreParentGroups = false;
            }
            Canvas[] labelCanvases = label.gameObject.GetComponents<Canvas>();
            for (int i = 0; i < labelCanvases.Length; i++)
            {
                labelCanvases[i].enabled = true;
                labelCanvases[i].overrideSorting = false;
                labelCanvases[i].targetDisplay = 0;
            }
            Shadow shadow = label.GetComponent<Shadow>();
            if (shadow == null) shadow = label.gameObject.AddComponent<Shadow>();
            shadow.enabled = true;
            shadow.effectColor = new Color(0f, 0f, 0f, 0.82f);
            shadow.effectDistance = new Vector2(1f, -1f);
        }

        private static Text FindDialogLabel(GameObject watermark)
        {
            if (watermark == null) return null;
            Transform child = watermark.transform.Find(DialogLabelObjectName);
            return child != null ? child.GetComponent<Text>() : null;
        }

        private static void RepairChip(
            GameObject chip,
            Transform expectedParent,
            Vector2 anchor,
            Vector2 pivot,
            Vector2 inset,
            float maxWidth,
            int primarySize,
            int detailSize,
            bool ignoreParentGroups)
        {
            if (!IsChipStructurallyValid(chip))
                throw new InvalidOperationException("来源标记结构不完整");
            if (expectedParent == null)
                throw new InvalidOperationException("来源标记父节点不存在");
            chip.SetActive(true);
            RectTransform rect = (RectTransform)chip.transform;
            if (rect.parent != expectedParent) rect.SetParent(expectedParent, false);
            rect.anchorMin = anchor;
            rect.anchorMax = anchor;
            rect.pivot = pivot;
            rect.anchoredPosition = inset;
            rect.localScale = Vector3.one;
            rect.localRotation = Quaternion.identity;
            chip.transform.SetAsLastSibling();

            Canvas canvas = chip.GetComponent<Canvas>();
            if (canvas == null) canvas = chip.AddComponent<Canvas>();
            canvas.enabled = true;
            canvas.overrideSorting = true;
            canvas.sortingOrder = TopSortingOrder;
            canvas.sortingLayerID = 0;
            canvas.targetDisplay = 0;
            CanvasGroup[] groups = chip.GetComponents<CanvasGroup>();
            if (groups.Length == 0)
                groups = new CanvasGroup[] { chip.AddComponent<CanvasGroup>() };
            for (int i = 0; i < groups.Length; i++)
            {
                groups[i].alpha = 1f;
                groups[i].interactable = false;
                groups[i].blocksRaycasts = false;
                groups[i].ignoreParentGroups = ignoreParentGroups;
            }
            LayoutElement layout = chip.GetComponent<LayoutElement>();
            if (layout == null) layout = chip.AddComponent<LayoutElement>();
            layout.ignoreLayout = true;

            Image bg = chip.GetComponent<Image>();
            bg.enabled = true;
            bg.sprite = WhiteSprite();
            bg.material = null;
            Color bgColor = new Color(0.08f, 0.06f, 0.05f, 0.92f);
            bg.color = bgColor;
            bg.raycastTarget = false;
            bg.canvasRenderer.cull = false;
            bg.canvasRenderer.SetAlpha(1f);
            bg.canvasRenderer.SetColor(bgColor);
            bg.SetAllDirty();
            Transform accentTransform = chip.transform.Find(AccentObjectName);
            RectTransform accentRect = (RectTransform)accentTransform;
            accentRect.anchorMin = new Vector2(0f, 0f);
            accentRect.anchorMax = new Vector2(0f, 1f);
            accentRect.pivot = new Vector2(0f, 0.5f);
            accentRect.sizeDelta = new Vector2(5f, 0f);
            accentRect.anchoredPosition = Vector2.zero;
            accentRect.localScale = Vector3.one;
            accentRect.localRotation = Quaternion.identity;
            Image accent = accentTransform.GetComponent<Image>();
            accent.enabled = true;
            accent.sprite = WhiteSprite();
            accent.material = null;
            Color accentColor = new Color(0.92f, 0.57f, 0.16f, 1f);
            accent.color = accentColor;
            accent.raycastTarget = false;
            CanvasGroup[] accentGroups = accent.gameObject.GetComponents<CanvasGroup>();
            for (int i = 0; i < accentGroups.Length; i++)
            {
                accentGroups[i].alpha = 1f;
                accentGroups[i].interactable = false;
                accentGroups[i].blocksRaycasts = false;
                accentGroups[i].ignoreParentGroups = false;
            }
            accent.canvasRenderer.cull = false;
            accent.canvasRenderer.SetAlpha(1f);
            accent.canvasRenderer.SetColor(accentColor);
            accent.SetAllDirty();

            Text primary = FindLabel(chip, PrimaryObjectName);
            Text detail = FindLabel(chip, DetailObjectName);
            RepairLabel(primary, CurrentPrimaryText(), primarySize, true);
            RepairLabel(detail, CurrentDetailText(), detailSize, false);
            ResizeChip(chip, maxWidth);
        }

        private static void RepairLabel(Text label, string expected, int fontSize, bool primary)
        {
            label.gameObject.SetActive(true);
            RectTransform rect = label.rectTransform;
            rect.anchorMin = primary ? new Vector2(0f, 0.43f) : Vector2.zero;
            rect.anchorMax = primary ? Vector2.one : new Vector2(1f, 0.47f);
            rect.offsetMin = new Vector2(14f, 1f);
            rect.offsetMax = new Vector2(-9f, -1f);
            rect.localScale = Vector3.one;
            rect.localRotation = Quaternion.identity;
            label.enabled = true;
            label.fontSize = fontSize;
            label.fontStyle = primary ? FontStyle.Bold : FontStyle.Normal;
            label.alignment = TextAnchor.MiddleLeft;
            label.horizontalOverflow = HorizontalWrapMode.Overflow;
            label.verticalOverflow = VerticalWrapMode.Truncate;
            label.resizeTextForBestFit = true;
            label.resizeTextMinSize = Math.Max(9, fontSize - 4);
            label.resizeTextMaxSize = fontSize;
            label.raycastTarget = false;
            label.supportRichText = false;
            label.text = expected;
            label.font = ResolveFont(null);
            label.material = null;
            Color expectedColor = primary
                ? new Color(1f, 0.91f, 0.76f, 1f)
                : new Color(0.92f, 0.89f, 0.83f, 0.98f);
            label.color = expectedColor;
            CanvasGroup[] groups = label.gameObject.GetComponents<CanvasGroup>();
            for (int i = 0; i < groups.Length; i++)
            {
                groups[i].alpha = 1f;
                groups[i].interactable = false;
                groups[i].blocksRaycasts = false;
                groups[i].ignoreParentGroups = false;
            }
            Canvas[] canvases = label.gameObject.GetComponents<Canvas>();
            for (int i = 0; i < canvases.Length; i++)
            {
                canvases[i].enabled = true;
                canvases[i].overrideSorting = false;
                canvases[i].targetDisplay = 0;
            }
            label.canvasRenderer.cull = false;
            label.canvasRenderer.SetAlpha(1f);
            label.canvasRenderer.SetColor(expectedColor);
            label.SetAllDirty();
            Shadow shadow = label.GetComponent<Shadow>();
            if (shadow == null) shadow = label.gameObject.AddComponent<Shadow>();
            shadow.enabled = true;
            shadow.effectColor = new Color(0f, 0f, 0f, 0.78f);
            shadow.effectDistance = new Vector2(1f, -1f);
        }

        private static Text FindLabel(GameObject chip, string name)
        {
            if (chip == null) return null;
            Transform child = chip.transform.Find(name);
            return child == null ? null : child.GetComponent<Text>();
        }

        private static PanelBinding FindPanelBinding(Transform parent)
        {
            for (int i = PanelBindings.Count - 1; i >= 0; i--)
            {
                PanelBinding binding = PanelBindings[i];
                if (binding == null || binding.Parent == null)
                {
                    PanelBindings.RemoveAt(i);
                    continue;
                }
                if (binding.Parent == parent) return binding;
            }
            return null;
        }

        private static void ClearVisuals()
        {
            for (int i = 0; i < DialogBindings.Count; i++)
            {
                DialogBinding binding = DialogBindings[i];
                if (binding != null)
                {
                    DestroyChip(ref binding.Watermark);
                }
            }
            DialogBindings.Clear();
            for (int i = 0; i < PanelBindings.Count; i++)
            {
                PanelBinding binding = PanelBindings[i];
                if (binding != null) DestroyChip(ref binding.Chip);
            }
            PanelBindings.Clear();
            if (_edgeRoot != null) UnityEngine.Object.Destroy(_edgeRoot);
            _edgeRoot = null;
            _edgeChip = null;
            if (_guardianRoot != null) UnityEngine.Object.Destroy(_guardianRoot);
            _guardianRoot = null;
        }

        private static void DestroyChip(ref GameObject chip)
        {
            if (chip != null) UnityEngine.Object.Destroy(chip);
            chip = null;
        }

        private static Font FindFontOn(Transform root)
        {
            if (root == null) return null;
            Text text = root.GetComponentInChildren<Text>(true);
            return text != null ? text.font : null;
        }

        private static Font ResolveFont(Font preferred)
        {
            if (preferred != null) return preferred;
            if (_osFont != null) return _osFont;
            try
            {
                _osFont = Font.CreateDynamicFontFromOSFont(new string[]
                {
                    "Microsoft YaHei",
                    "微软雅黑",
                    "PingFang SC",
                    "Noto Sans CJK SC",
                    "Source Han Sans SC",
                    "SimHei",
                    "Microsoft JhengHei",
                    "Meiryo",
                    "Yu Gothic UI",
                    "Noto Sans CJK JP",
                    "Malgun Gothic",
                    "맑은 고딕",
                    "Noto Sans CJK KR"
                }, 16);
                if (_osFont != null) return _osFont;
            }
            catch
            {
                // 系统字体不可用时走 Unity 内置 Arial；至少固定英文 MOD / SHA-256 仍可辨识。
            }
            return Resources.GetBuiltinResource<Font>("Arial.ttf");
        }

        private static Sprite WhiteSprite()
        {
            if (_whiteSprite != null) return _whiteSprite;
            _whiteSprite = Sprite.Create(
                Texture2D.whiteTexture,
                new Rect(0f, 0f, Texture2D.whiteTexture.width, Texture2D.whiteTexture.height),
                new Vector2(0.5f, 0.5f),
                100f);
            _whiteSprite.name = "lom_disclosure_px";
            return _whiteSprite;
        }
    }
}
