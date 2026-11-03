using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using BepInEx;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.InputSystem;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// MOD 专用存档浏览器。只显示当前模式/作品的20个手动槽与3个自动槽；
    /// 保存与读取仍调用 SaveSystem 的实际接口，不创建另一套游戏进度格式。
    /// </summary>
    internal sealed class ModSaveBrowser : MonoBehaviour
    {
        private sealed class Entry
        {
            internal string Slot, Label, AutoKind, Chapter, Lines;
            internal int Index;
            internal GameSave Data;
            internal ModSaveNote Note;
        }

        private static readonly Color Ink = new Color(.055f, .064f, .064f, 1f);
        private static readonly Color Card = new Color(.095f, .108f, .105f, 1f);
        private static readonly Color Selected = new Color(.17f, .18f, .15f, 1f);
        private static readonly Color Gold = new Color(.83f, .72f, .49f, 1f);
        private static readonly Color Paper = new Color(.95f, .93f, .86f, 1f);
        private static readonly Color Muted = new Color(.69f, .73f, .70f, 1f);
        private static readonly Color Rule = new Color(.28f, .31f, .28f, 1f);
        private static readonly Color Danger = new Color(.82f, .45f, .39f, 1f);
        private static Font _font;

        internal static ModSaveBrowser Current { get; private set; }
        internal string CampaignId { get { return _package != null ? _package.CampaignId : null; } }
        internal bool Saving { get { return _saving; } }

        private bool _saving, _addon, _fromTitle, _auto, _favorites, _recent = true;
        private bool _closing, _busy, _resetScroll;
        private CommonPanel _host;
        private MonoBehaviour _owner;
        private bool _ownerEnabled;
        private InputButton _hostClose;
        private GameObject _previousFocus;
        private readonly List<ModPackage> _packages = new List<ModPackage>();
        private readonly List<Entry> _entries = new List<Entry>();
        private ModPackage _package;
        private Entry _selected;
        private Action<ModPackage, string> _start, _load, _loadAuto;
        private Action<string> _campaignSelected;
        private string _recentCampaign;
        private RectTransform _pane, _rows, _detail, _confirm;
        private ScrollRect _scroll;
        private Text _eyebrow, _title, _subtitle, _status;
        private InputField _search, _note;
        private Button _primary, _delete, _favorite;
        private readonly List<Button> _tabs = new List<Button>();
        private Action _confirmation;

        private static string NotesRoot
        {
            get { return Path.Combine(Paths.ConfigPath, "MortalModHost", "save_notes"); }
        }

        internal static void ShowModes(Action original, Action addon, Action campaign)
        {
            if (original == null) throw new ArgumentNullException(nameof(original));
            if (addon == null) throw new ArgumentNullException(nameof(addon));
            if (campaign == null) throw new ArgumentNullException(nameof(campaign));
            ModNativeSaveBrowser.Close();
            ModSaveBrowser browser = Create(false, false, true, null, null);
            browser.gameObject.name = "MortalModHost_PlayModePicker";
            browser._eyebrow.text = "活侠传  /  开始冒险";
            browser._title.text = "开始游戏";
            browser._subtitle.text = "选择这次冒险的玩法。";
            string[] titles = { "纯原版", "原版＋MOD", "纯MOD" };
            string[] categories = { "原本的江湖", "为原作添一章", "另一段故事" };
            string[] descriptions =
            {
                "体验游戏原本的故事与规则。\n\n不启用 MOD，使用原版存档。",
                "原版主线，加入附加剧情。\n\n使用独立的「原版＋MOD」存档。",
                "先选择作品，再开始独立故事。\n\n每部作品分别保存进度。"
            };
            string[] names = { "Original", "Addon", "Campaign" };
            Action[] callbacks = { original, addon, campaign };
            Button first = null;
            for (int i = 0; i < titles.Length; i++)
            {
                Action navigate = callbacks[i];
                Button card = Button(browser._pane, "", 32 + i * 478, 190, 460, 438, delegate
                {
                    if (browser._busy) return;
                    browser._busy = true;
                    browser.Close();
                    navigate();
                });
                card.name = "ModeChoice_" + names[i];
                if (first == null) first = card;
                Frame((RectTransform)card.transform);
                Label(card.transform, categories[i], 17, Gold, 28, 28, 402, 28);
                Label(card.transform, titles[i], 34, Paper, 28, 84, 402, 54);
                Label(card.transform, descriptions[i], 23, Muted, 28, 172, 402, 166);
                Line(card.transform, 28, 359, 402);
                Label(card.transform, "选择  →", 24, Gold, 28, 384, 402, 38);
            }
            Label(browser._pane, "三种玩法的进度分别保存，互不覆盖。", 20, Muted, 34, 672, 1380, 40);
            browser._status.text = "先选玩法，再选择作品或存档。";
            if (first != null) first.Select();
        }

        internal static void ShowCampaigns(IList<ModPackage> packages,
            Action<ModPackage, string> start, Action<ModPackage, string> load,
            Action<ModPackage, string> auto, string recent, string preferred,
            Action<string> selected, bool title, CommonPanel host, MonoBehaviour owner)
        {
            ModSaveBrowser browser = Create(false, false, title, host, owner);
            browser._start = start;
            browser._load = load;
            browser._loadAuto = auto;
            browser._campaignSelected = selected;
            browser._recentCampaign = recent;
            if (packages != null)
                browser._packages.AddRange(packages.Where(x => x != null && x.Campaign != null
                    && x.Campaign.NewGame && x.PlayMode != "addon")
                    .OrderBy(x => x.Name, StringComparer.OrdinalIgnoreCase));
            browser._package = browser._packages.FirstOrDefault(x => x.CampaignId == preferred);
            browser.Render();
        }

        internal static void ShowAddon(bool saving, bool title, CommonPanel host, MonoBehaviour owner)
        {
            Create(saving, true, title, host, owner).Render();
        }

        internal static void ShowSave(SaveGamePanel owner, ModPackage package, bool addon)
        {
            ModSaveBrowser browser = Create(true, addon, false,
                owner.GetComponentInParent<CommonPanel>(), owner);
            browser._package = package;
            browser.Render();
        }

        private static ModSaveBrowser Create(bool saving, bool addon, bool title,
            CommonPanel host, MonoBehaviour owner)
        {
            if (Current != null) Current.Close();
            var root = new GameObject("MortalModHost_SaveBrowser", typeof(RectTransform),
                typeof(Canvas), typeof(CanvasScaler), typeof(GraphicRaycaster));
            var browser = root.AddComponent<ModSaveBrowser>();
            Current = browser;
            browser._saving = saving;
            browser._addon = addon;
            browser._fromTitle = title;
            browser._host = host;
            browser._owner = owner;
            browser._previousFocus = EventSystem.current != null
                ? EventSystem.current.currentSelectedGameObject : null;
            if (owner != null)
            {
                browser._ownerEnabled = owner.enabled;
                owner.enabled = false;
                string field = owner is SaveGamePanel ? "_closeInputButton" : "_closeButton";
                browser._hostClose = AccessTools.Field(owner.GetType(), field)?.GetValue(owner) as InputButton;
                if (browser._hostClose != null) browser._hostClose.SetActive(false);
            }
            Canvas canvas = root.GetComponent<Canvas>();
            canvas.renderMode = RenderMode.ScreenSpaceOverlay;
            canvas.sortingOrder = 31000;
            CanvasScaler scaler = root.GetComponent<CanvasScaler>();
            scaler.uiScaleMode = CanvasScaler.ScaleMode.ScaleWithScreenSize;
            scaler.referenceResolution = new Vector2(1600, 900);
            scaler.matchWidthOrHeight = .5f;
            browser.Build();
            return browser;
        }

        private void Build()
        {
            // Match Coffee's explicit body-font source: a save slot's time label,
            // rather than an arbitrary Text (which may use a calligraphy title font).
            Font saveFont = FindSaveFont();
            if (saveFont != null) _font = saveFont;
            if (_font == null)
                _font = Font.CreateDynamicFontFromOSFont(new[] { "Microsoft YaHei", "SimSun" }, 24);
            RectTransform backdrop = Box(transform, "Shade", new Color(0, 0, 0, .88f));
            Stretch(backdrop);
            _pane = Box(transform, "Panel", Ink);
            _pane.anchorMin = _pane.anchorMax = new Vector2(.5f, .5f);
            _pane.pivot = new Vector2(.5f, .5f);
            _pane.sizeDelta = new Vector2(1480, 800);
            _pane.anchoredPosition = Vector2.zero;
            Frame(_pane);
            _eyebrow = Label(_pane, "MOD  /  江湖手札", 15, Gold, 32, 23, 430, 24);
            _title = Label(_pane, "", 34, Paper, 32, 54, 1020, 48);
            _title.resizeTextForBestFit = true;
            _title.resizeTextMinSize = 22;
            _title.resizeTextMaxSize = 34;
            _subtitle = Label(_pane, "", 17, Muted, 34, 105, 1150, 28);
            Button close = Button(_pane, "关闭  Esc", 1306, 40, 140, 48, Close);
            close.name = "CloseSaveBrowser";
            Line(_pane, 32, 143, 1416);
            _status = Label(_pane, "", 17, Muted, 34, 755, 1130, 26);
            Label(_pane, "Enter 确认 · Esc 返回", 15, Muted, 1180, 757, 264, 24)
                .alignment = TextAnchor.MiddleRight;
        }

        private void Render()
        {
            ClearArea();
            _selected = null;
            _title.text = _addon ? "原版＋MOD · " + (_saving ? "保存进度" : "江湖存档")
                : _package == null ? "纯MOD · 选择作品"
                : ModDisclosurePolicy.SafePackageName(_package) + " · " + (_saving ? "保存进度" : "江湖存档");
            _subtitle.text = _addon ? "原版故事，加入附加剧情 · 这里的进度与纯原版分开保存"
                : _package == null ? "选择一部作品，查看它的存档或开始新的冒险。"
                : "纯MOD独立剧本 · " + ModDisclosurePolicy.SafePackageAuthor(_package)
                    + "  /  v" + (_package.Version ?? "");
            if (!_addon && _package == null)
            {
                BuildSearch();
                BuildList();
                BuildDetailPane();
                RenderCampaigns();
                Label(_detail, "各自的故事，各自的进度", 27, Paper, 28, 38, 570, 46);
                Label(_detail, "选择左侧作品即可查看它的手动和自动存档。\n\n新开一局会使用空存档，不会覆盖已有进度。",
                    21, Muted, 28, 111, 570, 180);
                _status.text = _packages.Count == 0 ? "尚未安装独立剧本。请先在编辑器安装作品。" : "共 " + _packages.Count + " 部作品";
                return;
            }
            BuildTabs();
            BuildSearch();
            BuildList();
            BuildDetailPane();
            if (_fromTitle && !_saving)
            {
                Button fresh = Button(_pane, "＋ 新开一局", _addon ? 1190 : 1008, 98,
                    _addon ? 254 : 170, 34, delegate
                {
                    SaveNote();
                    _auto = _favorites = false;
                    _search.text = "";
                    RefreshEntries();
                    Entry empty = _entries.FirstOrDefault(x => x.Data == null && x.AutoKind == null);
                    if (empty == null)
                    {
                        _status.text = "手动栏位已满。请先确认并删除不需要的进度，再新开一局。";
                        return;
                    }
                    _selected = empty;
                    RenderRows();
                    int index = _entries.IndexOf(empty);
                    _scroll.verticalNormalizedPosition = Mathf.Clamp01(1 - index * 116f
                        / Math.Max(1, _rows.sizeDelta.y - 454));
                    Primary();
                });
                fresh.name = "BodyNewGame";
            }
            if (!_addon && !_saving && _fromTitle)
            {
                Button back = Button(_pane, "← 返回作品", 1190, 98, 254, 34, delegate
                {
                    SaveNote();
                    _package = null;
                    _auto = _favorites = false;
                    Render();
                });
                back.name = "BodyBack";
            }
            RefreshEntries();
        }

        private void ClearArea()
        {
            foreach (Transform child in _pane)
                if (child.name.StartsWith("Body", StringComparison.Ordinal)) Destroy(child.gameObject);
            _tabs.Clear();
            _rows = _detail = null;
            _search = _note = null;
            _primary = _delete = _favorite = null;
        }

        private void BuildTabs()
        {
            string[] labels = _saving ? new[] { "手动存档", "收藏" } : new[] { "手动存档", "自动存档", "收藏" };
            for (int i = 0; i < labels.Length; i++)
            {
                int tab = i;
                Button button = Button(_pane, labels[i], 32 + i * 124, 160, 116, 42, delegate
                {
                    _favorites = tab == (_saving ? 1 : 2);
                    _auto = !_saving && tab == 1;
                    RefreshEntries();
                });
                button.name = "BodyTab" + i;
                _tabs.Add(button);
            }
            Button sort = Button(_pane, "最近保存 ↓", 558, 160, 204, 42, delegate
            {
                _recent = !_recent;
                RefreshEntries();
            });
            sort.name = "BodySort";
        }

        private void BuildSearch()
        {
            _search = Input(_pane, !_addon && _package == null ? "搜索作品名称或简介…"
                : "搜索备注、日期或章节…", 32, 216, 730, 43, false);
            _search.name = "BodySearch";
            _search.characterLimit = 80;
            _search.onValueChanged.AddListener(delegate { RenderRows(); });
        }

        private void BuildList()
        {
            var viewport = Box(_pane, "BodyListViewport", new Color(0, 0, 0, .12f));
            Place(viewport, 32, 275, 730, 454);
            viewport.gameObject.AddComponent<RectMask2D>();
            _scroll = viewport.gameObject.AddComponent<ScrollRect>();
            _scroll.horizontal = false;
            _scroll.vertical = true;
            _scroll.viewport = viewport;
            _scroll.movementType = ScrollRect.MovementType.Clamped;
            _rows = Rect(viewport, "Rows");
            Place(_rows, 0, 0, 730, 0);
            _scroll.content = _rows;
        }

        private void BuildDetailPane()
        {
            _detail = Box(_pane, "BodyDetail", new Color(.075f, .087f, .082f, 1f));
            Place(_detail, 790, 160, 654, 570);
            Frame(_detail);
        }

        private void RenderCampaigns()
        {
            ClearRows();
            string query = _search != null ? _search.text.Trim() : "";
            int i = 0;
            Button first = null;
            foreach (ModPackage package in _packages)
            {
                string name = ModDisclosurePolicy.SafePackageName(package);
                string description = ModDisclosurePolicy.SafePackageDescription(package);
                if (!Contains(name + " " + description, query)) continue;
                ModPackage captured = package;
                Button row = Button(_rows, "", 0, i * 124, 714, 112, delegate
                {
                    _package = captured;
                    _campaignSelected?.Invoke(captured.CampaignId);
                    if (ModNativeSaveBrowser.TryOpenTitle(captured, false, _start, _load, _loadAuto))
                    {
                        Close(false);
                        return;
                    }
                    _auto = _favorites = false;
                    Render();
                });
                row.name = "CampaignEntry_" + package.CampaignId;
                if (first == null) first = row;
                Label(row.transform, name, 25, Paper, 20, 14, 665, 36);
                Label(row.transform, string.IsNullOrEmpty(description) ? "点击查看存档 →" : description,
                    18, Muted, 20, 58, 665, 42);
                if (package.CampaignId == _recentCampaign)
                    Label(row.transform, "最近游玩", 15, Gold, 562, 20, 122, 26).alignment = TextAnchor.MiddleRight;
                i++;
            }
            _rows.sizeDelta = new Vector2(730, Math.Max(454, i * 124));
            if (i == 0) Label(_rows, "没有找到作品", 22, Muted, 24, 40, 640, 42);
            if (first != null && !InputHasFocus()) first.Select();
        }

        private void RefreshEntries()
        {
            string prior = _selected != null ? _selected.Slot : null;
            _entries.Clear();
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null) throw new InvalidOperationException("游戏存档系统尚未就绪");
            bool autos = _auto && !_saving;
            bool allKinds = _favorites && !_saving;
            int count = allKinds ? ModSaveSlotPolicy.OfficialManualCount + 3
                : autos ? 3 : ModSaveSlotPolicy.OfficialManualCount;
            string[] kinds = { "auto", "auto_free", "auto_battle" };
            string[] labels = { "剧情自动存档", "养成自动存档", "战斗自动存档" };
            for (int i = 0; i < count; i++)
            {
                bool automatic = autos || (allKinds && i >= ModSaveSlotPolicy.OfficialManualCount);
                int index = automatic ? (allKinds ? i - ModSaveSlotPolicy.OfficialManualCount : i) : i;
                string kind = automatic ? kinds[index] : null;
                string slot = automatic
                    ? (_addon ? ModSaveSlotPolicy.AddonAutoSlot(kind)
                        : ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(CampaignId, kind))
                    : (_addon ? ModSaveSlotPolicy.AddonManualSlot(i + 1)
                        : ModSaveSlotPolicy.IsolatedManualSlot(CampaignId, i + 1));
                GameSave data = automatic ? saves.GetAutoSaveData(slot)?.GameSave : saves.GetSaveData(slot);
                var entry = new Entry
                {
                    Slot = slot, AutoKind = kind, Index = i + 1, Data = data,
                    Label = automatic ? labels[index] : "手动 " + (i + 1).ToString("000"),
                    Note = data != null ? ReadNote(slot) : new ModSaveNote()
                };
                entry.Chapter = ChapterText(data);
                entry.Lines = RecentLines(data);
                _entries.Add(entry);
            }
            if (_recent)
            {
                _entries.Sort(delegate(Entry a, Entry b)
                {
                    if (_saving && (a.Data == null) != (b.Data == null)) return a.Data == null ? -1 : 1;
                    int recent = (b.Data != null ? b.Data.TimeTick : 0).CompareTo(a.Data != null ? a.Data.TimeTick : 0);
                    return recent != 0 ? recent : a.Index.CompareTo(b.Index);
                });
            }
            _selected = _entries.FirstOrDefault(x => x.Slot == prior)
                ?? _entries.FirstOrDefault(x => _saving ? x.Data == null : x.Data != null)
                ?? _entries.FirstOrDefault();
            // Content is still zero-height on first open. Do not read back the
            // ScrollRect's normalized value until the new rows have been laid out.
            _resetScroll = true;
            RenderRows();
            foreach (Button tab in _tabs)
            {
                string label = tab.GetComponentInChildren<Text>().text;
                bool active = _favorites ? label == "收藏" : _auto ? label == "自动存档" : label == "手动存档";
                tab.image.color = active ? Selected : Card;
            }
            Transform sort = _pane.Find("BodySort");
            if (sort != null) sort.GetComponentInChildren<Text>().text = _recent ? "最近保存 ↓" : "按栏位排序";
            _status.text = "手动 20 栏 · 自动 3 栏 · " + (_addon ? "原版＋MOD专属进度" : "仅此作品的进度");
            _status.color = Muted;
        }

        private ModSaveNote ReadNote(string slot)
        {
            try { return ModSaveNotes.Read(NotesRoot, CampaignId, _addon, slot); }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogWarning("读取存档备注失败：" + ex.Message);
                return new ModSaveNote();
            }
        }

        private void RenderRows()
        {
            if (_rows == null) return;
            if (!_addon && _package == null) { RenderCampaigns(); return; }
            float scroll = _resetScroll ? 1f : _scroll.verticalNormalizedPosition;
            ClearRows();
            string query = _search != null ? _search.text.Trim() : "";
            List<Entry> visible = _entries.Where(entry =>
                (!_favorites || entry.Note.Favorite)
                && Contains(entry.Label + " " + entry.Note.Text + " " + entry.Chapter
                    + " " + GameDate(entry.Data) + " " + SaveDate(entry.Data), query)).ToList();
            if (!visible.Contains(_selected)) _selected = visible.FirstOrDefault();
            int i = 0;
            Button selectedButton = null;
            foreach (Entry entry in visible)
            {
                Entry captured = entry;
                Button row = Button(_rows, "", 0, i * 116, 714, 104, delegate
                {
                    if (_busy || ReferenceEquals(captured, _selected)) return;
                    SaveNote();
                    _selected = captured;
                    RenderRows();
                });
                row.name = "SaveEntry_" + entry.Slot;
                row.gameObject.AddComponent<ModSaveSubmit>().Action = delegate
                {
                    if (_busy) return;
                    _selected = captured;
                    Primary();
                };
                if (ReferenceEquals(entry, _selected))
                {
                    selectedButton = row;
                    row.image.color = Selected;
                    RectTransform marker = Box(row.transform, "Selected", Gold);
                    Place(marker, 0, 0, 4, 104);
                }
                string state = entry.Note.Favorite ? "★ " : "";
                if (SaveSystem.Instance.CurrentSlot == entry.Slot) state += "当前 · ";
                Label(row.transform, state + entry.Label, 22, entry.Data != null ? Paper : Muted, 20, 10, 330, 32);
                Label(row.transform, entry.Data == null ? "空存档" : GameDate(entry.Data), 19, Gold, 356, 12, 335, 30)
                    .alignment = TextAnchor.MiddleRight;
                Label(row.transform, entry.Data == null ? (_fromTitle ? "可以从这里开始新的冒险" : "还没有保存进度")
                    : SaveDate(entry.Data) + "   ·   " + entry.Chapter, 16, Muted, 20, 46, 670, 23);
                Label(row.transform, entry.Note.Text.Length == 0 ? "暂无备注" : entry.Note.Text,
                    17, entry.Note.Text.Length == 0 ? Muted : Paper, 20, 72, 670, 24);
                i++;
            }
            _rows.sizeDelta = new Vector2(730, Math.Max(454, i * 116));
            if (i == 0) Label(_rows, _favorites ? "还没有收藏的存档" : "没有找到匹配的存档", 22, Muted, 24, 40, 640, 42);
            Canvas.ForceUpdateCanvases();
            _scroll.verticalNormalizedPosition = scroll;
            if (_resetScroll) _scroll.velocity = Vector2.zero;
            _resetScroll = false;
            RenderDetails();
            if (selectedButton != null && !InputHasFocus()) selectedButton.Select();
        }

        private void ClearRows()
        {
            foreach (Transform child in _rows) Destroy(child.gameObject);
        }

        private void RenderDetails()
        {
            if (_detail == null) return;
            foreach (Transform child in _detail)
                if (!child.name.StartsWith("Border", StringComparison.Ordinal)) Destroy(child.gameObject);
            _note = null;
            Entry entry = _selected;
            if (entry == null)
            {
                _primary = _delete = _favorite = null;
                Label(_detail, "选择一份存档", 27, Paper, 28, 38, 570, 46);
                Label(_detail, "存档详情、对白和备注会显示在这里。", 21, Muted, 28, 111, 570, 120);
                return;
            }
            Label(_detail, entry.Label, 27, Gold, 26, 22, 430, 42);
            _favorite = Button(_detail, entry.Note.Favorite ? "★ 已收藏" : "☆ 收藏", 480, 24, 148, 40, delegate
            {
                if (_selected == null || _selected.Data == null) return;
                _selected.Note.Favorite = !_selected.Note.Favorite;
                SaveNote();
                RenderRows();
            });
            _favorite.interactable = entry.Data != null;
            Label(_detail, entry.Data != null ? GameDate(entry.Data) : "新的冒险，从这里开始", 25, Paper, 26, 82, 574, 42);
            Label(_detail, entry.Data != null ? "保存于 " + SaveDate(entry.Data) : "此栏位还没有进度。", 17, Muted, 28, 132, 584, 28);
            Label(_detail, entry.Data != null ? entry.Chapter : (_saving ? "保存后可以从这里继续。" : "不会覆盖其他栏位。"),
                19, Muted, 28, 169, 588, 32);
            Line(_detail, 26, 218, 600);
            Label(_detail, "存档时的对白", 16, Gold, 28, 233, 594, 27);
            Text lines = Label(_detail, entry.Data == null ? "尚未留下故事" : entry.Lines,
                21, Paper, 28, 272, 594, 134);
            lines.verticalOverflow = VerticalWrapMode.Truncate;
            _note = Input(_detail, "备注 · 离开输入框时自动保存", 26, 423, 602, 57, true);
            _note.characterLimit = ModSaveNotes.MaxLength;
            _note.interactable = entry.Data != null;
            _note.text = entry.Note.Text;
            _note.onEndEdit.AddListener(delegate(string value)
            {
                if (!_closing && ReferenceEquals(_selected, entry)) SaveNoteValue(entry, value);
            });
            _primary = Button(_detail, PrimaryText(entry), 26, 502, 390, 46, Primary);
            _primary.name = "SavePrimary";
            _primary.image.color = new Color(.35f, .32f, .23f, 1f);
            _primary.interactable = !_busy && (_saving || entry.Data != null || (_fromTitle && entry.AutoKind == null));
            _delete = Button(_detail, "删除", 434, 502, 194, 46, delegate
            {
                Confirm("删除「" + entry.Label + "」？", "只删除当前模式中的这一份进度，此操作无法撤销。", delegate
                {
                    ValidateSlot(entry, true);
                    SaveSystem.Instance.DeleteSaveData(entry.Slot);
                    ModSaveNotes.Delete(NotesRoot, CampaignId, _addon, entry.Slot);
                    RefreshEntries();
                    _status.text = "已删除 " + entry.Label;
                }, true);
            });
            _delete.GetComponentInChildren<Text>().color = Danger;
            _delete.interactable = entry.Data != null;
        }

        private string PrimaryText(Entry entry)
        {
            if (_saving) return entry.Data != null ? "覆盖此存档" : "保存到这里";
            return entry.Data != null ? "读取存档" : _fromTitle && entry.AutoKind == null ? "开始新的冒险" : "暂无存档";
        }

        private void Primary()
        {
            Run(PrimaryCore);
        }

        private void PrimaryCore()
        {
            Entry entry = _selected;
            if (entry == null || _busy) return;
            SaveNote();
            if (_saving)
            {
                Action save = delegate
                {
                    ValidateSlot(entry, false);
                    SaveSystem.Instance.SaveGameData(entry.Slot);
                    if (SaveSystem.Instance.GetSaveData(entry.Slot) == null)
                        throw new IOException("没有读到刚保存的进度，请检查游戏日志。");
                    RefreshEntries();
                    _status.text = "已保存到 " + entry.Label;
                };
                if (entry.Data != null)
                    Confirm("覆盖「" + entry.Label + "」？", "将用当前进度替换这份存档。", save, false);
                else Run(save);
                return;
            }
            ValidateSlot(entry, true);
            if (entry.Data == null)
            {
                if (!_fromTitle || entry.AutoKind != null) return;
                Confirm("开始新的冒险？", "新进度会保存到空栏位「" + entry.Label + "」。", delegate
                {
                    if (_addon) Plugin.Instance.StartAddon(entry.Slot);
                    else
                    {
                        _start(_package, entry.Slot);
                        CheckNavigation();
                    }
                    Close();
                }, false);
            }
            else
            {
                Run(delegate
                {
                    _busy = true;
                    if (_addon)
                    {
                        if (entry.AutoKind != null) Plugin.Instance.LoadAddonAuto(entry.AutoKind);
                        else Plugin.Instance.LoadAddon(entry.Slot);
                    }
                    else
                    {
                        if (entry.AutoKind != null) _loadAuto(_package, entry.AutoKind);
                        else _load(_package, entry.Slot);
                        CheckNavigation();
                    }
                    Close();
                });
            }
        }

        private void ValidateSlot(Entry entry, bool allowAuto)
        {
            if (!ModSaveSlotPolicy.OwnsBrowserSlot(CampaignId, _addon, entry.Slot, allowAuto))
                throw new InvalidOperationException("该存档不属于当前模式或作品，已停止操作。");
            ModPlayModeKind expected = _addon ? ModPlayModeKind.Addon : ModPlayModeKind.Campaign;
            if (ModPlayModeSession.Selected != expected)
                throw new InvalidOperationException("游戏模式已改变，请重新打开存档页。");
            if (_saving && SaveSystem.Instance != null
                && !ModSaveSlotPolicy.OwnsBrowserSlot(CampaignId, _addon, SaveSystem.Instance.CurrentSlot, true))
                throw new InvalidOperationException("当前进度与保存页的归属不一致，已停止保存。");
        }

        private void SaveNote()
        {
            if (_selected == null || _selected.Data == null) return;
            SaveNoteValue(_selected, _note != null ? _note.text : _selected.Note.Text);
        }

        private void SaveNoteValue(Entry entry, string text)
        {
            Run(delegate
            {
                ValidateSlot(entry, true);
                entry.Note.Text = text ?? "";
                ModSaveNotes.Write(NotesRoot, CampaignId, _addon, entry.Slot, entry.Note);
            });
        }

        private static void CheckNavigation()
        {
            if (Plugin.Instance != null && !string.IsNullOrEmpty(Plugin.Instance.LastNavigationError))
                throw new InvalidOperationException(Plugin.Instance.LastNavigationError);
        }

        private void Confirm(string title, string detail, Action action, bool destructive)
        {
            if (_confirm != null) return;
            _confirmation = action;
            _confirm = Box(_pane.parent, "Confirm", new Color(0, 0, 0, .8f));
            Stretch(_confirm);
            RectTransform card = Box(_confirm, "Dialog", Ink);
            card.anchorMin = card.anchorMax = card.pivot = new Vector2(.5f, .5f);
            card.anchoredPosition = Vector2.zero;
            card.sizeDelta = new Vector2(680, 280);
            Frame(card);
            Label(card, title, 28, Paper, 30, 28, 620, 48);
            Label(card, detail, 21, Muted, 30, 95, 620, 74);
            Button cancel = Button(card, "取消", 30, 198, 290, 48, CancelConfirm);
            Button accept = Button(card, destructive ? "确认删除" : "确认", 344, 198, 306, 48, delegate
            {
                Action pending = _confirmation;
                CancelConfirm();
                if (pending != null) Run(pending);
            });
            accept.GetComponentInChildren<Text>().color = destructive ? Danger : Gold;
            cancel.Select();
        }

        private void CancelConfirm()
        {
            _confirmation = null;
            if (_confirm != null) Destroy(_confirm.gameObject);
            _confirm = null;
            if (_primary != null) _primary.Select();
        }

        private void Run(Action action)
        {
            try { action(); }
            catch (Exception ex)
            {
                _busy = false;
                if (_status != null) { _status.text = ex.Message; _status.color = Danger; }
                PersistentModState.Log?.LogError("MOD 存档操作失败：" + ex);
            }
        }

        private void Update()
        {
            if (_closing) return;
            if (_host != null && !_host.gameObject.activeInHierarchy) { Close(); return; }
            if (_host != null) _host.SetCanvasGroup(0, false, false);
            GameObject selectedObject = EventSystem.current != null
                ? EventSystem.current.currentSelectedGameObject : null;
            if (_confirm == null && selectedObject != null
                && selectedObject.name.StartsWith("SaveEntry_", StringComparison.Ordinal))
            {
                string slot = selectedObject.name.Substring("SaveEntry_".Length);
                Entry focus = _entries.FirstOrDefault(x => x.Slot == slot);
                if (focus != null && !ReferenceEquals(focus, _selected))
                {
                    SaveNote();
                    _selected = focus;
                    RenderRows();
                }
            }
            Keyboard keyboard = Keyboard.current;
            if (keyboard == null) return;
            if (keyboard.escapeKey.wasPressedThisFrame)
            {
                if (_confirm != null) CancelConfirm();
                else Close();
            }
            if ((keyboard.enterKey.wasPressedThisFrame || keyboard.numpadEnterKey.wasPressedThisFrame)
                && _confirm == null && _primary != null && _primary.interactable)
            {
                GameObject focus = EventSystem.current != null ? EventSystem.current.currentSelectedGameObject : null;
                if (focus == null) Primary();
            }
        }

        internal void Close()
        {
            Close(true);
        }

        internal void Close(bool restoreFocus)
        {
            if (_closing) return;
            SaveNote();
            _closing = true;
            if (Current == this) Current = null;
            if (_hostClose != null) _hostClose.SetActive(true);
            if (_owner != null) _owner.enabled = _ownerEnabled;
            if (_host != null)
            {
                _host.SetCanvasGroup(1, true, true);
                _host.Show(false);
            }
            if (restoreFocus && _previousFocus != null && _previousFocus.activeInHierarchy && EventSystem.current != null)
                EventSystem.current.SetSelectedGameObject(_previousFocus);
            Destroy(gameObject);
        }

        private void OnDestroy()
        {
            if (Current == this) Current = null;
        }

        private string ChapterText(GameSave data)
        {
            if (data == null) return "";
            string scene = data.CurrentScene ?? "";
            string label = scene == "Free" ? "养成" : scene == "Combat" ? "决斗" : scene == "Battle" ? "战役" : "剧情";
            string story = data.CurrentStoryScript ?? "";
            if (_package != null)
            {
                string prefix = "MOD_" + _package.Id + "_";
                if (story.StartsWith(prefix, StringComparison.Ordinal)) story = story.Substring(prefix.Length);
            }
            return label + (string.IsNullOrEmpty(story) ? "" : " · " + story);
        }

        private string RecentLines(GameSave data)
        {
            if (data == null || data.StoryLogs == null) return "尚无可预览的对白";
            var lines = new List<string>();
            for (int i = data.StoryLogs.Count - 1; i >= 0 && lines.Count < 3; i--)
            {
                StoryLogSave log = data.StoryLogs[i];
                if (log == null || string.IsNullOrEmpty(log.Key) || log.Type != StoryLogType.Story) continue;
                string value = null;
                if (_package != null) _package.Texts.TryGetValue(log.Key, out value);
                if (string.IsNullOrEmpty(value) && LocalizationManager.Instance != null)
                {
                    value = LocalizationManager.Instance.LocaleResolver.GetString("Story/" + log.Key);
                    if (value == "Story/" + log.Key || value == log.Key) value = null;
                }
                if (string.IsNullOrEmpty(value)) continue;
                value = Regex.Replace(value, "<[^>]*>|\\{[^}]*\\}", "").Trim();
                if (value.Length > 110) value = value.Substring(0, 110) + "…";
                if (value.Length > 0) lines.Add("「" + value + "」");
            }
            lines.Reverse();
            return lines.Count == 0 ? "尚无可预览的对白" : string.Join("\n", lines);
        }

        private static string SaveDate(GameSave data)
        {
            if (data == null || data.TimeTick <= 0 || data.TimeTick > DateTime.MaxValue.Ticks) return "";
            return new DateTime(data.TimeTick).ToString("yyyy/MM/dd HH:mm");
        }

        private static string GameDate(GameSave data)
        {
            if (data == null) return "";
            var time = new GameTime(data.CurrentYear, data.CurrentMonth, data.CurrentStage);
            var resolver = LocalizationManager.Instance != null ? LocalizationManager.Instance.LocaleResolver : null;
            if (resolver == null) return "第 " + data.CurrentYear + " 年 " + data.CurrentMonth + " 月";
            return string.Format(resolver.GetString("System/GameTimeText"),
                resolver.GetString(time.YearText), resolver.GetString(time.MonthText), resolver.GetString(time.StageText));
        }

        private static bool Contains(string value, string query)
        {
            return query.Length == 0 || (value ?? "").IndexOf(query, StringComparison.OrdinalIgnoreCase) >= 0;
        }

        private static Font FindSaveFont()
        {
            IEnumerable<Component> slots = Resources.FindObjectsOfTypeAll<SaveSlotPanel>()
                .Cast<Component>().Concat(Resources.FindObjectsOfTypeAll<LoadSlotPanel>());
            foreach (Component slot in slots)
            {
                var time = AccessTools.Field(slot.GetType(), "_timeText")?.GetValue(slot) as Text[];
                Font font = time != null ? time.Where(x => x != null).Select(x => x.font).FirstOrDefault(x => x != null) : null;
                if (font != null) return font;
                var label = AccessTools.Field(slot.GetType(), "_slotText")?.GetValue(slot) as Text;
                if (label != null && label.font != null) return label.font;
            }
            return null;
        }

        private bool InputHasFocus()
        {
            GameObject focus = EventSystem.current != null ? EventSystem.current.currentSelectedGameObject : null;
            return focus != null && focus.transform.IsChildOf(transform)
                && focus.GetComponent<InputField>() != null;
        }

        private static RectTransform Rect(Transform parent, string name)
        {
            var obj = new GameObject(name, typeof(RectTransform));
            obj.transform.SetParent(parent, false);
            return (RectTransform)obj.transform;
        }

        private static RectTransform Box(Transform parent, string name, Color color)
        {
            RectTransform rect = Rect(parent, name);
            Image image = rect.gameObject.AddComponent<Image>();
            image.color = color;
            return rect;
        }

        private static void Place(RectTransform rect, float x, float y, float width, float height)
        {
            rect.anchorMin = rect.anchorMax = rect.pivot = new Vector2(0, 1);
            rect.anchoredPosition = new Vector2(x, -y);
            rect.sizeDelta = new Vector2(width, height);
        }

        private static void Stretch(RectTransform rect)
        {
            rect.anchorMin = Vector2.zero;
            rect.anchorMax = Vector2.one;
            rect.offsetMin = rect.offsetMax = Vector2.zero;
        }

        private static Text Label(Transform parent, string value, int size, Color color,
            float x, float y, float width, float height)
        {
            RectTransform rect = Rect(parent, "Text");
            Place(rect, x, y, width, height);
            Text text = rect.gameObject.AddComponent<Text>();
            text.font = _font;
            text.fontSize = size;
            text.text = value ?? "";
            text.color = color;
            text.supportRichText = false;
            text.raycastTarget = false;
            text.alignment = TextAnchor.UpperLeft;
            text.horizontalOverflow = HorizontalWrapMode.Wrap;
            text.verticalOverflow = VerticalWrapMode.Truncate;
            return text;
        }

        private static Button Button(Transform parent, string value, float x, float y,
            float width, float height, Action action)
        {
            RectTransform rect = Box(parent, "Button", Card);
            Place(rect, x, y, width, height);
            Button button = rect.gameObject.AddComponent<Button>();
            ColorBlock colors = button.colors;
            colors.normalColor = Color.white;
            colors.highlightedColor = new Color(1.3f, 1.3f, 1.2f, 1);
            colors.selectedColor = new Color(1.4f, 1.3f, 1.1f, 1);
            colors.pressedColor = new Color(.8f, .8f, .7f, 1);
            colors.disabledColor = new Color(.55f, .55f, .55f, 1);
            button.colors = colors;
            button.targetGraphic = rect.GetComponent<Image>();
            button.navigation = new Navigation { mode = Navigation.Mode.Automatic };
            if (!string.IsNullOrEmpty(value))
            {
                Text label = Label(rect, value, 21, Paper, 8, 0, width - 16, height);
                label.alignment = TextAnchor.MiddleCenter;
            }
            button.onClick.AddListener(delegate { action?.Invoke(); });
            return button;
        }

        private static InputField Input(Transform parent, string placeholder, float x,
            float y, float width, float height, bool multi)
        {
            RectTransform rect = Box(parent, "Input", new Color(.035f, .043f, .042f, 1));
            Place(rect, x, y, width, height);
            InputField input = rect.gameObject.AddComponent<InputField>();
            Text text = Label(rect, "", 20, Paper, 12, 8, width - 24, height - 16);
            Text hint = Label(rect, placeholder, 18, Muted, 12, 8, width - 24, height - 16);
            input.textComponent = text;
            input.placeholder = hint;
            input.targetGraphic = rect.GetComponent<Image>();
            input.lineType = multi ? InputField.LineType.MultiLineNewline : InputField.LineType.SingleLine;
            input.customCaretColor = true;
            input.caretColor = Gold;
            input.selectionColor = new Color(.65f, .58f, .35f, .5f);
            input.navigation = new Navigation { mode = Navigation.Mode.Automatic };
            return input;
        }

        private static void Line(Transform parent, float x, float y, float width)
        {
            Place(Box(parent, "Line", Rule), x, y, width, 1);
        }

        private static void Frame(RectTransform parent)
        {
            float width = parent.sizeDelta.x, height = parent.sizeDelta.y;
            Place(Box(parent, "BorderTop", Rule), 0, 0, width, 1);
            Place(Box(parent, "BorderBottom", Rule), 0, height - 1, width, 1);
            Place(Box(parent, "BorderLeft", Rule), 0, 0, 1, height);
            Place(Box(parent, "BorderRight", Rule), width - 1, 0, 1, height);
        }
    }

    internal sealed class ModSaveSubmit : MonoBehaviour, ISubmitHandler
    {
        internal Action Action;
        public void OnSubmit(BaseEventData eventData)
        {
            Action?.Invoke();
        }
    }
}
