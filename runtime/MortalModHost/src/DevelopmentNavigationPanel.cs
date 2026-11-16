using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    internal static partial class DevelopmentNavigationRuntime
    {
        private static int _pane, _kind, _lastKind;
        private static bool _atCurrentDate, _includeInactive = true, _lastInactive;
        private static int? _lastRound;
        private static float _detailHeight = 800;
        private static List<DevelopmentEvent> _measuredRows;
        private static float[] _rowTops, _rowTitles, _rowDates, _rowMeta;
        private static float _measuredWidth, _rowsHeight;
        private static string _measuredLocale;
        private static string _filterLocale;
        private static readonly Dictionary<DevelopmentEvent, string> SearchEntries = new Dictionary<DevelopmentEvent, string>();
        private static readonly string[] KindKeys = { "dev.all", "dev.main", "dev.side" };
        private static readonly string[] PaneKeys = { "dev.node_details", "dev.jump_test", "dev.activity" };

        private static void ResetPanel()
        {
            _pane = _kind = 0; _atCurrentDate = false; _includeInactive = true;
            _lastRound = null; _filterText = null; _detailHeight = 800;
            _measuredRows = null;
            SearchEntries.Clear(); _filterLocale = null;
        }

        internal static void RefreshPresentation()
        {
            _filterText = null; _measuredRows = null; _filterLocale = null;
            _message = ""; _messageIsError = false; _detailScroll = Vector2.zero;
        }

        // True only after an accepted scene transition; browsing and filling a
        // form never enable development mode or change the game's progress.
        internal static bool Draw(Rect area)
        {
            Maintain();
            if (!HasContext) return false;
            InitializeDraftDate();
            bool ready = Ready;
            GUI.Label(new Rect(area.x, area.y, area.width - 340, 30), I18n.MenuT("dev.workspace"), ModMenuTheme.Heading);
            GUI.Label(new Rect(area.xMax - 324, area.y + 2, 136, 30), I18n.MenuT(_enabled ? "dev.editing" : "dev.read_only"),
                _enabled ? ModMenuTheme.Chip : ModMenuTheme.QuietChip);
            bool previous = GUI.enabled;
            GUI.enabled = previous && ready;
            if (GUI.Button(new Rect(area.xMax - 176, area.y - 2, 176, 38), I18n.MenuT(_enabled ? "dev.disable" : "dev.enable"),
                _enabled ? GUI.skin.button : ModMenuTheme.Primary))
            {
                if (_enabled) { _enabled = false; _boundSlot = null; _message = ""; _messageIsError = false; }
                else Run(Enable);
            }
            GUI.enabled = previous;
            GUI.Label(new Rect(area.x, area.y + 36, area.width, 23), CurrentDate(), ModMenuTheme.Single);
            GUI.Label(new Rect(area.x, area.y + 64, area.width, 22), I18n.MenuT(!ready ? "dev.require_free" : _enabled ? "dev.warning_short" : "dev.browse_hint"),
                _enabled || !ready ? ModMenuTheme.Notice : ModMenuTheme.Caption);
            PrepareRows();
            float leftWidth = Mathf.Floor(Mathf.Min(358, (area.width - 16) * .39f));
            Rect left = new Rect(area.x, area.y + 98, leftWidth, area.height - 98);
            Rect right = new Rect(left.xMax + 16, left.y, area.width - leftWidth - 16, left.height);
            DrawTimeline(left);
            return DrawDetails(right, ready && _enabled);
        }

        private static string CurrentDate()
        {
            var stat = PlayerStatManagerData.Instance;
            if (stat == null) return I18n.MenuT("dev.require_free");
            var time = stat.GameTime;
            int stage = (int)time.Stage;
            return I18n.MenuT("dev.current", time.Year, time.Month, stage >= 1 && stage <= 3 ? I18n.MenuT(StageKeys[stage - 1]) : "—");
        }

        private static void PrepareRows()
        {
            if (_rows == null)
            {
                _rows = new List<Dictionary<string, object>>();
                Run(() => _rows = VanillaBookmarkCapture.ReadTimelineRows());
                _catalog = new DevelopmentEventCatalog(_rows);
                SearchEntries.Clear();
                if (_selected != null)
                {
                    DevelopmentEvent match = null;
                    foreach (var row in _catalog.Entries)
                        if (row.IsMain == _selected.IsMain && row.Series == _selected.Series && row.Key == _selected.Key && row.Script == _selected.Script)
                        { match = row; break; }
                    _selected = match;
                }
            }
            int? round = null;
            if (_atCurrentDate && PlayerStatManagerData.Instance != null)
            {
                var time = PlayerStatManagerData.Instance.GameTime;
                if (DevelopmentNavigationPolicy.IsDate(time.Year, time.Month, (int)time.Stage))
                    round = DevelopmentEventCatalog.DateRound(time.Year, time.Month, (int)time.Stage);
            }
            string locale = I18n.MenuLocale;
            if (_filterLocale != locale) { _filterText = null; _filterLocale = locale; SearchEntries.Clear(); }
            if (_filteredCatalog == _catalog && _filterText == _search && _lastKind == _kind && _lastRound == round && _lastInactive == _includeInactive) return;
            _filteredCatalog = _catalog; _filterText = _search; _lastKind = _kind; _lastRound = round; _lastInactive = _includeInactive;
            _filtered = _catalog.Filter("", _kind, round, _includeInactive);
            if (!string.IsNullOrWhiteSpace(_search))
            {
                string[] terms = DevelopmentEventText.ToSimplified(_search).Split((char[])null, StringSplitOptions.RemoveEmptyEntries);
                _filtered.RemoveAll(row => !MatchesSearch(row, terms));
            }
            _scroll = Vector2.zero;
        }

        private static bool MatchesSearch(DevelopmentEvent row, string[] terms)
        {
            string text;
            if (!SearchEntries.TryGetValue(row, out text))
            {
                text = DevelopmentEventText.ToSimplified(string.Join(" ", row.Title, row.When, row.Series, row.Key, row.Script, row.Condition, row.ConditionSources, row.Place, DisplayWhen(row), DisplayCondition(row.Condition)));
                SearchEntries.Add(row, text);
            }
            foreach (string term in terms) if (text.IndexOf(term, StringComparison.OrdinalIgnoreCase) < 0) return false;
            return true;
        }

        private static string Display(string value) { return DevelopmentEventText.Display(value, I18n.MenuLocale); }

        private static string DisplayCondition(string value)
        {
            return Display(value).Replace(" ∧ ", " "+I18n.MenuT("dev.and")+" ").Replace(" ∨ ", " "+I18n.MenuT("dev.or")+" ")
                .Replace("旗标 ", I18n.MenuT("dev.flag_label")+" ").Replace("旗標 ", I18n.MenuT("dev.flag_label")+" ");
        }

        private static string DisplayRound(int round)
        { return I18n.MenuT("dev.calendar_date", round / 36 + 1, round % 36 / 3 + 1, I18n.MenuT(StageKeys[round % 3])); }

        private static string DisplayWhen(DevelopmentEvent row)
        {
            if (row.FromRound.HasValue && row.ToRound.HasValue && row.FromRound.Value >= 0 && row.ToRound.Value >= 0)
                return row.FromRound == row.ToRound ? DisplayRound(row.FromRound.Value)
                    : DisplayRound(row.FromRound.Value) + " ～ " + DisplayRound(row.ToRound.Value);
            if (row.FromRound.HasValue && row.FromRound.Value >= 0) return I18n.MenuT("dev.date_from", DisplayRound(row.FromRound.Value));
            if (row.ToRound.HasValue && row.ToRound.Value >= 0) return I18n.MenuT("dev.date_until", DisplayRound(row.ToRound.Value));
            if (row.When == "任意时间／条件触发") return I18n.MenuT("dev.any_condition_time");
            if (row.When == "时间不定") return I18n.MenuT("dev.unknown_time");
            if (row.When.StartsWith("每月", StringComparison.Ordinal))
            {
                string[] suffixes = { "上旬", "中旬", "下旬" };
                for (int i = 0; i < suffixes.Length; i++)
                    if (row.When.EndsWith(suffixes[i], StringComparison.Ordinal)) return I18n.MenuT("dev.monthly_stage", I18n.MenuT(StageKeys[i]));
            }
            return Display(row.When);
        }

        private static void DrawTimeline(Rect area)
        {
            GUI.Box(area, GUIContent.none, ModMenuTheme.Card);
            GUI.Label(new Rect(area.x + 16, area.y + 14, area.width - 116, 27), I18n.MenuT("dev.timeline"), ModMenuTheme.Heading);
            if (GUI.Button(new Rect(area.xMax - 94, area.y + 12, 78, 28), I18n.MenuT("dev.refresh"), ModMenuTheme.Link))
            { _rows = null; PrepareRows(); }
            Rect search = new Rect(area.x + 14, area.y + 51, area.width - 28, 36);
            GUI.SetNextControlName("modkit_event_search");
            string next = GUI.TextField(search, _search);
            if (_search.Length == 0 && GUI.GetNameOfFocusedControl() != "modkit_event_search")
                GUI.Label(new Rect(search.x + 11, search.y + 8, search.width - 22, 24), I18n.MenuT("dev.search_hint"), ModMenuTheme.Single);
            if (next != _search) { _search = next; PrepareRows(); }
            float segment = (area.width - 36) / 3;
            for (int i = 0; i < 3; i++)
                if (GUI.Button(new Rect(area.x + 14 + i * (segment + 4), area.y + 97, segment, 30), I18n.MenuT(KindKeys[i]),
                    _kind == i ? ModMenuTheme.SelectedRow : ModMenuTheme.Link)) { _kind = i; PrepareRows(); }
            float half = (area.width - 34) / 2;
            if (GUI.Button(new Rect(area.x + 14, area.y + 135, half, 28), new GUIContent(I18n.MenuT(_atCurrentDate ? "dev.current_period" : "dev.any_period"), I18n.MenuT("dev.range_hint")),
                _atCurrentDate ? ModMenuTheme.SelectedRow : ModMenuTheme.Link)) { _atCurrentDate = !_atCurrentDate; PrepareRows(); }
            if (GUI.Button(new Rect(area.x + 20 + half, area.y + 135, half, 28), new GUIContent(I18n.MenuT(_includeInactive ? "dev.include_inactive" : "dev.active_only"), I18n.MenuT("dev.table_status_hint")),
                _includeInactive ? ModMenuTheme.Link : ModMenuTheme.SelectedRow)) { _includeInactive = !_includeInactive; PrepareRows(); }
            Rect viewport = new Rect(area.x + 10, area.y + 174, area.width - 20, area.height - 224);
            if (_filtered.Count == 0)
            {
                GUI.Label(new Rect(viewport.x + 12, viewport.y + 24, viewport.width - 24, 72), I18n.MenuT("dev.no_matches"), ModMenuTheme.Muted);
                if (GUI.Button(new Rect(viewport.x + 12, viewport.y + 108, viewport.width - 24, 34), I18n.MenuT("dev.reset_filters")))
                { _search = ""; _kind = 0; _atCurrentDate = false; _includeInactive = true; PrepareRows(); GUI.FocusControl(null); }
            }
            else
            {
                MeasureRows(viewport.width - 18);
                _scroll = GUI.BeginScrollView(viewport, _scroll, new Rect(0, 0, viewport.width - 18, _rowsHeight), false, true);
                int first = 0;
                while (first + 1 < _filtered.Count && _rowTops[first + 1] < _scroll.y) first++;
                int last = first;
                while (last < _filtered.Count && _rowTops[last] < _scroll.y + viewport.height) last++;
                for (int i = first; i < last; i++)
                {
                    var row = _filtered[i];
                    Rect item = new Rect(0, _rowTops[i], viewport.width - 18, _rowTops[i + 1] - _rowTops[i] - 6);
                    bool selected = ReferenceEquals(row, _selected);
                    if (GUI.Button(item, new GUIContent("", Display(row.Title)), selected ? ModMenuTheme.SelectedRow : ModMenuTheme.Row))
                    { Select(row); GUI.FocusControl(null); }
                    if (selected) ModMenuTheme.Fill(new Rect(item.x, item.y + 7, 3, item.height - 14), ModMenuTheme.Accent);
                    float rowY = item.y + 10;
                    GUI.Label(new Rect(12, rowY, item.width - 24, _rowTitles[i]), Display(row.Title), ModMenuTheme.RowTitle); rowY += _rowTitles[i] + 3;
                    GUI.Label(new Rect(12, rowY, item.width - 24, _rowDates[i]), DisplayWhen(row), ModMenuTheme.Muted); rowY += _rowDates[i] + 2;
                    GUI.Label(new Rect(12, rowY, item.width - 24, _rowMeta[i]), RowMeta(row), ModMenuTheme.Caption);
                }
                GUI.EndScrollView();
            }
            ModMenuTheme.RuleAt(area.x + 14, area.yMax - 43, area.width - 28);
            GUI.Label(new Rect(area.x + 16, area.yMax - 33, area.width - 120, 24), I18n.MenuT("dev.filtered_count", _filtered.Count, _catalog.Entries.Count), ModMenuTheme.Caption);
            if (GUI.Button(new Rect(area.xMax - 96, area.yMax - 36, 80, 28), I18n.MenuT("dev.export"), ModMenuTheme.Link))
                Run(() => { VanillaBookmarkCapture.ExportTimeline(BepInEx.Paths.GameRootPath); _message = I18n.MenuT("dev.exported"); _messageIsError = false; });
        }

        private static string RowMeta(DevelopmentEvent row)
        { return I18n.MenuT(row.IsMain ? "dev.main" : "dev.side") + " · " + row.Series + "/" + row.Key + (row.Active ? "" : " · " + I18n.MenuT("dev.table_disabled")); }

        private static void MeasureRows(float width)
        {
            string locale = I18n.MenuLocale;
            if (ReferenceEquals(_measuredRows, _filtered) && _measuredWidth == width && _measuredLocale == locale) return;
            _measuredRows = _filtered; _measuredWidth = width; _measuredLocale = locale;
            _rowTops = new float[_filtered.Count + 1];
            _rowTitles = new float[_filtered.Count]; _rowDates = new float[_filtered.Count]; _rowMeta = new float[_filtered.Count];
            for (int i = 0; i < _filtered.Count; i++)
            {
                var row = _filtered[i];
                _rowTitles[i] = ModMenuTheme.LabelHeight(Display(row.Title), ModMenuTheme.RowTitle, width - 24);
                _rowDates[i] = ModMenuTheme.LabelHeight(DisplayWhen(row), ModMenuTheme.Muted, width - 24);
                _rowMeta[i] = ModMenuTheme.LabelHeight(RowMeta(row), ModMenuTheme.Caption, width - 24);
                _rowTops[i + 1] = _rowTops[i] + _rowTitles[i] + _rowDates[i] + _rowMeta[i] + 31;
            }
            _rowsHeight = _rowTops[_filtered.Count];
        }

        private static bool DrawDetails(Rect area, bool canJump)
        {
            GUI.Box(area, GUIContent.none, ModMenuTheme.Card);
            float tab = (area.width - 40) / 3;
            for (int i = 0; i < 3; i++)
                if (GUI.Button(new Rect(area.x + 14 + i * (tab + 6), area.y + 12, tab, 34), I18n.MenuT(PaneKeys[i]),
                    _pane == i ? ModMenuTheme.SelectedRow : GUI.skin.button)) ChangePane(i);
            ModMenuTheme.RuleAt(area.x + 14, area.y + 58, area.width - 28);
            bool jumpPane = _pane == 1;
            string status = _pane == 2 && !_messageIsError ? "" : _message;
            GUIStyle statusStyle = _messageIsError ? ModMenuTheme.Notice : ModMenuTheme.Muted;
            float statusHeight = status.Length == 0 ? 0 : ModMenuTheme.LabelHeight(status, statusStyle, area.width - 56) + 20;
            float footerHeight = (jumpPane ? 62 : 0) + statusHeight;
            Rect viewport = new Rect(area.x + 16, area.y + 72, area.width - 30, area.height - 86 - footerHeight);
            float width = viewport.width - 20;
            _detailScroll = GUI.BeginScrollView(viewport, _detailScroll, new Rect(0, 0, width, Mathf.Max(viewport.height, _detailHeight)), false, true);
            float y = 0;
            bool started = false;
            if (_pane == 0) DrawNodeDetails(width, ref y);
            else if (_pane == 1) DrawJump(width, ref y, canJump);
            else DrawHistory(width, ref y);
            _detailHeight = y + 14;
            GUI.EndScrollView();
            if (statusHeight > 0)
            {
                Rect notice = new Rect(area.x + 14, area.yMax - footerHeight - 9, area.width - 28, statusHeight);
                GUI.Box(notice, GUIContent.none, ModMenuTheme.Card);
                GUI.Label(new Rect(notice.x + 12, notice.y + 10, notice.width - 24, statusHeight - 20), status, statusStyle);
            }
            if (jumpPane)
            {
                ModMenuTheme.RuleAt(area.x + 14, area.yMax - 63, area.width - 28);
                bool previous = GUI.enabled;
                int year, month;
                bool valid = _jumpTab == 0 ? DraftDate(out year, out month) : VanillaStoryPatch.ValidTarget((_script ?? "").Trim());
                GUI.enabled = previous && canJump && valid;
                if (GUI.Button(new Rect(area.x + 16, area.yMax - 52, area.width - 32, 40),
                    I18n.MenuT(_jumpTab == 0 ? "dev.set_date" : "dev.goto_script"), ModMenuTheme.Primary))
                {
                    if (_jumpTab == 0) { DraftDate(out year, out month); started = Run(() => SetDate(year, month, _stage)); }
                    else started = Run(() => GoToScript(_script));
                }
                GUI.enabled = previous;
            }
            return started;
        }

        private static void ChangePane(int pane)
        { if (_pane != pane) { _pane = pane; _detailScroll = Vector2.zero; _detailHeight = 800; GUI.FocusControl(null); } }

        private static void Paragraph(ref float y, float width, string text, GUIStyle style, float gap = 10)
        {
            float height = Mathf.Max(style.lineHeight, ModMenuTheme.LabelHeight(text, style, width));
            GUI.Label(new Rect(0, y, width, height), text, style); y += height + gap;
        }

        private static void Field(ref float y, float width, string label, string value)
        {
            Paragraph(ref y, width, label, ModMenuTheme.Caption, 5);
            Paragraph(ref y, width, string.IsNullOrWhiteSpace(value) ? I18n.MenuT("dev.unspecified") : value, ModMenuTheme.Body, 16);
        }

        private static float Metadata(float x, float y, float width, string label, string value)
        {
            if (string.IsNullOrWhiteSpace(value)) value = I18n.MenuT("dev.unspecified");
            float labelHeight = ModMenuTheme.LabelHeight(label, ModMenuTheme.Caption, width);
            float valueHeight = ModMenuTheme.LabelHeight(value, ModMenuTheme.Body, width);
            GUI.Label(new Rect(x, y, width, labelHeight), label, ModMenuTheme.Caption);
            GUI.Label(new Rect(x, y + labelHeight + 4, width, valueHeight), value, ModMenuTheme.Body);
            return labelHeight + 4 + valueHeight + 14;
        }

        private static void MetadataPair(ref float y, float width, string leftLabel, string leftValue, string rightLabel, string rightValue)
        {
            float half = (width - 18) / 2;
            float leftHeight = Metadata(0, y, half, leftLabel, leftValue);
            float rightHeight = Metadata(half + 18, y, half, rightLabel, rightValue);
            y += Mathf.Max(leftHeight, rightHeight);
        }

        private static void DrawNodeDetails(float width, ref float y)
        {
            if (_selected == null)
            {
                y += 24;
                Paragraph(ref y, width, I18n.MenuT("dev.choose_node"), ModMenuTheme.Heading);
                Paragraph(ref y, width, I18n.MenuT("dev.choose_help"), ModMenuTheme.Muted, 22);
                if (GUI.Button(new Rect(0, y, width, 38), I18n.MenuT("dev.manual_jump"))) ChangePane(1);
                y += 46; return;
            }
            Paragraph(ref y, width, Display(_selected.Title), ModMenuTheme.Heading);
            if (I18n.MenuLocale == I18n.Ja || I18n.MenuLocale == I18n.Ko)
                Paragraph(ref y, width, I18n.MenuT("dev.original_notes"), ModMenuTheme.Caption);
            GUI.Label(new Rect(0, y, 112, 25), I18n.MenuT(_selected.IsMain ? "dev.main" : "dev.side"), ModMenuTheme.Chip);
            GUI.Label(new Rect(120, y, 132, 25), I18n.MenuT(_selected.Active ? "dev.table_enabled" : "dev.table_disabled"),
                _selected.Active ? ModMenuTheme.QuietChip : ModMenuTheme.ErrorChip);
            y += 39;
            if (GUI.Button(new Rect(0, y, (width - 8) / 2, 32), I18n.MenuT("dev.prepare_date"), ModMenuTheme.Link))
            { if (UseEventDate()) { _jumpTab = 0; ChangePane(1); } }
            if (GUI.Button(new Rect((width + 8) / 2, y, (width - 8) / 2, 32), I18n.MenuT("dev.prepare_script"), ModMenuTheme.Link))
            { _script = _selected.Script; _jumpTab = 1; ChangePane(1); }
            y += 48;
            MetadataPair(ref y, width, I18n.MenuT("dev.time_range"), DisplayWhen(_selected), I18n.MenuT("dev.place"), Display(_selected.Place));
            MetadataPair(ref y, width, I18n.MenuT("dev.mission_key"), _selected.Series + " / " + _selected.Key, I18n.MenuT("dev.script"), _selected.Script);
            Field(ref y, width, I18n.MenuT("dev.conditions"), DisplayCondition(_selected.Condition));
            if (_selected.ConditionSources.Length > 0)
            {
                Field(ref y, width, I18n.MenuT("dev.source_names"), _selected.ConditionSources);
                Paragraph(ref y, width, I18n.MenuT("dev.source_names_hint"), ModMenuTheme.Caption);
            }
            ModMenuTheme.RuleAt(0, y, width); y += 17;
            Paragraph(ref y, width, I18n.MenuT("dev.related"), ModMenuTheme.Heading);
            Related(ref y, width, "dev.previous", _catalog.Previous(_selected));
            var next = _catalog.Next(_selected);
            Related(ref y, width, "dev.next", next);
            if (next.Count == 0 && _selected.NextKey.Length > 0)
                Paragraph(ref y, width, I18n.MenuT("dev.next_unlisted", _selected.NextKey), ModMenuTheme.Caption);
            Paragraph(ref y, width, I18n.MenuT("dev.relations_hint"), ModMenuTheme.Caption, 18);
            if (GUI.Button(new Rect(0, y, width, 34), I18n.MenuT("dev.copy_location"), ModMenuTheme.Link))
            {
                GUIUtility.systemCopyBuffer = new StringBuilder().AppendLine(Display(_selected.Title)).AppendLine(DisplayWhen(_selected))
                    .AppendLine(_selected.Series + "/" + _selected.Key).AppendLine(_selected.Script)
                    .AppendLine(Display(_selected.Place)).AppendLine(DisplayCondition(_selected.Condition)).Append(_selected.ConditionSources).ToString();
                _message = I18n.MenuT("dev.copied");
                _messageIsError = false;
            }
            y += 42;
        }

        private static void Related(ref float y, float width, string label, List<DevelopmentEvent> rows)
        {
            Paragraph(ref y, width, I18n.MenuT(label), ModMenuTheme.Caption, 5);
            if (rows.Count == 0) { Paragraph(ref y, width, I18n.MenuT("dev.no_relation"), ModMenuTheme.Muted); return; }
            foreach (var row in rows)
            {
                string text = Display(row.Title) + " · " + row.Key;
                float height = Mathf.Max(36, ModMenuTheme.LabelHeight(text, ModMenuTheme.Link, width) + 10);
                if (GUI.Button(new Rect(0, y, width, height), new GUIContent(text, row.Script), ModMenuTheme.Link)) Select(row);
                y += height + 6;
            }
            y += 6;
        }

        private static bool UseEventDate()
        {
            int year, month, stage;
            if (_selected == null || !_selected.FromRound.HasValue
                || !DevelopmentEventCatalog.TryDate(_selected.FromRound.Value, out year, out month, out stage))
            { _message = I18n.MenuT("dev.no_exact_date"); _messageIsError = true; return false; }
            SetDraftDate(year, month, stage); _message = ""; _messageIsError = false; return true;
        }

        private static bool DraftDate(out int year, out int month)
        {
            month = 0;
            return int.TryParse(_year, NumberStyles.None, CultureInfo.InvariantCulture, out year)
                && int.TryParse(_month, NumberStyles.None, CultureInfo.InvariantCulture, out month)
                && DevelopmentNavigationPolicy.IsDate(year, month, _stage);
        }

        private static void SetDraftDate(int year, int month, int stage)
        { _year = year.ToString(CultureInfo.InvariantCulture); _month = month.ToString(CultureInfo.InvariantCulture); _stage = stage; _draftReady = true; }

        private static void DrawJump(float width, ref float y, bool canJump)
        {
            Paragraph(ref y, width, I18n.MenuT("dev.jump_intro"), ModMenuTheme.Muted, 10);
            float half = (width - 8) / 2;
            if (GUI.Button(new Rect(0, y, half, 36), I18n.MenuT("dev.date_section"), _jumpTab == 0 ? ModMenuTheme.SelectedRow : GUI.skin.button)) _jumpTab = 0;
            if (GUI.Button(new Rect(half + 8, y, half, 36), I18n.MenuT("dev.chapter_section"), _jumpTab == 1 ? ModMenuTheme.SelectedRow : GUI.skin.button)) _jumpTab = 1;
            y += 46;
            bool previous = GUI.enabled;
            if (_jumpTab == 0)
            {
                GUI.Label(new Rect(0, y, 74, 21), I18n.MenuT("dev.year"), ModMenuTheme.Caption);
                GUI.Label(new Rect(90, y, 74, 21), I18n.MenuT("dev.month"), ModMenuTheme.Caption); y += 22;
                _year = GUI.TextField(new Rect(0, y, 74, 38), _year, 3);
                _month = GUI.TextField(new Rect(90, y, 74, 38), _month, 3);
                GUI.Label(new Rect(182, y + 8, width - 182, 26), I18n.MenuT("dev.date_bounds"), ModMenuTheme.Single); y += 46;
                float segment = (width - 12) / 3;
                for (int i = 0; i < 3; i++)
                    if (GUI.Button(new Rect(i * (segment + 6), y, segment, 34), I18n.MenuT(StageKeys[i]), _stage == i + 1 ? ModMenuTheme.SelectedRow : GUI.skin.button)) _stage = i + 1;
                y += 42;
                int year, month;
                bool valid = DraftDate(out year, out month);
                int round = valid ? DevelopmentEventCatalog.DateRound(year, month, _stage) : -1;
                GUI.enabled = previous && valid && round > 0;
                if (GUI.Button(new Rect(0, y, segment, 32), I18n.MenuT("dev.previous_period"), ModMenuTheme.Link)) MoveDraft(round - 1);
                GUI.enabled = previous;
                if (GUI.Button(new Rect(segment + 6, y, segment, 32), I18n.MenuT("dev.use_current"), ModMenuTheme.Link))
                { var time = PlayerStatManagerData.Instance.GameTime; SetDraftDate(time.Year, time.Month, (int)time.Stage); }
                GUI.enabled = previous && valid && round < 323;
                if (GUI.Button(new Rect(2 * (segment + 6), y, segment, 32), I18n.MenuT("dev.next_period"), ModMenuTheme.Link)) MoveDraft(round + 1);
                GUI.enabled = previous; y += 40;
                if (_selected != null)
                {
                    if (GUI.Button(new Rect(0, y, width, 32), I18n.MenuT("dev.use_node_date"), ModMenuTheme.Link)) UseEventDate();
                    y += 40;
                }
                // Re-evaluate after quick-date buttons have changed the draft.
                valid = DraftDate(out year, out month);
                Paragraph(ref y, width, I18n.MenuT(valid ? "dev.date_hint" : "dev.invalid_date"), valid ? ModMenuTheme.Caption : ModMenuTheme.Notice, 18);
            }
            else
            {
                Paragraph(ref y, width, I18n.MenuT("dev.script"), ModMenuTheme.Caption, 6);
                _script = GUI.TextField(new Rect(0, y, width, 38), _script); y += 50;
                if (_selected != null)
                {
                    if (GUI.Button(new Rect(0, y, width, 32), I18n.MenuT("dev.use_node_script"), ModMenuTheme.Link)) _script = _selected.Script;
                    y += 45;
                }
                Paragraph(ref y, width, I18n.MenuT("dev.script_hint"), ModMenuTheme.Caption, 20);
            }
            GUI.enabled = previous;
            if (!canJump) Paragraph(ref y, width, I18n.MenuT("dev.enable_hint"), ModMenuTheme.Notice);
        }

        private static void MoveDraft(int round)
        { int year, month, stage; if (DevelopmentEventCatalog.TryDate(round, out year, out month, out stage)) SetDraftDate(year, month, stage); }

        private static void DrawHistory(float width, ref float y)
        {
            Paragraph(ref y, width, I18n.MenuT("dev.activity_hint"), ModMenuTheme.Muted, 18);
            if (_history.Count == 0) { Paragraph(ref y, width, I18n.MenuT("dev.no_activity"), ModMenuTheme.Body); return; }
            foreach (var record in _history)
            {
                string target = record.Script != null ? I18n.MenuT("dev.script_target", record.Script)
                    : I18n.MenuT("dev.date_target", record.Year, record.Month, I18n.MenuT(StageKeys[record.Stage - 1]));
                Paragraph(ref y, width, target, ModMenuTheme.Body, 5);
                Paragraph(ref y, width, OutcomeText(record), record.Failed ? ModMenuTheme.Notice : ModMenuTheme.Caption, 8);
                if (GUI.Button(new Rect(0, y, width, 30), I18n.MenuT("dev.fill_again"), ModMenuTheme.Link))
                {
                    if (record.Script != null) { _script = record.Script; _jumpTab = 1; }
                    else { SetDraftDate(record.Year, record.Month, record.Stage); _jumpTab = 0; }
                    ChangePane(1);
                }
                y += 46; ModMenuTheme.RuleAt(0, y, width); y += 18;
            }
        }
    }
}
