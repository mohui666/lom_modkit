using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    internal static partial class DevelopmentNavigationRuntime
    {
        private const int StoryPageSize = 12;
        private static string _storyQuery = "", _submittedQuery = "", _storyLocale;
        private static bool _searchDeaths;
        private static DevelopmentStoryIndex _storyIndex;
        private static TextAsset[] _storyAssets;
        private static int _indexedChapters, _storyPage;
        private static List<DevelopmentStoryIndex.Site> _storyHits;
        private static DevelopmentStoryIndex.Site _storyHit;
        private static List<DevelopmentEvent> _storyEventHits;

        private static void ResetStorySearch()
        {
            _storyIndex = null; _storyAssets = null; _storyLocale = null;
            _storyHits = null; _storyHit = null; _storyEventHits = null;
            _storyQuery = _submittedQuery = "";
            _searchDeaths = false; _indexedChapters = _storyPage = 0;
        }

        private static void SubmitStoryQuery()
        {
            _submittedQuery = _storyQuery.Trim();
            _storyHit = null; _storyPage = 0; _detailScroll = Vector2.zero;
            _storyHits = null; _storyEventHits = null;
            _message = ""; _messageIsError = false;
            if (!_searchDeaths && _submittedQuery.Length == 0) return;
            string locale = I18n.StoryLocale;
            if (_storyIndex == null || _storyLocale != locale)
            {
                Run(() =>
                {
                    // LuaManager.ExecuteLuaScript uses this Resources folder. Keep
                    // the installed resource names so a result can be opened directly.
                    var assets = Resources.LoadAll<TextAsset>("Story/ChineseTraditional");
                    if (assets.Length == 0) throw new InvalidOperationException(I18n.MenuT("dev.story_missing"));
                    var index = new DevelopmentStoryIndex();
                    var strings = LocalizationManager.Instance.LocaleResolver;
                    foreach (var item in LibrarySystem.Instance.Dead.List)
                    {
                        string title = strings.GetString(item.GetTitleKey());
                        string desc = strings.GetString(item.GetDescKey());
                        string picture = item.Picture == null ? "" : "\nCG · " + item.Picture.name;
                        index.AddDeath(item.name, (title + "\n" + desc).Trim() + picture);
                    }
                    _storyIndex = index;
                    _storyAssets = assets;
                    _storyLocale = locale;
                    _indexedChapters = 0;
                });
            }
            else if (_storyAssets == null) FindStoryHits();
        }

        private static void ReadStoryBatch()
        {
            var strings = LocalizationManager.Instance.LocaleResolver;
            long deadline = Stopwatch.GetTimestamp() + Stopwatch.Frequency / 125;
            do
            {
                var chapter = _storyAssets[_indexedChapters++];
                _storyIndex.AddChapter(chapter.name, chapter.text, strings.GetStoryText);
            } while (_indexedChapters < _storyAssets.Length && Stopwatch.GetTimestamp() < deadline);
            if (_indexedChapters < _storyAssets.Length) return;
            _storyAssets = null;
            if (_searchDeaths || _submittedQuery.Length != 0) FindStoryHits();
        }

        private static void FindStoryHits()
        {
            string[] terms = DevelopmentEventText.ToSimplified(_submittedQuery).Split((char[])null, StringSplitOptions.RemoveEmptyEntries);
            _storyHits = _storyIndex.Search(terms, _searchDeaths);
            _storyEventHits = _searchDeaths ? new List<DevelopmentEvent>()
                : _catalog.Entries.Where(row => MatchesSearch(row, terms)).ToList();
        }

        private static void DrawStorySearch(float width, ref float y)
        {
            if (_storyHit != null)
            {
                DrawStoryHit(width, ref y);
                return;
            }
            float half = (width - 8) / 2;
            if (GUI.Button(new Rect(0, y, half, 34), I18n.MenuT("dev.story_dialogue"), !_searchDeaths ? ModMenuTheme.SelectedRow : GUI.skin.button) && _searchDeaths)
            { _searchDeaths = false; SubmitStoryQuery(); }
            if (GUI.Button(new Rect(half + 8, y, half, 34), I18n.MenuT("dev.story_death"), _searchDeaths ? ModMenuTheme.SelectedRow : GUI.skin.button) && !_searchDeaths)
            { _searchDeaths = true; SubmitStoryQuery(); }
            y += 44;
            GUI.SetNextControlName("modkit_story_search");
            bool enter = Event.current.type == EventType.KeyDown
                && (Event.current.keyCode == KeyCode.Return || Event.current.keyCode == KeyCode.KeypadEnter)
                && GUI.GetNameOfFocusedControl() == "modkit_story_search";
            if (enter) Event.current.Use();
            _storyQuery = GUI.TextField(new Rect(0, y, width - 84, 36), _storyQuery, 256);
            if (GUI.Button(new Rect(width - 76, y, 76, 36), I18n.MenuT("dev.story_submit")) || enter)
                SubmitStoryQuery();
            y += 46;
            if (_storyAssets != null)
            {
                if (Event.current.type == EventType.Repaint && !Run(ReadStoryBatch))
                { _storyAssets = null; _storyIndex = null; }
                if (_storyAssets != null)
                {
                    Paragraph(ref y, width, I18n.MenuT("dev.story_loading", _indexedChapters, _storyAssets.Length), ModMenuTheme.Caption);
                    return;
                }
            }
            if (_storyHits == null)
            {
                Paragraph(ref y, width, I18n.MenuT("dev.story_query_hint"), ModMenuTheme.Caption);
                return;
            }
            int hits = _storyHits.Count + _storyEventHits.Count;
            if (hits == 0)
            {
                Paragraph(ref y, width, I18n.MenuT("dev.story_no_hits"), ModMenuTheme.Muted);
                return;
            }
            int pages = (hits + StoryPageSize - 1) / StoryPageSize;
            GUI.Label(new Rect(0, y + 3, width - 106, 25), I18n.MenuT("dev.story_count", hits, _storyPage + 1, pages), ModMenuTheme.Caption);
            if (GUI.Button(new Rect(width - 96, y, 44, 28), "‹", ModMenuTheme.Link) && _storyPage > 0) _storyPage--;
            if (GUI.Button(new Rect(width - 44, y, 44, 28), "›", ModMenuTheme.Link) && _storyPage + 1 < pages) _storyPage++;
            y += 38;
            for (int i = _storyPage * StoryPageSize; i < Math.Min(hits, (_storyPage + 1) * StoryPageSize); i++)
            {
                if (i < _storyEventHits.Count)
                {
                    var row = _storyEventHits[i];
                    if (SearchRow(width, ref y, Display(row.Title), row.Script + " · " + DisplayWhen(row))) Select(row);
                    continue;
                }
                var hit = _storyHits[i - _storyEventHits.Count];
                string location = hit.Chapter.Length == 0 ? hit.Key : hit.Chapter + ":" + hit.Line + "    " + hit.Key;
                string text = hit.Text.Replace('\n', ' ');
                if (text.Length > 64) text = text.Substring(0, 64) + "…";
                if (SearchRow(width, ref y, text, location))
                { _storyHit = hit; _detailScroll = Vector2.zero; }
            }
        }

        private static bool SearchRow(float width, ref float y, string title, string location)
        {
            var rect = new Rect(0, y, width, 63);
            bool clicked = GUI.Button(rect, GUIContent.none, ModMenuTheme.Row);
            GUI.Label(new Rect(12, y + 8, width - 24, 30), title, ModMenuTheme.RowTitle);
            GUI.Label(new Rect(12, y + 41, width - 24, 20), location, ModMenuTheme.Caption);
            y += 70;
            return clicked;
        }

        private static void DrawStoryHit(float width, ref float y)
        {
            var hit = _storyHit;
            if (GUI.Button(new Rect(0, y, width, 32), I18n.MenuT("dev.story_back"), ModMenuTheme.Link))
            { _storyHit = null; _detailScroll = Vector2.zero; }
            y += 41;
            string location = hit.Chapter.Length == 0 ? hit.Key : hit.Chapter + ":" + hit.Line + "    " + hit.Key;
            Field(ref y, width, location, hit.Text);
            if (hit.Chapter.Length == 0)
            {
                Paragraph(ref y, width, I18n.MenuT("dev.story_death_unlinked"), ModMenuTheme.Muted);
                return;
            }
            float half = (width - 8) / 2;
            if (GUI.Button(new Rect(0, y, half, 36), I18n.MenuT("dev.copy_location"), ModMenuTheme.Link))
            {
                GUIUtility.systemCopyBuffer = location.TrimEnd() + "\n" + hit.Text;
                _message = I18n.MenuT("dev.copied"); _messageIsError = false;
            }
            if (GUI.Button(new Rect(half + 8, y, half, 36), I18n.MenuT("dev.prepare_script"), ModMenuTheme.Link))
                PrepareSearchChapter(hit.Chapter);
            y += 48;
            string context = string.Join("\n\n", _storyIndex.Context(hit).Where(site => site != hit).Select(site => site.Line + "  " + site.Text));
            if (context.Length != 0) Field(ref y, width, I18n.MenuT("dev.story_context"), context);
            if (_searchDeaths)
            {
                string pictures = string.Join("\n", _storyIndex.Pictures(hit.Chapter).Select(site => site.Line + "  " + site.Key));
                if (pictures.Length != 0) Field(ref y, width, I18n.MenuT("dev.story_same_chapter_pictures"), pictures);
            }
            var rows = _catalog.Entries.Where(row => row.Script.Equals(hit.Chapter, StringComparison.OrdinalIgnoreCase)).ToList();
            if (rows.Count != 0) Related(ref y, width, "dev.story_linked_events", rows);
            DrawLinkedChapters(width, ref y, hit.Chapter, true);
            DrawLinkedChapters(width, ref y, hit.Chapter, false);
        }

        private static void DrawLinkedChapters(float width, ref float y, string chapter, bool predecessors)
        {
            var linked = _storyIndex.Linked(chapter, predecessors).ToList();
            if (linked.Count == 0) return;
            Paragraph(ref y, width, I18n.MenuT(predecessors ? "dev.story_callers" : "dev.story_targets"), ModMenuTheme.Caption, 6);
            foreach (string other in linked)
            {
                if (GUI.Button(new Rect(0, y, width, 28), other, ModMenuTheme.Link)) PrepareSearchChapter(other);
                y += 33;
            }
        }

        private static void PrepareSearchChapter(string chapter)
        {
            var rows = _catalog.Entries.Where(row => row.Script.Equals(chapter, StringComparison.OrdinalIgnoreCase)).Take(2).ToList();
            if (rows.Count == 1) Select(rows[0]);
            PrepareChapter(chapter, rows.Count == 1 ? rows[0] : null);
            _jumpTab = 1;
            ChangePane(JumpPane);
        }
    }
}
