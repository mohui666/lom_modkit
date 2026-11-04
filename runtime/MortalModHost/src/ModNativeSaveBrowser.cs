// UI source adapted at the user's request from Coffee Companion / 咖啡伴侣.
// Author/source: kevinnie2003/LOMAssistant, v3.4.0, commit 5b2749bdc37e2dba312feab694c1617f151e1ff5.
// https://github.com/kevinnie2003/LOMAssistant/blob/v3.4.0/LOMAssistant.Features.Extras/SaveBrowser.cs
// Original list/detail layout, portrait fitting, native button templates and fonts retained.
// Host changes: explicit mode/campaign ownership, 20+3 scoped slots, native navigation,
// isolated notes, scene lifecycle restoration, saved-dialogue preview instead of StoryAtlas.
// Attribution is recorded in runtime/MortalModHost/THIRD_PARTY_NOTICES.md; no MIT grant is asserted.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using BepInEx;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.EventSystems;
using UnityEngine.InputSystem;
using UnityEngine.ResourceManagement.AsyncOperations;
using UnityEngine.UI;

namespace MortalModHost
{

/// <summary>
/// 新版存档、读档界面：保留游戏面板的外框、标题、关闭按钮和确认框，把原来的栏位藏起来，换成左边列表、右边详情。
/// 列表显示心上人头像、游戏日期、保存时间和备注；详情栏保留心上人立绘与存档时的最近对白。
/// 手动栏只使用当前玩法/作品的20个隔离槽；自动页只列剧情、养成、战斗三种已有隔离档，不扩展历史档。
/// 条目动作经Host归属检查后调用实际SaveSystem或导航回调；覆盖与删除沿用游戏确认框。
/// </summary>
internal class ModNativeSaveBrowser : MonoBehaviour
{
	private static readonly Color Txt = new Color(0.91f, 0.88f, 0.82f, 1f);
	private static readonly Color Gold = new Color(0.79f, 0.64f, 0.42f, 1f);
	private static readonly Color Dim = new Color(0.59f, 0.55f, 0.49f, 1f);
	private static readonly Color LineColor = new Color(0.35f, 0.31f, 0.25f, 1f);
	private static readonly Color PaneColor = new Color(0.03f, 0.028f, 0.025f, 0.5f);

	// 原来的这些子物体换成新界面后藏起来（只调透明、不停用：游戏打开面板时还会刷新、选中它们）
	private static readonly string[] LoadHidden = { "Logo", "RecentSave", "AutoSave", "Line", "Scroll View", "Auto", "Manual", "Recent", "GamepadIcon", "GamepadIcon (1)" };
	private static readonly string[] SaveHidden = { "Logo", "Line", "Scroll View" };

	/// <summary>心上人是「自己」：还没有心上人。</summary>
	private const int NoLover = 99;

	private const string NewGame = "新游戏";

	private static readonly (string Category, string Name)[] AutoGroups = { ("auto", "剧情"), ("auto_free", "养成"), ("auto_battle", "战斗") };

	/// <summary>心上人（游戏数值 lover）对应的立绘。</summary>
	private static readonly Dictionary<int, string> Portraits = new Dictionary<int, string>
	{
		{ 0, "Sister" }, { 1, "Girl1_瑞杏" }, { 2, "Girl2_葉雲裳" }, { 3, "Girl5_小梅" }, { 4, "Girl9_上官瑩" },
		{ 5, "Girl6_小蘭" }, { 6, "Girl7_小竹" }, { 7, "Girl8_小菊" }, { 8, "Girl4_龍湘" },
	};

	// 结缘后详情栏用的害羞立绘：文件名各人不同（小梅没有 shy、小兰只有 shy2），选的都和 normal 同尺寸；瑞杏没有害羞表情，照旧 normal
	private static readonly Dictionary<int, string> ShyPortraits = new Dictionary<int, string>
	{
		{ 0, "shy" }, { 2, "shy" }, { 3, "shy1" }, { 4, "shy" }, { 5, "shy2" }, { 6, "shy" }, { 7, "shy" }, { 8, "shy" },
	};

	private sealed class Entry
	{
		public string Slot = ""; // 删除、备注用的栏位名（001、auto_1 …）
		public string Label = ""; // 传奇001
		public string Caption = ""; // 自动存档的类别、「当前进度」
		public GameSave Data;
		public bool Latest;
		public bool LastSaved; // 保存时间最新的手动存档
		public bool Current; // 正在玩的栏位（「继续游戏」读的那个）
		public bool CanNote;
		public bool CanDelete;
		public bool Compact;
		public Action Primary;
	}

	private bool _isLoad;
	private string _language; // 界面建起来时游戏的语言
	private MonoBehaviour _panel;
	private RectTransform _root, _listContent, _detail;
	private ScrollRect _scroll;
	private Font _calligraphy, _serif;
	private Button _buttonTemplate;
	private int _tab; // 读档页：0 手动，1 自动
	private Button _sortButton; // 读档页手动存档的排序：按栏位 / 最近在上
	private readonly List<Button> _tabs = new List<Button>();
	private readonly List<(Entry Entry, Button Button)> _rows = new List<(Entry, Button)>();
	private int _selected;
	private Entry _shown;

	// 详情栏
	private Image _portrait;
	private Text _loverCaption, _loverName, _date, _info, _mission, _section, _lines;
	private InputField _noteBox;
	private Button _primaryButton, _deleteButton;
	private readonly Dictionary<Entry, Text> _rowNotes = new Dictionary<Entry, Text>();
	private float _noteEditedAt = -10f;
	private static ModNativeSaveBrowser _current;

	public static bool Active(MonoBehaviour panel) => panel != null && panel.GetComponent<ModNativeSaveBrowser>() is ModNativeSaveBrowser b && b._root != null;


    private sealed class Scope
    {
        internal ModPackage Package;
        internal bool Addon, FromTitle;
        internal Action<ModPackage, string> Start, Load, LoadAuto;
        internal string CampaignId { get { return Package != null ? Package.CampaignId : null; } }
        internal bool Owns(string slot, bool auto = true)
        {
            return ModSaveSlotPolicy.OwnsBrowserSlot(CampaignId, Addon, slot, auto);
        }
        internal string Manual(int index) { return Addon ? ModSaveSlotPolicy.AddonManualSlot(index) : ModSaveSlotPolicy.IsolatedManualSlot(CampaignId, index); }
        internal string Auto(string kind) { return Addon ? ModSaveSlotPolicy.AddonAutoSlot(kind) : ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(CampaignId, kind); }
    }

    private Scope _scope;
    private CommonPanel _host;
    private Text _ownerLabel;
    private bool _navigating, _detaching;
    private static int _sortOrder;
    private readonly Dictionary<CanvasGroup, (float Alpha, bool Interactable, bool Blocks)> _hidden =
        new Dictionary<CanvasGroup, (float, bool, bool)>();
    private RectTransform _saveHeader;
    private Vector2 _saveHeaderPosition;

    internal static ModNativeSaveBrowser Current { get { return _current; } }
    internal static bool IsActive { get { return _current != null && _current._scope != null && _current._host != null && _current._host.gameObject.activeInHierarchy; } }
    internal static string SelectedCampaignId { get { return _current != null ? _current.CampaignId : null; } }
    internal bool Saving { get { return !_isLoad; } }
    internal string CampaignId { get { return _scope != null ? _scope.CampaignId : null; } }
    private string NotesRoot { get { return System.IO.Path.Combine(Paths.ConfigPath, "MortalModHost", "save_notes"); } }

    internal static bool TryOpenSave(SaveGamePanel owner, ModPackage package, bool addon)
    {
        return OpenScoped(owner, owner.GetComponentInParent<CommonPanel>(), package, addon, true, false, null, null, null);
    }

    internal static bool TryOpenExisting(CommonPanel host, LoadGamePanel owner, ModPackage package, bool addon,
        Action<ModPackage, string> start, Action<ModPackage, string> load, Action<ModPackage, string> auto)
    {
        return OpenScoped(owner, host, package, addon, false, false, start, load, auto);
    }

    internal static bool TryOpenTitle(ModPackage package, bool addon, Action<ModPackage, string> start = null,
        Action<ModPackage, string> load = null, Action<ModPackage, string> auto = null)
    {
        TitleManager title = TitleManager.Instance;
        if (title == null) return false;
        CommonPanel host = Traverse.Create(title).Field("_slotPanel").GetValue<CommonPanel>();
        LoadGamePanel panel = host != null ? host.GetComponentInChildren<LoadGamePanel>(true) : null;
        return OpenScoped(panel, host, package, addon, false, true, start, load, auto);
    }

    private static bool OpenScoped(MonoBehaviour owner, CommonPanel host, ModPackage package, bool addon,
        bool saving, bool title, Action<ModPackage, string> start, Action<ModPackage, string> load,
        Action<ModPackage, string> auto)
    {
        if (owner == null || host == null || (!addon && package == null)
            || ModPlayModeSession.Selected != (addon ? ModPlayModeKind.Addon : ModPlayModeKind.Campaign)) return false;
        ModNativeSaveBrowser browser = owner.GetComponent<ModNativeSaveBrowser>();
        if (_opening && browser != null && _current == browser) return true;
        if (_current != null) Close();
        browser = owner.GetComponent<ModNativeSaveBrowser>() ?? owner.gameObject.AddComponent<ModNativeSaveBrowser>();
        browser._panel = owner;
        browser._host = host;
        browser._isLoad = !saving;
        browser._scope = new Scope { Package = package, Addon = addon, FromTitle = title, Start = start, Load = load, LoadAuto = auto };
        browser._navigating = false;
        _current = browser;
        _opening = true;
        try
        {
            if (!host.gameObject.activeInHierarchy) host.Show(true);
            if (browser._root != null && browser._language != I18n.Current)
            {
                Destroy(browser._root.gameObject);
                browser._root = null;
            }
            browser.HideOriginal();
            if (browser._root == null && !browser.Build()) throw new InvalidOperationException("游戏存档面板缺少原生模板");
            browser._root.gameObject.SetActive(true);
            browser.SetOwnerLabel();
            browser._selected = -1;
            browser.Refresh();
            return true;
        }
        catch (Exception ex)
        {
            browser.Report(ex);
            browser.DetachScope();
            return false;
        }
        finally { _opening = false; }
    }

    private static bool _opening;

    private void HideOriginal()
    {
        Transform container = _panel.transform.Find("Container");
        if (container == null) return;
        foreach (string name in _isLoad ? LoadHidden : SaveHidden)
        {
            Transform item = container.Find(name);
            if (item == null) continue;
            CanvasGroup group = item.GetComponent<CanvasGroup>() ?? item.gameObject.AddComponent<CanvasGroup>();
            if (!_hidden.ContainsKey(group)) _hidden.Add(group, (group.alpha, group.interactable, group.blocksRaycasts));
            group.alpha = 0;
            group.interactable = group.blocksRaycasts = false;
        }
        if (!_isLoad && _saveHeader == null && container.Find("Header") is RectTransform header)
        {
            _saveHeader = header;
            _saveHeaderPosition = header.anchoredPosition;
        }
        if (!_isLoad && _saveHeader != null) _saveHeader.anchoredPosition = new Vector2(106, -48);
    }

    private void SetOwnerLabel()
    {
        if (_ownerLabel == null)
        {
            _ownerLabel = NewText(_root, "MortalModHost_SaveOwner", _serif, 23, Gold, TextAnchor.UpperLeft);
            Place(_ownerLabel.rectTransform, 56, 108, Mathf.Max(300, _root.rect.width - 112), 34);
            _ownerLabel.resizeTextForBestFit = true;
            _ownerLabel.resizeTextMinSize = 18;
            _ownerLabel.resizeTextMaxSize = 23;
        }
        _ownerLabel.text = _scope.Addon ? "原版＋MOD · 独立存档" : "纯MOD · " + ModDisclosurePolicy.SafePackageName(_scope.Package) + " · v" + _scope.Package.Version;
    }

    private void CheckSlot(string slot, bool saving)
    {
        if (_scope == null || !_scope.Owns(slot, !saving)
            || ModPlayModeSession.Selected != (_scope.Addon ? ModPlayModeKind.Addon : ModPlayModeKind.Campaign))
            throw new InvalidOperationException("存档不属于当前玩法或作品，已停止操作。");
        if (saving && (SaveSystem.Instance == null || !_scope.Owns(SaveSystem.Instance.CurrentSlot)))
            throw new InvalidOperationException("当前进度与保存页不属于同一玩法或作品。");
    }

    private void Report(Exception ex)
    {
        if (_ownerLabel != null) { _ownerLabel.text = ex.Message; _ownerLabel.color = new Color(.95f, .58f, .5f, 1); }
        PersistentModState.Log?.LogError("MOD 原生存档界面：" + ex);
    }

    internal static void Close()
    {
        ModNativeSaveBrowser browser = _current;
        if (browser == null) return;
        CommonPanel host = browser._host;
        browser.DetachScope();
        if (host != null && host.gameObject.activeInHierarchy) host.Show(false);
    }

    private void DetachScope()
    {
        if (_detaching) return;
        _detaching = true;
        if (_scope != null && ModPlayModeSession.Selected == (_scope.Addon ? ModPlayModeKind.Addon : ModPlayModeKind.Campaign)
            && _shown != null && _shown.CanNote && _shown.Data != null && _noteBox != null)
        {
            try { SaveNote(_noteBox.text); } catch (Exception ex) { Report(ex); }
        }
        _scope = null;
        if (_current == this) _current = null;
        if (_root != null) _root.gameObject.SetActive(false);
        foreach (var item in _hidden)
        {
            if (item.Key == null) continue;
            item.Key.alpha = item.Value.Alpha;
            item.Key.interactable = item.Value.Interactable;
            item.Key.blocksRaycasts = item.Value.Blocks;
        }
        _hidden.Clear();
        if (_saveHeader != null) _saveHeader.anchoredPosition = _saveHeaderPosition;
        _saveHeader = null;
        _detaching = false;
    }

    private void OnDisable() { DetachScope(); }
    private void OnDestroy() { DetachScope(); }

    internal static void RefreshOwner(MonoBehaviour owner)
    {
        if (IsActive && _current._panel == owner) _current.Refresh();
    }

    internal static bool Guard(MonoBehaviour owner, string slot, bool saving)
    {
        if (!IsActive || _current._panel != owner) return true;
        try { _current.CheckSlot(slot, saving); return true; }
        catch (Exception ex) { _current.Report(ex); return false; }
    }

    internal static bool DeleteConfirmed(LoadGamePanel owner)
    {
        if (!IsActive || _current._panel != owner) return false;
        ModNativeSaveBrowser browser = _current;
        string slot = Traverse.Create(owner).Field("_currentDeleteSlot").GetValue<string>();
        try
        {
            browser.CheckSlot(slot, false);
            SaveSystem.Instance.DeleteSaveData(slot);
            ModSaveNotes.Delete(browser.NotesRoot, browser.CampaignId, browser._scope.Addon, slot);
            Traverse.Create(owner).Field("_currentDeleteSlot").SetValue("");
            owner.CloseDeletePanel();
            browser.Refresh();
        }
        catch (Exception ex) { browser.Report(ex); }
        return true;
    }

	// ---------------- 搭界面 ----------------

	private bool Build()
	{
		_language = I18n.Current;
		Transform container = _panel.transform.Find("Container");
		if (container == null)
		{
			PersistentModState.Log?.LogWarning("[存档界面] 找不到 Container，用原来的界面");
			return false;
		}
		if (!FindTemplates())
		{
			PersistentModState.Log?.LogWarning("[存档界面] 找不到原来栏位的字体或按钮，用原来的界面");
			return false;
		}
		foreach (string name in _isLoad ? LoadHidden : SaveHidden)
		{
			Transform t = container.Find(name);
			if (t != null)
			{
				CanvasGroup g = t.GetComponent<CanvasGroup>() ?? t.gameObject.AddComponent<CanvasGroup>();
				g.alpha = 0f;
				g.interactable = false;
				g.blocksRaycasts = false;
			}
		}
		var size = ((RectTransform)container).rect.size;
		// 存档页的标题原来在右边，挪到和读档页一样的左上角
		if (!_isLoad && container.Find("Header") is RectTransform header)
		{
			header.anchoredPosition = new Vector2(106f, -48f);
		}
		_root = NewRect("MortalModHost_NativeSaveBrowser", container);
		Stretch(_root);
		// 放在确认框、关闭按钮下面
		Transform below = container.Find(_isLoad ? "DeletePanel" : "ConfirmPanel");
		_root.SetSiblingIndex(below != null ? below.GetSiblingIndex() : 0);

		float margin = 56f, top = 150f, bottom = 40f, gap = 36f;
		float listWidth = Mathf.Round((size.x - margin * 2 - gap) * 0.52f);
		float detailX = margin + listWidth + gap;
		float listTop = top + (_isLoad ? 70f : 10f);
		if (_isLoad)
		{
			BuildTabs(margin, top);
			// 手动存档页列表右上角：排序切换（配置 SaveListOrder，按栏位 → 最近在上 → 原版：按栏位、空栏位也列出），字样和「删除」一样，不加边框
			_sortButton = TextButton(_root, "排序：按栏位");
			Place((RectTransform)_sortButton.transform, margin + listWidth - 240f, top + 16f, 240f, 40f);
			_sortButton.GetComponentInChildren<Text>().fontSize = 24;
			_sortButton.GetComponentInChildren<Text>().alignment = TextAnchor.MiddleRight;
			_sortButton.onClick.AddListener(() =>
			{
				_sortOrder = (_sortOrder + 1) % 3;
				_selected = -1;
				Refresh();
			});
		}
		BuildList(margin, listTop, listWidth, size.y - listTop - bottom);
		BuildDetail(detailX, top, size.x - detailX - margin, size.y - top - bottom);
		PersistentModState.Log?.LogInfo((object)$"[存档界面] {(_isLoad ? "读档" : "存档")}页已换成新界面（{size.x:0}x{size.y:0}）");
		return true;
	}

	private bool FindTemplates()
	{
		Component slot = _isLoad
			? (Component)(Traverse.Create(_panel).Field("_saveSlots").GetValue<LoadSlotPanel[]>()?.FirstOrDefault())
			: Traverse.Create(_panel).Field("_saveSlots").GetValue<SaveSlotPanel[]>()?.FirstOrDefault();
		if (slot == null)
		{
			return false;
		}
		var tr = Traverse.Create(slot);
		_calligraphy = tr.Field("_titleText").GetValue<Text[]>()?.FirstOrDefault()?.font;
		_serif = tr.Field("_timeText").GetValue<Text[]>()?.FirstOrDefault()?.font ?? tr.Field("_slotText").GetValue<Text>()?.font;
		// 按钮样式用读档页栏位上的「删除」按钮（存档页栏位没有，去读档面板上找）
		LoadGamePanel load = _isLoad ? (LoadGamePanel)_panel : LoadGamePanel.Instance;
		LoadSlotPanel loadSlot = load != null ? Traverse.Create(load).Field("_saveSlots").GetValue<LoadSlotPanel[]>()?.FirstOrDefault() : null;
		_buttonTemplate = loadSlot != null ? loadSlot.DeleteButton : null;
		if (_buttonTemplate == null)
		{
			AutoSaveSlotPanel auto = load != null ? Traverse.Create(load).Field("_autoSaveSlot").GetValue<AutoSaveSlotPanel[]>()?.FirstOrDefault() : null;
			_buttonTemplate = auto != null ? auto.DeleteButton : null;
		}
		return _calligraphy != null && _serif != null;
	}

	private void BuildTabs(float x, float y)
	{
		_tabs.Clear();
		string[] names = { "手动存档", "自动存档" };
		for (int i = 0; i < names.Length; i++)
		{
			int index = i;
			RectTransform rt = NewRect("Tab" + i, _root);
			Place(rt, x + i * 220f, y, 200f, 60f);
			Image hit = rt.gameObject.AddComponent<Image>();
			hit.color = new Color(0, 0, 0, 0);
			Button b = rt.gameObject.AddComponent<Button>();
			b.transition = Selectable.Transition.None;
			b.onClick.AddListener(() => SwitchTab(index));
			Text t = NewText(rt, "Text", _calligraphy, 40, Dim, TextAnchor.MiddleLeft);
			Stretch(t.rectTransform);
			t.text = names[i];
			RectTransform underline = NewRect("Underline", rt);
			Place(underline, 0f, 56f, 160f, 3f);
			underline.gameObject.AddComponent<Image>().color = Gold;
			_tabs.Add(b);
		}
	}

	private void SwitchTab(int tab)
	{
		if (_tab == tab)
		{
			return;
		}
		_tab = tab;
		_selected = -1;
		Refresh();
	}

	private void BuildList(float x, float y, float w, float h)
	{
		RectTransform view = NewRect("List", _root);
		Place(view, x, y, w, h);
		view.gameObject.AddComponent<RectMask2D>();
		_scroll = view.gameObject.AddComponent<ScrollRect>();
		_scroll.horizontal = false;
		_scroll.movementType = ScrollRect.MovementType.Clamped;
		_scroll.scrollSensitivity = 40f;
		Image hit = view.gameObject.AddComponent<Image>();
		hit.color = new Color(0, 0, 0, 0);
		_listContent = NewRect("Content", view);
		_listContent.anchorMin = new Vector2(0, 1);
		_listContent.anchorMax = new Vector2(1, 1);
		_listContent.pivot = new Vector2(0.5f, 1f);
		_listContent.sizeDelta = Vector2.zero;
		VerticalLayoutGroup layout = _listContent.gameObject.AddComponent<VerticalLayoutGroup>();
		layout.childControlHeight = true;
		layout.childControlWidth = true;
		layout.childForceExpandHeight = false;
		layout.spacing = 6f;
		layout.padding = new RectOffset(0, 18, 0, 10);
		ContentSizeFitter fitter = _listContent.gameObject.AddComponent<ContentSizeFitter>();
		fitter.verticalFit = ContentSizeFitter.FitMode.PreferredSize;
		_scroll.content = _listContent;
		_scroll.viewport = view;
		// 细滚动条
		RectTransform bar = NewRect("Scrollbar", view);
		bar.anchorMin = new Vector2(1, 0);
		bar.anchorMax = new Vector2(1, 1);
		bar.pivot = new Vector2(1, 0.5f);
		bar.sizeDelta = new Vector2(4f, 0f);
		bar.gameObject.AddComponent<Image>().color = LineColor;
		Scrollbar scrollbar = bar.gameObject.AddComponent<Scrollbar>();
		scrollbar.direction = Scrollbar.Direction.BottomToTop;
		RectTransform handle = NewRect("Handle", bar);
		Stretch(handle);
		Image handleImage = handle.gameObject.AddComponent<Image>();
		handleImage.color = new Color(0.67f, 0.63f, 0.55f, 1f);
		scrollbar.handleRect = handle;
		scrollbar.targetGraphic = handleImage;
		scrollbar.navigation = new Navigation { mode = Navigation.Mode.None };
		_scroll.verticalScrollbar = scrollbar;
		_scroll.verticalScrollbarVisibility = ScrollRect.ScrollbarVisibility.AutoHide;
	}

	private void BuildDetail(float x, float y, float w, float h)
	{
		_detail = NewRect("Detail", _root);
		Place(_detail, x, y, w, h);
		_detail.gameObject.AddComponent<Image>().color = PaneColor;
		Border(_detail, new Color(Gold.r, Gold.g, Gold.b, 0.35f));

		// 立绘：右上角，只露出头到胸口，下沿正好落在分隔线上
		// portraitW：立绘大致占的宽度，人物中心放在这一块的中间，左边留给文字
		const float portraitH = 300f;
		float picHeight = portraitH / 0.46f, picWidth = picHeight * 1268f / 1700f;
		float portraitW = Mathf.Min(Mathf.Round(picWidth * 0.72f), w * 0.56f);
		RectTransform frame = NewRect("Portrait", _detail);
		// 遮罩只管上下（下沿落在分隔线上），左右放开：有的立绘人物偏一边、手里拿着东西，左右一裁就切到人
		Place(frame, 0f, 4f, w, portraitH);
		frame.gameObject.AddComponent<RectMask2D>();
		RectTransform pic = NewRect("Image", frame);
		_portrait = pic.gameObject.AddComponent<Image>();
		_portrait.raycastTarget = false;
		_portrait.preserveAspect = true;
		Place(pic, w - 8f - portraitW / 2f - picWidth / 2f, -0.3f * picHeight, picWidth, picHeight);
		_portraitCenter = w - 8f - portraitW / 2f;
		_portraitHeight = portraitH;

		float textW = w - portraitW - 40f;
		_loverCaption = NewText(_detail, "LoverCaption", _serif, 22, Dim, TextAnchor.UpperLeft);
		Place(_loverCaption.rectTransform, 28f, 22f, textW, 30f);
		_loverName = NewText(_detail, "Lover", _calligraphy, 54, Txt, TextAnchor.UpperLeft);
		Place(_loverName.rectTransform, 28f, 48f, textW + 60f, 66f);
		_date = NewText(_detail, "Date", _calligraphy, 34, Txt, TextAnchor.UpperLeft);
		Place(_date.rectTransform, 28f, 120f, textW + 60f, 46f);
		_info = NewText(_detail, "Info", _serif, 21, Dim, TextAnchor.UpperLeft);
		Place(_info.rectTransform, 28f, 172f, textW, 60f);
		_info.horizontalOverflow = HorizontalWrapMode.Wrap;
		_mission = NewText(_detail, "Mission", _serif, 22, Txt, TextAnchor.UpperLeft);
		Place(_mission.rectTransform, 28f, 234f, textW, 62f);

		RectTransform rule = NewRect("Rule", _detail);
		Place(rule, 24f, 304f, w - 48f, 1f);
		rule.gameObject.AddComponent<Image>().color = LineColor;
		_section = NewText(_detail, "Section", _serif, 21, Dim, TextAnchor.UpperLeft);
		Place(_section.rectTransform, 28f, 316f, w - 56f, 30f);
		_lines = NewText(_detail, "Lines", _serif, 22, Txt, TextAnchor.UpperLeft);
		Place(_lines.rectTransform, 28f, 348f, w - 56f, h - 348f - 190f);
		_lines.horizontalOverflow = HorizontalWrapMode.Wrap;
		// 放不下的整句不显示，不压到备注框上
		_lines.verticalOverflow = VerticalWrapMode.Truncate;
		_linesHeight = h - 348f - 190f;
		_lines.lineSpacing = 1.1f;

		// 备注框：点了改备注
		RectTransform note = NewRect("Note", _detail);
		Place(note, 24f, h - 178f, w - 48f, 68f);
		Image noteBack = note.gameObject.AddComponent<Image>();
		noteBack.color = new Color(0f, 0f, 0f, 0.3f);
		Border(note, new Color(Gold.r, Gold.g, Gold.b, 0.25f));
		// 备注直接在这里写：点进去打字，回车或点别处就存
		Text noteText = NewText(note, "Text", _serif, 23, Gold, TextAnchor.MiddleLeft);
		Place(noteText.rectTransform, 16f, 0f, w - 80f, 68f);
		Text placeholder = NewText(note, "Placeholder", _serif, 21, new Color(0.42f, 0.39f, 0.35f, 1f), TextAnchor.MiddleLeft);
		Place(placeholder.rectTransform, 16f, 0f, w - 80f, 68f);
		placeholder.text = "点这里写备注，回车保存";
		_noteBox = note.gameObject.AddComponent<InputField>();
		_noteBox.textComponent = noteText;
		_noteBox.placeholder = placeholder;
		_noteBox.targetGraphic = noteBack;
		_noteBox.lineType = InputField.LineType.SingleLine;
		_noteBox.characterLimit = 30;
		_noteBox.caretColor = Gold;
		_noteBox.customCaretColor = true;
		_noteBox.selectionColor = new Color(Gold.r, Gold.g, Gold.b, 0.35f);
		_noteBox.transition = Selectable.Transition.ColorTint;
		_noteBox.colors = new ColorBlock { normalColor = Color.white, highlightedColor = new Color(1.6f, 1.5f, 1.3f, 1f), selectedColor = new Color(1.6f, 1.5f, 1.3f, 1f), pressedColor = Color.white, disabledColor = Color.white, colorMultiplier = 1f };
		_noteBox.onEndEdit.AddListener(SaveNote);

		_primaryButton = MakeButton(_detail, "读取", 30, 24f, h - 88f, 260f, 62f);
		_primaryButton.onClick.AddListener(() => Act(_shown));
		_deleteButton = MakeButton(_detail, "删除", 26, 304f, h - 88f, 140f, 62f);
		_deleteButton.onClick.AddListener(() => Delete(_shown));
	}

	// ---------------- 内容 ----------------

	private void Refresh()
	{
		if (_root == null || _scope == null)
		{
			return;
		}
		var timer = System.Diagnostics.Stopwatch.StartNew();
		try
		{
			List<Entry> entries;
			entries = _isLoad ? (_tab == 0 ? ManualEntries(includeEmpty: false) : AutoEntries()) : ManualEntries(includeEmpty: true);
			if (!_isLoad || _tab == 0)
			{
				// 上次保存：手动存档里保存时间最新的那个（存档文件里带保存时间，重启游戏也还在）
				string saved = entries.Where(e => e.Data != null && e.Caption != NewGame).OrderByDescending(e => e.Data.TimeTick).FirstOrDefault()?.Slot;
				// 当前进度：游戏自己记的当前栏位（读档、开新游戏时设定，重启游戏也还在，「继续游戏」读的就是它）
				string current = SaveSystem.Instance.CurrentSlot;
				foreach (Entry e in entries)
				{
					bool real = e.Data != null && e.Caption != NewGame;
					e.LastSaved = real && !string.IsNullOrEmpty(saved) && e.Slot == saved;
					e.Current = real && !string.IsNullOrEmpty(current) && e.Slot == current;
				}
			}
			int jumpTo = -1;
			if (_selected < 0)
			{
				jumpTo = Mathf.Max(0, entries.FindIndex(e => e.LastSaved));
				_selected = jumpTo;
			}
			foreach (Transform child in _listContent)
			{
				Destroy(child.gameObject);
			}
			_listContent.DetachChildren();
			_rows.Clear();
			_rowNotes.Clear();
			_current = this;
			if (_isLoad && _tab == 1)
			{
				string group = null;
				foreach (Entry e in entries)
				{
					if (e.Caption != group && e.Caption != "当前进度")
					{
						group = e.Caption;
						GroupHeader(group);
					}
					_rows.Add((e, Row(e)));
				}
			}

			else
			{
				foreach (Entry e in entries)
				{
					_rows.Add((e, Row(e)));
				}
			}
			if (_rows.Count == 0)
			{
				GroupHeader(_isLoad && _tab == 1 ? "还没有自动存档" : "还没有存档");
			}
			if (_sortButton != null)
			{
				_sortButton.gameObject.SetActive(_isLoad && _tab == 0);
				int order = _sortOrder;
				_sortButton.GetComponentInChildren<Text>().text = order == 1 ? "排序：最近在上" : order == 2 ? "排序：原版" : "排序：按栏位";
			}
			for (int i = 0; i < _tabs.Count; i++)
			{
				bool on = i == _tab;
				_tabs[i].GetComponentInChildren<Text>().color = on ? Txt : Dim;
				_tabs[i].transform.Find("Underline").gameObject.SetActive(on);
			}
			SetNavigation();
			_selected = Mathf.Clamp(_selected, 0, Mathf.Max(0, _rows.Count - 1));
			Show(_rows.Count > 0 ? _rows[_selected].Entry : null);

			Reselect();
			if (jumpTo >= 0 && jumpTo < _rows.Count)
			{
				ScrollTo((RectTransform)_rows[jumpTo].Button.transform);
			}
			if (timer.ElapsedMilliseconds >= 50)
			{
				PersistentModState.Log?.LogInfo((object)$"[存档界面] 刷新 {_rows.Count} 条用时 {timer.ElapsedMilliseconds} 毫秒");
			}
		}
		catch (Exception e)
		{
			PersistentModState.Log?.LogError("[存档界面] 刷新失败: " + e);
		}
	}

	private List<Entry> ManualEntries(bool includeEmpty) => SortManual(ManualEntriesBySlot(includeEmpty));

	// 设置里选了「最近在上」：「开始新游戏」仍在最上面，有存档的按保存时间从新到旧，空栏位放最后（按栏位号）。
	// 存档页最上面先放第一个空栏位，存新档不用滚到最底下，也不会只能覆盖最新那个
	private static List<Entry> SortManual(List<Entry> list)
	{
		if (_sortOrder != 1)
		{
			return list;
		}
		Entry firstEmpty = list.FirstOrDefault(e => e.Caption != NewGame && e.Data == null);
		return list.OrderBy(e => e.Caption == NewGame || e == firstEmpty ? 0 : e.Data != null ? 1 : 2)
			.ThenByDescending(e => e.Caption != NewGame && e.Data != null ? e.Data.TimeTick : 0L)
			.ToList();
	}


    private List<Entry> ManualEntriesBySlot(bool includeEmpty)
    {
        var list = new List<Entry>();
        bool addedStart = false;
        for (int i = 1; i <= ModSaveSlotPolicy.OfficialManualCount; i++)
        {
            string slot = _scope.Manual(i);
            CheckSlot(slot, !_isLoad);
            GameSave data = SaveSystem.Instance.GetSaveData(slot);
            bool start = _isLoad && data == null;
            if (start && (!_scope.FromTitle || addedStart)) continue;
            if (!_isLoad && data == null && !includeEmpty) continue;
            var entry = new Entry
            {
                Slot = slot, Label = "手动 " + i.ToString("000"), Data = data,
                Caption = start ? NewGame : "", Compact = start,
                CanNote = data != null, CanDelete = _isLoad && data != null
            };
            entry.Primary = () => Perform(entry, null, start);
            if (start) { list.Insert(0, entry); addedStart = true; } else list.Add(entry);
        }
        return list;
    }

    private List<Entry> AutoEntries()
    {
        var list = new List<Entry>();
        foreach (var pair in AutoGroups)
        {
            string kind = pair.Category;
            string slot = _scope.Auto(kind);
            CheckSlot(slot, false);
            GameSave data = SaveSystem.Instance.GetAutoSaveData(slot)?.GameSave;
            if (data == null) continue;
            var entry = new Entry { Slot = slot, Label = pair.Name + "自动存档", Caption = pair.Name,
                Data = data, Compact = true, CanNote = true, CanDelete = true, Latest = true };
            entry.Primary = () => Perform(entry, kind, false);
            list.Add(entry);
        }
        return list;
    }

    private void Perform(Entry entry, string kind, bool start)
    {
        if (_navigating) return;
        try
        {
            CheckSlot(entry.Slot, !_isLoad);
            if (!_isLoad)
            {
                var panel = (SaveGamePanel)_panel;
                if (SaveSystem.Instance.GetSaveData(entry.Slot) == null) panel.SaveData(entry.Slot);
                else panel.OpenConfirmPanel(entry.Slot);
                return;
            }
            if (start && SaveSystem.Instance.GetSaveData(entry.Slot) != null)
                throw new InvalidOperationException("该空栏位已有存档，请刷新后重新选择。");
            _navigating = true;
            Scope scope = _scope;
            if (scope.Addon)
            {
                if (start) Plugin.Instance.StartAddon(entry.Slot);
                else if (kind != null) Plugin.Instance.LoadAddonAuto(kind);
                else Plugin.Instance.LoadAddon(entry.Slot);
            }
            else
            {
                if (start) scope.Start(scope.Package, entry.Slot);
                else if (kind != null) scope.LoadAuto(scope.Package, kind);
                else scope.Load(scope.Package, entry.Slot);
                if (Plugin.Instance != null && !string.IsNullOrEmpty(Plugin.Instance.LastNavigationError))
                    throw new InvalidOperationException(Plugin.Instance.LastNavigationError);
            }
            Close();
        }
        catch (Exception ex) { _navigating = false; Report(ex); }
    }

	private static string SlotName(int index) => index.ToString().PadLeft(3, '0');

	internal static string SlotLabel(string slot)
	{
		try
		{
			string format = LocalizationManager.Instance.LocaleResolver.GetString("System/SaveSlotText");
			return (string.Format(format, slot));
		}
		catch
		{
			return slot;
		}
	}

	private static string GameDate(GameSave data)
	{
		try
		{
			var resolver = LocalizationManager.Instance.LocaleResolver;
			var time = new GameTime(data.CurrentYear, data.CurrentMonth, data.CurrentStage);
			return (string.Format(resolver.GetString("System/GameTimeText"), resolver.GetString(time.YearText), resolver.GetString(time.MonthText), resolver.GetString(time.StageText)));
		}
		catch
		{
			return $"第{data.CurrentYear}年{data.CurrentMonth}月";
		}
	}

	private static string SavedAt(GameSave data) => new DateTime(data.TimeTick).ToString("yyyy/MM/dd HH:mm");

	private static int Lover(GameSave data) => data?.Stats?.FirstOrDefault(s => s.Key == "lover")?.Value ?? 0;

	private static string LoverName(int lover)
	{
		try
		{
			string name = LocalizationManager.Instance.LocaleResolver.GetString("PlayerInfo/Lover/" + lover);
			return string.IsNullOrEmpty(name) || name.StartsWith("PlayerInfo/") ? "" : (name);
		}
		catch
		{
			return "";
		}
	}

	// ---------------- 列表行 ----------------

	private void GroupHeader(string text)
	{
		RectTransform rt = NewRect("Group", _listContent);
		rt.gameObject.AddComponent<LayoutElement>().preferredHeight = 46f;
		// 删除确认后游戏要在这一层找按钮，标题也挂一个不能点的按钮
		Button b = rt.gameObject.AddComponent<Button>();
		b.interactable = false;
		b.navigation = new Navigation { mode = Navigation.Mode.None };
		Text t = NewText(rt, "Text", _calligraphy, 30, Gold, TextAnchor.LowerLeft);
		Place(t.rectTransform, 8f, 0f, 240f, 42f);
		t.text = text;
		RectTransform line = NewRect("Line", rt);
		line.anchorMin = new Vector2(0, 0);
		line.anchorMax = new Vector2(1, 0);
		line.pivot = new Vector2(0, 0);
		line.offsetMin = new Vector2(80f, 10f);
		line.offsetMax = new Vector2(0f, 11f);
		line.gameObject.AddComponent<Image>().color = LineColor;
	}

	private Button Row(Entry e)
	{
		float height = e.Compact ? 76f : 118f;
		RectTransform rt = NewRect("Row", _listContent);
		rt.gameObject.AddComponent<LayoutElement>().preferredHeight = height;
		Image back = rt.gameObject.AddComponent<Image>();
		back.color = new Color(0, 0, 0, 0);
		Button button = rt.gameObject.AddComponent<Button>();
		button.transition = Selectable.Transition.None;
		button.onClick.AddListener(() => Act(e));
		if (e.LastSaved || e.Current)
		{
			// 上次保存、当前进度的存档：固定的底色，不随鼠标和选中移动，选中时的淡金底叠在它上面
			RectTransform mark = NewRect("LastMark", rt);
			mark.anchorMin = Vector2.zero;
			mark.anchorMax = Vector2.one;
			mark.offsetMin = Vector2.zero;
			mark.offsetMax = Vector2.zero;
			Image markImage = mark.gameObject.AddComponent<Image>();
			markImage.color = new Color(Gold.r, Gold.g, Gold.b, 0.16f);
			markImage.raycastTarget = false;
		}
		RectTransform accent = NewRect("Accent", rt);
		Place(accent, 0f, 8f, 3f, height - 16f);
		Image accentImage = accent.gameObject.AddComponent<Image>();
		accentImage.color = Gold;
		accentImage.enabled = false;
		RowView view = rt.gameObject.AddComponent<RowView>();
		view.Init(this, e, back, accentImage);

		float avatar = e.Compact ? 52f : 74f;
		AddAvatar(rt, e.Data, 14f, (height - avatar) / 2f - 2f, avatar);
		float x = 26f + avatar + 14f;
		if (e.Caption == NewGame)
		{
			Text title = NewText(rt, "Date", _calligraphy, 30, Txt, TextAnchor.UpperLeft);
			Place(title.rectTransform, 26f, 4f, 300f, 40f);
			title.text = "＋ 开始新游戏";
			Text where = NewText(rt, "Time", _serif, 18, Dim, TextAnchor.UpperLeft);
			Place(where.rectTransform, 30f, 44f, 400f, 26f);
			where.text = "存在空栏位 " + e.Label;
		}
		else if (e.Compact)
		{
			Text date = NewText(rt, "Date", _calligraphy, 30, e.Latest ? Txt : new Color(0.8f, 0.77f, 0.71f, 1f), TextAnchor.UpperLeft);
			Place(date.rectTransform, x, 4f, 300f, 40f);
			date.text = GameDate(e.Data);
			Text time = NewText(rt, "Time", _serif, 18, Dim, TextAnchor.UpperLeft);
			Place(time.rectTransform, x, 44f, 360f, 26f);
			time.supportRichText = true;
			time.text = SavedAt(e.Data) + "　" + e.Label + (e.Latest ? "　<color=#C9A26A>最新</color>" : "");
		}
		else
		{
			Text name = NewText(rt, "Name", _serif, 25, Gold, TextAnchor.UpperLeft);
			Place(name.rectTransform, x, 10f, 260f, 34f);
			// 星标和原来一样只标二周目（新周目）的存档
			name.text = e.Caption == "当前进度" ? "当前进度" : (e.Data != null && e.Data.NewGamePlus ? "★" : "") + e.Label;
			Text date = NewText(rt, "Date", _calligraphy, 36, e.Data == null ? Dim : Txt, TextAnchor.UpperLeft);
			Place(date.rectTransform, x + 140f, 4f, 360f, 46f);
			date.text = e.Data == null ? "空栏位" : GameDate(e.Data);
			Text time = NewText(rt, "Time", _serif, 20, Dim, TextAnchor.UpperLeft);
			Place(time.rectTransform, x, 50f, 400f, 28f);
			time.supportRichText = true;
			time.text = e.Data == null ? "" : SavedAt(e.Data) + (e.Caption == "当前进度" ? "　" + e.Label : "") + (e.LastSaved ? "　<color=#C9A26A>上次保存</color>" : "") + (e.Current ? "　<color=#C9A26A>当前进度</color>" : "");
			if (e.Data != null)
			{
				string note = e.CanNote ? ReadNote(e.Slot) : "";
				Text noteText = NewText(rt, "Note", _serif, 21, note.Length > 0 ? Gold : new Color(0.42f, 0.39f, 0.35f, 1f), TextAnchor.UpperLeft);
				Place(noteText.rectTransform, x - 6f, 80f, 0f, 30f);
				noteText.rectTransform.anchorMax = new Vector2(1f, 1f);
				noteText.rectTransform.offsetMax = new Vector2(-80f, noteText.rectTransform.offsetMax.y);
				noteText.text = note.Length > 0 ? "【" + note + "】" : (e.CanNote ? "（无备注）" : "");
				_rowNotes[e] = noteText;
			}
		}
		// 右边的小按钮：备注、删除
		float right = 12f;
		if (e.CanDelete && e.Data != null)
		{
			Button delete = TextButton(rt, "删除");
			AnchorRight((RectTransform)delete.transform, right, e.Compact ? 20f : 44f, 60f, 36f);
			delete.onClick.AddListener(() => Delete(e));
			right += 64f;
		}
		return button;
	}

	private void AddAvatar(RectTransform parent, GameSave data, float x, float y, float size)
	{
		if (data == null)
		{
			return;
		}
		Sprite icon = Avatar(Lover(data));
		if (icon == null)
		{
			return;
		}
		RectTransform ring = NewRect("Avatar", parent);
		Place(ring, x, y, size, size);
		Image ringImage = ring.gameObject.AddComponent<Image>();
		ringImage.sprite = Circle();
		ringImage.color = Gold;
		ringImage.raycastTarget = false;
		RectTransform mask = NewRect("Mask", ring);
		Stretch(mask);
		mask.offsetMin = new Vector2(2.5f, 2.5f);
		mask.offsetMax = new Vector2(-2.5f, -2.5f);
		Image maskImage = mask.gameObject.AddComponent<Image>();
		maskImage.sprite = Circle();
		maskImage.raycastTarget = false;
		mask.gameObject.AddComponent<Mask>().showMaskGraphic = false;
		RectTransform pic = NewRect("Icon", mask);
		Stretch(pic);
		Image picture = pic.gameObject.AddComponent<Image>();
		picture.sprite = icon;
		picture.preserveAspect = true;
		picture.raycastTarget = false;
	}

	/// <summary>列表一行：选中（键盘、手柄）或鼠标停上去时更新详情栏；实现 ILoadGameSlot，手柄的删除键照常能用。</summary>
	private sealed class RowView : MonoBehaviour, ISelectHandler, IDeselectHandler, IPointerEnterHandler, ILoadGameSlot
	{
		private ModNativeSaveBrowser _owner;
		private Entry _entry;
		private Image _back, _accent;

		public string Slot => _entry.CanDelete ? _entry.Slot : "";

		public bool IsEmpty => _entry.Data == null || !_entry.CanDelete;

		public void Init(ModNativeSaveBrowser owner, Entry entry, Image back, Image accent)
		{
			_owner = owner;
			_entry = entry;
			_back = back;
			_accent = accent;
		}

		public void OnSelect(BaseEventData eventData)
		{
			_owner.Focus(_entry, scroll: !(eventData is PointerEventData));
			Highlight(true);
		}

		public void OnDeselect(BaseEventData eventData) => Highlight(false);

		public void OnPointerEnter(PointerEventData eventData)
		{
			// 正在写备注时鼠标划过别的存档不切换，免得打到一半被打断
			if (Typing)
			{
				return;
			}
			if (EventSystem.current != null && EventSystem.current.currentSelectedGameObject != gameObject)
			{
				EventSystem.current.SetSelectedGameObject(gameObject, eventData);
			}
		}

		// 选中：淡淡的金色底，左边一道金线
		private void Highlight(bool on)
		{
			_back.color = on ? new Color(Gold.r, Gold.g, Gold.b, 0.1f) : new Color(0, 0, 0, 0);
			_accent.enabled = on;
		}
	}

	// ---------------- 详情栏 ----------------

	private void Focus(Entry e, bool scroll)
	{
		int index = _rows.FindIndex(r => r.Entry == e);
		if (index >= 0)
		{
			_selected = index;
			if (scroll)
			{
				ScrollTo((RectTransform)_rows[index].Button.transform);
			}
		}
		Show(e);
	}

	private void Show(Entry e)
	{
		_shown = e;
		GameSave data = e?.Data;
		_detail.gameObject.SetActive(e != null);
		if (e == null)
		{
			return;
		}
		int lover = Lover(data);
		_loverCaption.text = data == null ? "" : "心上人";
		_loverName.text = e.Caption == NewGame ? "新游戏" : data == null ? "空栏位" : lover == NoLover ? "还没有" : LoverName(lover);
		_date.text = data == null ? "" : GameDate(data);
		string kind = e.Caption == "当前进度" ? "当前进度" : e.Compact ? e.Caption + "自动存档" : "手动存档";
		_info.text = data == null ? e.Label : kind + " · " + e.Label + "\n存于 " + SavedAt(data);
		_mission.text = e.Caption == NewGame ? "在这个空栏位开始新的一局。" : data == null ? "存到这里会新建一个存档。" : Mission(data);
		ShowLines(data);
		_noteBox.gameObject.SetActive(e.CanNote && data != null);
		// 没有备注框（自动存档）时对白可以往下多占一块
		var linesSize = _lines.rectTransform.sizeDelta;
		_lines.rectTransform.sizeDelta = new Vector2(linesSize.x, _linesHeight + (_noteBox.gameObject.activeSelf ? 0f : 84f));
		string note = e.CanNote && data != null ? ReadNote(e.Slot) : "";
		_noteBox.SetTextWithoutNotify(note);
		SetButtonText(_primaryButton, e.Caption == NewGame ? "开始新游戏" : _isLoad ? "读取" : data == null ? "存到这里" : "覆盖这一栏");
		_deleteButton.gameObject.SetActive(e.CanDelete && data != null);
		ShowPortrait(data == null ? -1 : lover);

	}


    private static string Mission(GameSave data)
    {
        return data.CurrentScene == "Free" ? "养成" : data.CurrentScene == "Story" ? "剧情"
            : data.CurrentScene == "Combat" ? "决斗" : data.CurrentScene == "Battle" ? "战役" : "";
    }

	private float _linesHeight;


    private void ShowLines(GameSave data)
    {
        _section.text = data == null ? "" : data.CurrentScene == "Free" ? "读档后回到养成地图" : "存档时的对白";
        _lines.text = "";
        if (data == null || data.CurrentScene == "Free" || data.StoryLogs == null) return;
        var lines = new List<string>();
        for (int i = data.StoryLogs.Count - 1; i >= 0 && lines.Count < 3; i--)
        {
            StoryLogSave log = data.StoryLogs[i];
            if (log == null || log.Type != StoryLogType.Story || string.IsNullOrEmpty(log.Key)) continue;
            string text = null;
            if (_scope != null && _scope.Package != null) _scope.Package.Texts.TryGetValue(log.Key, out text);
            if (string.IsNullOrEmpty(text) && LocalizationManager.Instance != null)
            {
                string key = log.Key.StartsWith("Story/", StringComparison.Ordinal) ? log.Key : "Story/" + log.Key;
                text = LocalizationManager.Instance.LocaleResolver.GetString(key);
                if (text == key || text == log.Key) continue;
            }
            if (string.IsNullOrEmpty(text)) continue;
            text = Regex.Replace(text, "<[^>]*>|\\{[^}]*\\}", "").Trim();
            if (text.Length > 44) text = text.Substring(0, 44) + "…";
            if (text.Length > 0) lines.Add("「" + text + "」");
        }
        lines.Reverse();
        _lines.text = string.Join("\n", lines);
    }

	// ---------------- 动作 ----------------

	/// <summary>读档、存档、开新游戏。备注框刚关掉的半秒内不算：那一下点击是给「保存」的，不能落到后面的存档上。</summary>
	private static void Act(Entry e)
	{
		// 刚写完备注按的回车也不能落到存档上
		float noteEdited = _current != null ? _current._noteEditedAt : -10f;
		if (e == null || Typing || Time.unscaledTime - noteEdited < 0.5f)
		{
			return;
		}
		e.Primary?.Invoke();
	}

	private void Delete(Entry e)
	{
        if (_scope == null || e == null) return;
        try { CheckSlot(e.Slot, false); } catch (Exception ex) { Report(ex); return; }
		if (e == null || !e.CanDelete || e.Data == null || !_isLoad)
		{
			return;
		}
		// 游戏的删除确认框记下当前选中的物体，删完、取消后选回去：先选中这一行
		int index = _rows.FindIndex(r => r.Entry == e);
		if (index >= 0)
		{
			_selected = index;
			EventSystem.current?.SetSelectedGameObject(_rows[index].Button.gameObject);
		}
		((LoadGamePanel)_panel).OpenDeletePanel(e.Slot);
	}

	/// <summary>备注框写完（回车或点别处）：存下来，左边这一行的备注跟着改。</summary>
	private void SaveNote(string text)
	{
        if (_scope == null) return;
		_noteEditedAt = Time.unscaledTime;
		Entry e = _shown;
		if (e == null || !e.CanNote || e.Data == null)
		{
			return;
		}
		text = (text ?? "").Trim();
		if (text == ReadNote(e.Slot))
		{
			return;
		}
		WriteNote(e.Slot, text);
		if (_rowNotes.TryGetValue(e, out Text row) && row != null)
		{
			row.text = text.Length > 0 ? "【" + text + "】" : ("（无备注）");
			row.color = text.Length > 0 ? Gold : new Color(0.42f, 0.39f, 0.35f, 1f);
		}
	}

	/// <summary>正在备注框里打字：本 Mod 的热键不响应。</summary>
	public static bool Typing
	{
		get
		{
			GameObject selected = EventSystem.current != null ? EventSystem.current.currentSelectedGameObject : null;
			return selected != null && selected.GetComponent<InputField>() is InputField field && field.isFocused;
		}
	}

	private bool _pendingReselect;

	private void Reselect()
	{
		if (_rows.Count == 0 || !isActiveAndEnabled)
		{
			return;
		}
		// 改备注时输入事件暂停了，等恢复后再选
		if (EventSystem.current == null || !EventSystem.current.enabled)
		{
			_pendingReselect = true;
			return;
		}
		_pendingReselect = false;
		_selected = Mathf.Clamp(_selected, 0, _rows.Count - 1);
		EventSystem.current.SetSelectedGameObject(_rows[_selected].Button.gameObject);
	}

	private void Update()
	{
        if (_scope == null) return;
        if (_host == null || !_host.gameObject.activeInHierarchy) { DetachScope(); return; }
		if (_root == null || _scope == null)
		{
			return;
		}
		if (_pendingReselect)
		{
			Reselect();
		}
		if (!_isLoad)
		{
			return;
		}
		// 手柄 LB / RB 切换手动、自动存档
		Gamepad pad = Gamepad.current;
		if (pad != null && Traverse.Create(_panel).Field("_deletePanel").GetValue<CommonPanel>() is CommonPanel del && !IsShown(del))
		{
			if (pad.leftShoulder.wasPressedThisFrame || pad.rightShoulder.wasPressedThisFrame)
			{
				SwitchTab(1 - _tab);
			}
		}
	}

	private static bool IsShown(CommonPanel panel)
	{
		CanvasGroup g = panel.GetComponent<CanvasGroup>();
		return g != null ? g.alpha > 0.5f : panel.gameObject.activeInHierarchy;
	}

	private void SetNavigation()
	{
		for (int i = 0; i < _rows.Count; i++)
		{
			Button row = _rows[i].Button;
			var nav = new Navigation { mode = Navigation.Mode.Explicit };
			nav.selectOnUp = i > 0 ? _rows[i - 1].Button : (_tabs.Count > 0 ? _tabs[_tab] : null);
			nav.selectOnDown = i < _rows.Count - 1 ? _rows[i + 1].Button : null;
			Button[] small = row.GetComponentsInChildren<Button>().Where(b => b != row).ToArray();
			nav.selectOnRight = small.Length > 0 ? small[small.Length - 1] : _primaryButton;
			row.navigation = nav;
			// 小按钮：从右往左排的，左右在它们之间走，最右边再往右到详情栏
			for (int j = 0; j < small.Length; j++)
			{
				var n = new Navigation { mode = Navigation.Mode.Explicit, selectOnUp = nav.selectOnUp, selectOnDown = nav.selectOnDown };
				n.selectOnLeft = j < small.Length - 1 ? small[j + 1] : row;
				n.selectOnRight = j > 0 ? small[j - 1] : _primaryButton;
				small[j].navigation = n;
			}
		}
		Button first = _rows.Count > 0 ? _rows[0].Button : null;
		foreach (Button tab in _tabs)
		{
			tab.navigation = new Navigation { mode = Navigation.Mode.Explicit, selectOnDown = first, selectOnLeft = _tabs[0], selectOnRight = _tabs[_tabs.Count - 1] };
		}
		Selectable back = _rows.Count > 0 ? _rows[Mathf.Clamp(_selected, 0, _rows.Count - 1)].Button : null;
		foreach (Selectable b in new Selectable[] { _primaryButton, _deleteButton, _noteBox })
		{
			b.navigation = new Navigation { mode = Navigation.Mode.Explicit, selectOnLeft = back };
		}
		var primaryNav = _primaryButton.navigation;
		primaryNav.selectOnRight = _deleteButton;
		primaryNav.selectOnUp = _noteBox;
		_primaryButton.navigation = primaryNav;
		var deleteNav = _deleteButton.navigation;
		deleteNav.selectOnLeft = _primaryButton;
		deleteNav.selectOnUp = _noteBox;
		_deleteButton.navigation = deleteNav;
		var noteNav = _noteBox.navigation;
		noteNav.selectOnDown = _primaryButton;
		_noteBox.navigation = noteNav;
	}

	private void ScrollTo(RectTransform row)
	{
		Canvas.ForceUpdateCanvases();
		float viewHeight = _scroll.viewport.rect.height;
		float contentHeight = _listContent.rect.height;
		if (contentHeight <= viewHeight)
		{
			return;
		}
		float top = -row.anchoredPosition.y - row.rect.height * (1f - row.pivot.y);
		float bottom = top + row.rect.height;
		float offset = _listContent.anchoredPosition.y;
		if (top < offset)
		{
			offset = top;
		}
		else if (bottom > offset + viewHeight)
		{
			offset = bottom - viewHeight;
		}
		_listContent.anchoredPosition = new Vector2(_listContent.anchoredPosition.x, Mathf.Clamp(offset, 0f, contentHeight - viewHeight));
	}

	// ---------------- 立绘、头像 ----------------

	// 保留上游按人物/表情缓存的资源加载方式；本模块使用 normal 立绘，不移植剧情结缘图谱。
	private static readonly Dictionary<(int, string), AsyncOperationHandle<Sprite>> PortraitLoads = new Dictionary<(int, string), AsyncOperationHandle<Sprite>>();
	private (int, string) _wantedPortrait = (-2, null);



	private void ShowPortrait(int lover, bool bonded = false)
	{
		string mood = bonded && ShyPortraits.TryGetValue(lover, out string shy) ? shy : "normal";
		_wantedPortrait = (lover, mood);
		_portrait.enabled = false;
		if (!LoadPortrait(lover, mood, out AsyncOperationHandle<Sprite> handle))
		{
			return;
		}
		if (handle.IsDone)
		{
			ApplyPortrait(lover, mood, handle);
		}
		else
		{
			handle.Completed += h => ApplyPortrait(lover, mood, h);
		}
	}

	/// <summary>开始（或已经）读这位心上人的这张立绘；没有立绘返回 false。</summary>
	private static bool LoadPortrait(int lover, string mood, out AsyncOperationHandle<Sprite> handle)
	{
		handle = default;
		if (!Portraits.TryGetValue(lover, out string folder))
		{
			return false;
		}
		try
		{
			if (!PortraitLoads.TryGetValue((lover, mood), out handle))
			{
				handle = Addressables.LoadAssetAsync<Sprite>("Assets/__Project/Images/Characters/" + folder + "/" + mood + ".png");
				PortraitLoads[(lover, mood)] = handle;
			}
			return true;
		}
		catch (Exception e)
		{
			PersistentModState.Log?.LogInfo("[存档界面] 读立绘失败: " + e.Message);
			return false;
		}
	}

	/// <summary>列表里的小头像：游戏属性页那套心上人头像；心上人是「自己」（还没有）时不显示。</summary>
	private static Sprite Avatar(int lover) => lover == NoLover ? null : LoverIcon(lover);

	private void ApplyPortrait(int lover, string mood, AsyncOperationHandle<Sprite> handle)
	{
		if (this == null || _portrait == null || _wantedPortrait != (lover, mood))
		{
			return;
		}
		if (handle.Status == AsyncOperationStatus.Succeeded && handle.Result != null)
		{
			_portrait.sprite = handle.Result;
			_portrait.enabled = true;
			FitPortrait(handle.Result);
		}
		else if (mood != "normal")
		{
			// 害羞立绘读不到时退回 normal，不留空
			ShowPortrait(lover);
		}
	}

	private float _portraitCenter, _portraitHeight;

	/// <summary>立绘都画在同样大小的画布上：从画布 24% 处往下露出一半（头顶到腰），左右居中、不裁。</summary>
	private void FitPortrait(Sprite sprite)
	{
		Rect rect = sprite.rect;
		float picHeight = _portraitHeight / 0.5f, picWidth = picHeight * rect.width / rect.height;
		Place(_portrait.rectTransform, _portraitCenter - picWidth / 2f, -0.24f * picHeight, picWidth, picHeight);
	}

	private static int _iconSearchFrame = -1;

	/// <summary>属性页心上人头像用的图（在游戏启动时就载入的资源里，标题画面上也找得到）。</summary>
	private static readonly Dictionary<int, string> IconNames = new Dictionary<int, string>
	{
		{ 0, "icon_sister" }, { 1, "icon_girl1" }, { 2, "icon_girl2" }, { 3, "icon_girl5" }, { 4, "icon_girl9" },
		{ 5, "icon_girl6" }, { 6, "icon_girl7" }, { 7, "icon_girl8" }, { 8, "icon_girl4" },
	};

	private static Dictionary<int, Sprite> _icons;

	internal static Sprite LoverIcon(int lover)
	{
		if (_icons == null || _icons.Count < IconNames.Count)
		{
			// 找不到时同一帧里不再重复找（一次刷新要画几十个头像）
			if (_iconSearchFrame == Time.frameCount)
			{
				return _icons != null && _icons.TryGetValue(lover, out Sprite cached) ? cached : null;
			}
			_iconSearchFrame = Time.frameCount;
			_icons = new Dictionary<int, Sprite>();
			try
			{
				SpriteCollectionData collection = Resources.FindObjectsOfTypeAll<SpriteCollectionData>().FirstOrDefault(c => c.name == "LoverSpriteCollection");
				if (collection != null)
				{
					foreach (int key in IconNames.Keys)
					{
						if (collection.GetByKey(key.ToString()) is Sprite sprite)
						{
							_icons[key] = sprite;
						}
					}
				}
				if (_icons.Count < IconNames.Count)
				{
					var byName = IconNames.ToDictionary(p => p.Value, p => p.Key);
					foreach (Sprite sprite in Resources.FindObjectsOfTypeAll<Sprite>())
					{
						if (sprite != null && byName.TryGetValue(sprite.name, out int key) && !_icons.ContainsKey(key))
						{
							_icons[key] = sprite;
						}
					}
				}
				if (_icons.Count < IconNames.Count)
				{
					PersistentModState.Log?.LogInfo((object)$"[存档界面] 心上人头像只找到 {_icons.Count} 个（共 {IconNames.Count} 个）");
				}
			}
			catch (Exception e)
			{
				PersistentModState.Log?.LogInfo("[存档界面] 找心上人头像失败: " + e.Message);
			}
		}
		return _icons.TryGetValue(lover, out Sprite icon) ? icon : null;
	}

	private static Sprite _circle;

	private static Sprite Circle()
	{
		if (_circle != null)
		{
			return _circle;
		}
		const int n = 128;
		var tex = new Texture2D(n, n, TextureFormat.RGBA32, false) { wrapMode = TextureWrapMode.Clamp, filterMode = FilterMode.Bilinear };
		var pixels = new Color32[n * n];
		float r = n / 2f;
		for (int y = 0; y < n; y++)
		{
			for (int x = 0; x < n; x++)
			{
				float d = Mathf.Sqrt((x + 0.5f - r) * (x + 0.5f - r) + (y + 0.5f - r) * (y + 0.5f - r));
				byte a = (byte)(Mathf.Clamp01(r - d) * 255f);
				pixels[y * n + x] = new Color32(255, 255, 255, a);
			}
		}
		tex.SetPixels32(pixels);
		tex.Apply();
		_circle = Sprite.Create(tex, new Rect(0, 0, n, n), new Vector2(0.5f, 0.5f), 100f);
		return _circle;
	}

	// ---------------- 小工具 ----------------



	private Button MakeButton(Transform parent, string label, int fontSize, float x, float y, float w, float h)
	{
		Button b;
		if (_buttonTemplate != null)
		{
			b = CloneButton(_buttonTemplate, parent, label, "Button_" + label);
			var rt = (RectTransform)b.transform;
			Place(rt, x, y, w, h);
			rt.localScale = Vector3.one;
			// 边框图四周有一圈透明边：图片比按钮大一圈；点击范围只算按钮本身
			foreach (Image img in b.GetComponentsInChildren<Image>(includeInactive: true))
			{
				img.raycastTarget = false;
				if (img.transform != b.transform)
				{
					var irt = img.rectTransform;
					irt.anchorMin = irt.anchorMax = new Vector2(0.5f, 0.5f);
					irt.sizeDelta = new Vector2(w + 92f, h + 81f);
					irt.anchoredPosition = Vector2.zero;
				}
			}
			foreach (Text t in b.GetComponentsInChildren<Text>(includeInactive: true))
			{
				var trt = t.rectTransform;
				trt.anchorMin = trt.anchorMax = new Vector2(0.5f, 0.5f);
				trt.sizeDelta = new Vector2(w, h);
				trt.anchoredPosition = Vector2.zero;
				t.fontSize = fontSize;
				t.alignment = TextAnchor.MiddleCenter;
				t.horizontalOverflow = HorizontalWrapMode.Overflow;
			}
			Image hit = b.GetComponent<Image>() ?? b.gameObject.AddComponent<Image>();
			hit.color = new Color(0, 0, 0, 0);
			hit.raycastTarget = true;
		}
		else
		{
			RectTransform rt = NewRect("Button_" + label, parent);
			Place(rt, x, y, w, h);
			Image back = rt.gameObject.AddComponent<Image>();
			back.color = new Color(0.14f, 0.13f, 0.12f, 1f);
			rt.gameObject.AddComponent<Outline>().effectColor = new Color(Gold.r, Gold.g, Gold.b, 0.6f);
			b = rt.gameObject.AddComponent<Button>();
			b.targetGraphic = back;
			Text t = NewText(rt, "Text", _calligraphy, fontSize, Txt, TextAnchor.MiddleCenter);
			Stretch(t.rectTransform);
			t.text = label;
		}
		return b;
	}

	/// <summary>只有文字的小按钮（列表行里的备注、删除）：平时灰，鼠标停上去、选中时变亮。</summary>
	private Button TextButton(Transform parent, string label)
	{
		RectTransform rt = NewRect("Button_" + label, parent);
		Image hit = rt.gameObject.AddComponent<Image>();
		hit.color = new Color(0, 0, 0, 0);
		Button b = rt.gameObject.AddComponent<Button>();
		Text t = NewText(rt, "Text", _serif, 21, Color.white, TextAnchor.MiddleCenter);
		Stretch(t.rectTransform);
		t.text = label;
		b.targetGraphic = t;
		b.colors = new ColorBlock { normalColor = Dim, highlightedColor = Gold, selectedColor = Gold, pressedColor = Txt, disabledColor = Dim, colorMultiplier = 1f, fadeDuration = 0.08f };
		return b;
	}

	/// <summary>四条细线画边框（Outline 会把整块底色也描一遍，变成一片黄）。</summary>
	private static void Border(RectTransform rt, Color color)
	{
		void Edge(string name, Vector2 min, Vector2 max, Vector2 size)
		{
			RectTransform e = NewRect(name, rt);
			e.anchorMin = min;
			e.anchorMax = max;
			e.pivot = new Vector2(0.5f, 0.5f);
			e.sizeDelta = size;
			e.anchoredPosition = Vector2.zero;
			Image img = e.gameObject.AddComponent<Image>();
			img.color = color;
			img.raycastTarget = false;
		}
		Edge("Top", new Vector2(0, 1), new Vector2(1, 1), new Vector2(0, 1.5f));
		Edge("Bottom", new Vector2(0, 0), new Vector2(1, 0), new Vector2(0, 1.5f));
		Edge("Left", new Vector2(0, 0), new Vector2(0, 1), new Vector2(1.5f, 0));
		Edge("Right", new Vector2(1, 0), new Vector2(1, 1), new Vector2(1.5f, 0));
	}

	private static void SetButtonText(Button b, string label)
	{
		foreach (Text t in b.GetComponentsInChildren<Text>(includeInactive: true))
		{
			t.text = label;
		}
	}

	private static RectTransform NewRect(string name, Transform parent)
	{
		var go = new GameObject(name, typeof(RectTransform));
		go.layer = parent.gameObject.layer;
		var rt = (RectTransform)go.transform;
		rt.SetParent(parent, false);
		return rt;
	}

	/// <summary>按左上角放：x 向右、y 向下。</summary>
	private static void Place(RectTransform rt, float x, float y, float w, float h)
	{
		rt.anchorMin = rt.anchorMax = new Vector2(0f, 1f);
		rt.pivot = new Vector2(0f, 1f);
		rt.anchoredPosition = new Vector2(x, -y);
		rt.sizeDelta = new Vector2(w, h);
	}

	/// <summary>按右上角放：right 是离右边的距离。</summary>
	private static void AnchorRight(RectTransform rt, float right, float y, float w, float h)
	{
		rt.anchorMin = rt.anchorMax = new Vector2(1f, 1f);
		rt.pivot = new Vector2(1f, 1f);
		rt.anchoredPosition = new Vector2(-right, -y);
		rt.sizeDelta = new Vector2(w, h);
	}

	private static void Stretch(RectTransform rt)
	{
		rt.anchorMin = Vector2.zero;
		rt.anchorMax = Vector2.one;
		rt.offsetMin = rt.offsetMax = Vector2.zero;
	}

	private static Text NewText(Transform parent, string name, Font font, int size, Color color, TextAnchor anchor)
	{
		RectTransform rt = NewRect(name, parent);
		Text t = rt.gameObject.AddComponent<Text>();
		t.font = font;
		t.fontSize = size;
		t.color = color;
		t.alignment = anchor;
		t.horizontalOverflow = HorizontalWrapMode.Overflow;
		// 不截断：这套字体行高比字号大不少，框稍矮一点整行就不显示了
		t.verticalOverflow = VerticalWrapMode.Overflow;
		t.supportRichText = false;
		t.raycastTarget = false;
		return t;
	}

    private string ReadNote(string slot)
    {
        if (_scope == null) return "";
        CheckSlot(slot, false);
        return ModSaveNotes.Read(NotesRoot, CampaignId, _scope.Addon, slot).Text;
    }

    private void WriteNote(string slot, string text)
    {
        CheckSlot(slot, false);
        ModSaveNote note = ModSaveNotes.Read(NotesRoot, CampaignId, _scope.Addon, slot);
        note.Text = text ?? "";
        ModSaveNotes.Write(NotesRoot, CampaignId, _scope.Addon, slot, note);
    }

    // Minimal CloneInto equivalent used by the upstream layout. Keep native font
    // switchers and visual children; discard only text writers and copied inputs.
    private static Button CloneButton(Button template, Transform parent, string label, string objectName)
    {
        GameObject clone = Instantiate(template.gameObject, parent, false);
        clone.name = objectName;
        foreach (MonoBehaviour item in clone.GetComponentsInChildren<MonoBehaviour>(true))
        {
            if (item == null) continue;
            string type = item.GetType().Name;
            if (type == "LeanLocalizedText" || type == "LeanLocalizedTextMesh"
                || type == "LeanLocalizedTextMeshPro" || type == "LeanLocalizedTextMeshProUGUI"
                || item is InputButton) DestroyImmediate(item);
        }
        foreach (Text text in clone.GetComponentsInChildren<Text>(true)) text.text = label;
        Button button = clone.GetComponent<Button>();
        button.onClick = new Button.ButtonClickedEvent();
        button.interactable = true;
        clone.SetActive(true);
        return button;
    }

}


    [HarmonyPatch(typeof(SaveGamePanel), "UpdateSlot")]
    internal static class NativeSaveBrowserSaveRefreshPatch
    {
        private static void Postfix(SaveGamePanel __instance) { ModNativeSaveBrowser.RefreshOwner(__instance); }
    }
    [HarmonyPatch(typeof(LoadGamePanel), "UpdateSlot")]
    internal static class NativeSaveBrowserLoadRefreshPatch
    {
        private static void Postfix(LoadGamePanel __instance) { ModNativeSaveBrowser.RefreshOwner(__instance); }
    }
    [HarmonyPatch(typeof(LoadGamePanel), "OpenDeletePanel")]
    internal static class NativeSaveBrowserDeleteGuardPatch
    {
        private static bool Prefix(LoadGamePanel __instance, string slot) { return ModNativeSaveBrowser.Guard(__instance, slot, false); }
    }
    [HarmonyPatch(typeof(LoadGamePanel), "ConfirmDeleteSlot")]
    internal static class NativeSaveBrowserDeleteConfirmPatch
    {
        private static bool Prefix(LoadGamePanel __instance) { return !ModNativeSaveBrowser.DeleteConfirmed(__instance); }
    }
    [HarmonyPatch(typeof(SaveGamePanel), "OpenConfirmPanel")]
    internal static class NativeSaveBrowserConfirmGuardPatch
    {
        private static bool Prefix(SaveGamePanel __instance, string slot) { return ModNativeSaveBrowser.Guard(__instance, slot, true); }
    }
    [HarmonyPatch(typeof(SaveGamePanel), "ConfirmSaveSlot")]
    internal static class NativeSaveBrowserSaveGuardPatch
    {
        private static bool Prefix(SaveGamePanel __instance)
        {
            return ModNativeSaveBrowser.Guard(__instance, Traverse.Create(__instance).Field("_currentConfirmSlot").GetValue<string>(), true);
        }
    }
}
