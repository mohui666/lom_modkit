using System;
using System.Collections.Generic;
using System.Globalization;
using HarmonyLib;
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
        private static bool _syncChapterDate, _visitPlace;
        private static PositionType _draftPlace;
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
        private static readonly System.Reflection.FieldInfo PositionField = AccessTools.Field(typeof(PositionController), "_position");
        private sealed class NavigationRecord
        {
            internal string Script, Scene, EnteredScript, Failure;
            internal int Year, Month, Stage;
            internal PositionType Place;
            internal bool Failed, VisitedPlace;
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
            _draftReady = _syncChapterDate = _visitPlace = false;
            _draftPlace = PositionType.無;
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
                if (scenes.CurrentScene == "Free" && _pending.Place != PositionType.無)
                {
                    var free = FreeManager.Instance;
                    if (free == null || !free.EnableHotKey || free.IsProcessing) return;
                    if (!_pending.VisitedPlace && Run(() => VisitPendingPlace(_pending, free))) return;
                    if (_pending.Failed) { _pending = null; return; }
                }
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
                    && !free.IsProcessing && _pending == null && Time.timeScale > 0f && !VideoRenderRuntime.Busy;
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
        { SetDateAndPlace(year, month, stage, PositionType.無); }

        internal static void SetDateAndPlace(int year, int month, int stage, PositionType place)
        {
            RequireEnabled();
            if (!DevelopmentNavigationPolicy.IsDate(year, month, stage))
                throw new ArgumentException(I18n.MenuT("dev.invalid_date"));
            var stat = PlayerStatManagerData.Instance;
            var missions = MissionManagerData.Instance;
            if (stat == null || missions == null)
                throw new InvalidOperationException(I18n.MenuT("dev.require_free"));
            ExecuteNavigation(new NavigationRecord { Year = year, Month = month, Stage = stage, Place = place }, () =>
            {
                ApplyDate(stat, year, month, stage);
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

        private static void ApplyDate(PlayerStatManagerData stat, int year, int month, int stage)
        {
            stat.SetGameTime(year, month, (MonthStageType)stage);
            stat.ResetActionCount();
            stat.SetNewRound(true);
        }

        private static void VisitPendingPlace(NavigationRecord record, FreeManager free)
        {
            record.VisitedPlace = true;
            try
            {
                foreach (var position in UnityEngine.Object.FindObjectsOfType<PositionController>())
                {
                    if (position.gameObject.scene != free.gameObject.scene || (PositionType)PositionField.GetValue(position) != record.Place) continue;
                    position.OnPositionClick();
                    if (free.IsProcessing) return;
                    break;
                }
                throw new InvalidOperationException(I18n.MenuT("dev.place_unavailable", Display(record.Place.ToString())));
            }
            catch (Exception error)
            {
                record.Failed = true; record.Failure = error.Message;
                throw;
            }
        }

        internal static void GoToScript(string script) { StartChapter(script, 0, 0, 0); }

        internal static void GoToScriptAtDate(string script, int year, int month, int stage)
        {
            if (!DevelopmentNavigationPolicy.IsDate(year, month, stage))
                throw new ArgumentException(I18n.MenuT("dev.invalid_date"));
            StartChapter(script, year, month, stage);
        }

        private static void StartChapter(string script, int year, int month, int stage)
        {
            RequireEnabled();
            script = (script ?? "").Trim();
            TextAsset chapter = VanillaStoryPatch.ValidTarget(script)
                ? Resources.Load<TextAsset>("Story/ChineseTraditional/" + script) : null;
            if (chapter == null)
                throw new ArgumentException(I18n.MenuT("dev.invalid_script"));
            var stat = PlayerStatManagerData.Instance;
            ExecuteNavigation(new NavigationRecord { Script = chapter.name, Year = year, Month = month, Stage = stage }, () =>
            {
                if (year != 0) ApplyDate(stat, year, month, stage);
                StoryEntryRuntime.Clear();
                stat.SetStoryScript(chapter.name);
                stat.SetStartScript(chapter.name);
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
            if (record.Scene == "Story" && record.Place != PositionType.無 && !record.VisitedPlace)
                return I18n.MenuT("dev.place_preempted", record.EnteredScript);
            if (record.Scene == "Story") return I18n.MenuT("dev.arrived_story", record.EnteredScript);
            return I18n.MenuT("dev.requested");
        }

        private static void Select(DevelopmentEvent row)
        {
            _selected = row;
            PrepareChapter(row.Script, row);
            _draftPlace = EventPlace(row);
            _visitPlace = false;
            _detailScroll = Vector2.zero;
            _pane = 0;
        }

        private static void PrepareChapter(string script, DevelopmentEvent row)
        {
            _script = script;
            _syncChapterDate = SetEventDate(row);
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
