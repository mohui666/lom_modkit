using System;
using System.Collections.Generic;
using System.Globalization;
using Mortal.Core;
using Mortal.Free;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // An explicit opt-in to mutate the current isolated mode's progress. No
    // snapshots, new save names, forced events or inferred history are created.
    internal static partial class DevelopmentNavigationRuntime
    {
        private static string _previewSlot, _previewCampaign, _boundSlot, _contextSlot;
        private static ModPlayModeKind _boundMode, _contextMode;
        private static bool _enabled;
        private static string _year = "1", _month = "4", _script = "", _search = "", _message = "";
        private static bool _messageIsError;
        private static bool _draftReady;
        private static int _stage = 1;
        private static Vector2 _scroll;
        private static List<Dictionary<string,object>> _rows;
        private static DevelopmentEvent _selected;
        private static DevelopmentEventCatalog _catalog, _filteredCatalog;
        private static List<DevelopmentEvent> _filtered = new List<DevelopmentEvent>();
        private static string _filterText;
        private static Vector2 _detailScroll;
        private static int _jumpTab;
        private static readonly string[] StageKeys = { "dev.early", "dev.middle", "dev.late" };
        private sealed class NavigationRecord
        {
            internal string Script, Scene, EnteredScript, Failure;
            internal int Year, Month, Stage;
            internal bool Failed;
        }
        private static readonly List<NavigationRecord> _history = new List<NavigationRecord>();
        private static NavigationRecord _pending;
        private static int _pendingFrame;

        internal static bool Enabled { get { return _enabled; } }

        internal static void Clear()
        {
            _enabled = false;
            _previewSlot = _previewCampaign = _boundSlot = _contextSlot = null;
            _rows = null;
            _selected = null;
            _script = _search = _message = "";
            _messageIsError = false;
            _draftReady = false;
            _scroll = Vector2.zero;
            _detailScroll = Vector2.zero;
            _jumpTab = 0;
            _catalog = _filteredCatalog = null;
            _filtered.Clear(); _history.Clear(); _pending = null;
            DevelopmentEventText.Clear();
            ResetPanel();
        }

        internal static void ArmPreview(ModPackage package, PreviewRequest request)
        {
            if (!request.DeveloperMode) return;
            DevelopmentNavigationPolicy.ValidatePreview(package, request);
            var saves = SaveSystem.Instance;
            if (saves == null || ModPlayModeSession.Selected != ModPlayModeKind.Campaign
                || !CampaignIdentity.OwnsSlot(package.CampaignId, saves.CurrentSlot))
                throw new InvalidOperationException("开发测试没有进入所属预览隔离槽");
            Clear();
            _previewSlot = saves.CurrentSlot;
            _previewCampaign = package.CampaignId;
        }

        internal static bool HasContext
        {
            get
            {
                var saves = SaveSystem.Instance;
                return saves != null && DevelopmentNavigationPolicy.AllowsContext(
                    ModPlayModeSession.Selected, saves.CurrentSlot, _previewSlot, _previewCampaign,
                    ModCampaignState.ActiveModId, ModCampaignState.ActiveCampaignId,
                    ModCampaignState.Active && !ModCampaignState.DisableOfficialEvents);
            }
        }

        internal static void Maintain()
        {
            var scenes = SceneController.Instance;
            var saves = SaveSystem.Instance;
            if (scenes != null && scenes.CurrentScene == "Title") { Clear(); return; }
            // Read-only drafts and session records have the same slot ownership
            // as enabled editing; disabling editing must not bypass this check.
            if (_contextSlot != null && (saves == null || saves.CurrentSlot != _contextSlot
                    || ModPlayModeSession.Selected != _contextMode || !HasContext)
                || _enabled && (saves == null || saves.CurrentSlot != _boundSlot
                    || ModPlayModeSession.Selected != _boundMode)
                || _previewSlot != null && (saves == null || saves.CurrentSlot != _previewSlot
                    || ModPlayModeSession.Selected != ModPlayModeKind.Campaign || !HasContext))
                Clear();
            if (HasContext) { _contextSlot = saves.CurrentSlot; _contextMode = ModPlayModeSession.Selected; }
            if (_pending != null && scenes != null && Time.frameCount > _pendingFrame
                && Plugin.IsSceneReadyForModNavigation(scenes)
                && (scenes.CurrentScene == "Free" || scenes.CurrentScene == "Story"))
            {
                _pending.Scene = scenes.CurrentScene;
                _pending.EnteredScript = LuaManager.Instance == null ? "" : LuaManager.Instance.ScriptName;
                _message = OutcomeText(_pending);
                _messageIsError = false;
                _pending = null;
            }
        }

        private static bool Ready
        {
            get
            {
                var scenes = SceneController.Instance;
                var free = FreeManager.Instance;
                return HasContext && scenes != null && scenes.CurrentScene == "Free"
                    && Plugin.IsSceneReadyForModNavigation(scenes) && free != null
                    && !free.IsProcessing && Time.timeScale > 0f && !VideoRenderRuntime.Busy;
            }
        }

        internal static void Enable()
        {
            if (!Ready) throw new InvalidOperationException(I18n.MenuT("dev.require_free"));
            _boundSlot = SaveSystem.Instance.CurrentSlot;
            _boundMode = ModPlayModeSession.Selected;
            _enabled = true;
            InitializeDraftDate();
            _rows = null;
            _message = "";
            _messageIsError = false;
        }

        private static void InitializeDraftDate()
        {
            if (_draftReady || PlayerStatManagerData.Instance == null) return;
            var time = PlayerStatManagerData.Instance.GameTime;
            SetDraftDate(time.Year, time.Month, (int)time.Stage);
        }

        private static void RequireEnabled()
        {
            Maintain();
            if (!_enabled || !Ready || SaveSystem.Instance.CurrentSlot != _boundSlot
                || ModPlayModeSession.Selected != _boundMode)
                throw new InvalidOperationException(I18n.MenuT("dev.require_free"));
        }

        internal static void SetDate(int year, int month, int stage)
        {
            RequireEnabled();
            if (!DevelopmentNavigationPolicy.IsDate(year, month, stage))
                throw new ArgumentException(I18n.MenuT("dev.invalid_date"));
            var stat = PlayerStatManagerData.Instance;
            var missions = MissionManagerData.Instance;
            if (stat == null || missions == null)
                throw new InvalidOperationException(I18n.MenuT("dev.require_free"));
            ExecuteNavigation(new NavigationRecord { Year = year, Month = month, Stage = stage }, () =>
            {
                stat.SetGameTime(year, month, (MonthStageType)stage);
                stat.ResetActionCount();
                stat.SetNewRound(true);
                // Follow the original ChangeScene("Free") task check exactly once.
                bool previous = ScenarioReturnPatch.ReturningToFree;
                ScenarioReturnPatch.ReturningToFree = true;
                try
                {
                    missions.UpdateCheckMissions();
                    bool story = missions.HasAnyMissionTrigger();
                    if (story) SceneController.Instance.LoadStory();
                    else SceneController.Instance.LoadFree();
                }
                finally { ScenarioReturnPatch.ReturningToFree = previous; }
            });
        }

        internal static void GoToScript(string script)
        {
            RequireEnabled();
            script = (script ?? "").Trim();
            if (!VanillaStoryPatch.ValidTarget(script)
                || Resources.Load<TextAsset>("Story/ChineseTraditional/" + script) == null)
                throw new ArgumentException(I18n.MenuT("dev.invalid_script"));
            var stat = PlayerStatManagerData.Instance;
            ExecuteNavigation(new NavigationRecord { Script = script }, () =>
            {
                StoryEntryRuntime.Clear();
                stat.SetStoryScript(script);
                stat.SetStartScript(script);
                SceneController.Instance.LoadStory();
            });
        }

        private static void ExecuteNavigation(NavigationRecord record, Action operation)
        {
            _history.Insert(0, record);
            if (_history.Count > 8) _history.RemoveAt(_history.Count - 1);
            _pending = record; _pendingFrame = Time.frameCount;
            try { operation(); _message = OutcomeText(record); _messageIsError = false; }
            catch (Exception error)
            {
                record.Failed = true; record.Failure = error.Message;
                _pending = null;
                throw;
            }
        }

        private static string OutcomeText(NavigationRecord record)
        {
            if (record.Failed) return I18n.MenuT("dev.failed", DevelopmentEventText.Display(record.Failure, I18n.MenuLocale));
            if (record.Scene == "Free") return I18n.MenuT("dev.arrived_free");
            if (record.Scene == "Story") return I18n.MenuT("dev.arrived_story", record.EnteredScript);
            return I18n.MenuT("dev.requested");
        }

        private static void Select(DevelopmentEvent row)
        {
            _selected = row;
            _script = row.Script;
            _detailScroll = Vector2.zero;
            _pane = 0;
        }

        private static bool Run(Action action)
        {
            try { action(); return true; }
            catch (Exception error)
            {
                _message = I18n.MenuT("dev.failed", error.Message);
                _messageIsError = true;
                ScenarioRuntime.Log?.LogError("开发导航失败：" + error);
                return false;
            }
        }

    }
}
