using System;
using System.Collections.Generic;
using System.Linq;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using UnityEngine;
using UnityEngine.Events;
using UnityEngine.UI;

namespace MortalModHost
{
    internal static class CustomConsumableUi
    {
        internal static readonly Color Paper = new Color(.12f, .14f, .13f, .98f);
        internal static readonly Color Gold = new Color(.82f, .70f, .46f, 1f);
        internal static Text Label(Transform parent, string name, string text, Font font, int size)
        {
            var root = new GameObject(name, typeof(RectTransform), typeof(CanvasRenderer), typeof(Text));
            root.transform.SetParent(parent, false);
            var label = root.GetComponent<Text>();
            label.font = font; label.fontSize = size; label.text = text; label.color = new Color(.96f, .93f, .84f);
            label.alignment = TextAnchor.MiddleCenter; label.raycastTarget = false;
            var rect = label.rectTransform; rect.anchorMin = Vector2.zero; rect.anchorMax = Vector2.one;
            rect.offsetMin = new Vector2(8, 3); rect.offsetMax = new Vector2(-8, -3);
            return label;
        }
        internal static Button Button(Transform parent, string name, string text, Font font, UnityAction action)
        {
            var root = new GameObject(name, typeof(RectTransform), typeof(CanvasRenderer), typeof(Image), typeof(Button));
            root.transform.SetParent(parent, false);
            root.GetComponent<Image>().color = Paper;
            var border = root.AddComponent<Outline>(); border.effectColor = Gold; border.effectDistance = new Vector2(1, 1);
            var button = root.GetComponent<Button>();
            var colors = button.colors; colors.highlightedColor = new Color(1.18f, 1.18f, 1.08f); colors.pressedColor = new Color(.7f, .7f, .65f); button.colors = colors;
            Label(root.transform, "Label", text, font, 22);
            button.onClick.AddListener(action);
            return button;
        }
        internal static void Place(RectTransform rect, Vector2 anchor, Vector2 offset, Vector2 size)
        {
            rect.anchorMin = rect.anchorMax = anchor; rect.pivot = anchor; rect.anchoredPosition = offset; rect.sizeDelta = size;
        }
    }

    public sealed class CustomConsumableInventoryUi : MonoBehaviour
    {
        private ItemStatPanel _owner;
        private CustomConsumableRuntime.Entry _selected;
        private Button _use;
        private Text _label;
        private RectTransform _description;
        private Vector2 _originalDescriptionMin;
        private readonly Dictionary<int, ItemToggleButton> _rows = new Dictionary<int, ItemToggleButton>();

        internal static CustomConsumableInventoryUi Attach(ItemStatPanel owner)
        {
            var ui = owner.GetComponent<CustomConsumableInventoryUi>() ?? owner.gameObject.AddComponent<CustomConsumableInventoryUi>();
            ui._owner = owner;
            return ui;
        }
        internal void Rebuild()
        {
            _rows.Clear();
            var misc = Traverse.Create(_owner).Field("_misc").GetValue<ItemListPanel>();
            var template = Traverse.Create(_owner).Field("_itemPrefab").GetValue<ItemToggleButton>();
            foreach (var entry in CustomConsumableRuntime.Entries.Where(e => e.Item.CurrentCount > 0))
            {
                ItemToggleButton row = Instantiate(template);
                row.name = entry.Item.Id; row.Setup(entry.Item, misc.Content); misc.AddItem(row.gameObject);
                _rows[entry.Item.GetInstanceID()] = row;
            }
        }
        internal void Selected(IGameItem item)
        {
            CustomConsumableRuntime.Entry selected;
            _selected = CustomConsumableRuntime.TryGet(item as ItemData, out selected) ? selected : null;
            if (_selected != null) Build();
            Refresh();
        }
        private void Build()
        {
            if (_use != null) return;
            var fields = Traverse.Create(_owner);
            var detail = fields.Field("_detailPanel").GetValue<CommonPanel>();
            Font font = fields.Field("_descText").GetValue<Text>().font;
            _description = fields.Field("_descText").GetValue<Text>().rectTransform;
            _originalDescriptionMin = _description.offsetMin;
            _use = CustomConsumableUi.Button(detail.transform, "MortalModHost_UseConsumable", "使用", font, Use);
            CustomConsumableUi.Place((RectTransform)_use.transform, new Vector2(1, 0), new Vector2(-20, 18), new Vector2(300, 54));
            _label = _use.GetComponentInChildren<Text>();
        }
        private void Refresh()
        {
            if (_use == null) return;
            bool show = _selected != null && _selected.Item != null && ModPlayModeSession.Allows(_selected.Package);
            _use.gameObject.SetActive(show);
            if (_description != null) _description.offsetMin = _originalDescriptionMin + (show ? new Vector2(0, 72) : Vector2.zero);
            if (!show) return;
            string reason = CustomConsumableRuntime.FieldUnavailable(_selected);
            _use.interactable = reason == null;
            _label.text = reason ?? ("使用 · 剩余 " + _selected.Item.CurrentCount);
            Traverse.Create(_owner).Field("_itemSealText").GetValue<Text>().text = "消耗品";
        }
        private void Use()
        {
            var selected = _selected;
            if (selected == null) return;
            try
            {
                CustomConsumableRuntime.UseField(selected.Package, selected.Reference);
                var fields = Traverse.Create(_owner);
                int tab = fields.Field("_currentToggleIndex").GetValue<int>();
                AccessTools.Method(typeof(ItemStatPanel), "UpdateData").Invoke(_owner, null);
                Toggle[] tabs = fields.Field("_toggleButtons").GetValue<Toggle[]>();
                if (tab >= 0 && tab < tabs.Length) { tabs[tab].isOn = true; tabs[tab].onValueChanged.Invoke(true); }
                ItemToggleButton row;
                if (_rows.TryGetValue(selected.Item.GetInstanceID(), out row))
                    Traverse.Create(row).Field("_toggle").GetValue<Toggle>().isOn = true;
                else { _selected = null; _owner.ShowDetail(false); }
                Refresh();
            }
            catch (Exception error)
            {
                if (_label != null) _label.text = error.Message;
                LuaManagerPatch.Log?.LogError("使用自定义消耗品失败：" + error);
            }
        }
        private void Update() { Refresh(); }
        private void OnDisable()
        {
            if (_description != null) _description.offsetMin = _originalDescriptionMin;
        }
    }

    public sealed class CustomConsumableBattleUi : MonoBehaviour
    {
        private Button _button;
        private RectTransform _panel, _content;
        private Font _font;
        private readonly List<GameObject> _rows = new List<GameObject>();
        private void Start()
        {
            var actions = Traverse.Create(GetComponent<CombatUIManager>()).Field("_actionPanel").GetValue<CombatActionButtonPanel>();
            Canvas canvas = actions.GetComponentInParent<Canvas>();
            var attackTemplate = Traverse.Create(actions).Field("_attackButton").GetValue<CombatActionButton>();
            _font = Traverse.Create(attackTemplate).Field("_tipText").GetValue<Text>().font;
            _button = CustomConsumableUi.Button(canvas.transform, "MortalModHost_CombatConsumables", "道具", _font, Toggle);
            CustomConsumableUi.Place((RectTransform)_button.transform, new Vector2(1, 0), new Vector2(-28, 28), new Vector2(210, 54));
            var root = new GameObject("MortalModHost_CombatConsumableList", typeof(RectTransform), typeof(CanvasRenderer), typeof(Image));
            root.transform.SetParent(canvas.transform, false); root.GetComponent<Image>().color = CustomConsumableUi.Paper;
            _panel = root.GetComponent<RectTransform>();
            CustomConsumableUi.Place(_panel, new Vector2(1, 0), new Vector2(-28, 96), new Vector2(420, 470));
            Text title = CustomConsumableUi.Label(root.transform, "Title", "决斗道具", _font, 25);
            CustomConsumableUi.Place(title.rectTransform, new Vector2(.5f, 1), new Vector2(0, -10), new Vector2(400, 40));
            Text hint = CustomConsumableUi.Label(root.transform, "Hint", "使用占用本回合行动，并按休息规则恢复气力", _font, 18);
            CustomConsumableUi.Place(hint.rectTransform, new Vector2(.5f, 1), new Vector2(0, -54), new Vector2(392, 58));
            var view = new GameObject("List", typeof(RectTransform), typeof(CanvasRenderer), typeof(Image), typeof(RectMask2D), typeof(ScrollRect));
            view.transform.SetParent(root.transform, false);
            var viewRect = (RectTransform)view.transform; viewRect.anchorMin = Vector2.zero; viewRect.anchorMax = Vector2.one;
            viewRect.offsetMin = new Vector2(12, 12); viewRect.offsetMax = new Vector2(-12, -122);
            view.GetComponent<Image>().color = new Color(0, 0, 0, .12f);
            var content = new GameObject("Content", typeof(RectTransform), typeof(VerticalLayoutGroup), typeof(ContentSizeFitter));
            content.transform.SetParent(view.transform, false); _content = (RectTransform)content.transform;
            _content.anchorMin = new Vector2(0, 1); _content.anchorMax = new Vector2(1, 1); _content.pivot = new Vector2(.5f, 1); _content.sizeDelta = Vector2.zero;
            var layout = content.GetComponent<VerticalLayoutGroup>(); layout.spacing = 10; layout.childControlHeight = true; layout.childForceExpandHeight = false;
            content.GetComponent<ContentSizeFitter>().verticalFit = ContentSizeFitter.FitMode.PreferredSize;
            ScrollRect scroll = view.GetComponent<ScrollRect>(); scroll.viewport = viewRect; scroll.content = _content; scroll.horizontal = false;
            _panel.gameObject.SetActive(false);
        }
        private void Toggle()
        {
            if (_panel.gameObject.activeSelf) { _panel.gameObject.SetActive(false); return; }
            foreach (GameObject row in _rows) Destroy(row); _rows.Clear();
            foreach (var entry in CustomConsumableRuntime.Entries.Where(e => e.Content.Gameplay.UseIn == "combat" && e.Item.CurrentCount > 0).ToArray())
            {
                var captured = entry;
                string reason = CustomConsumableRuntime.CombatUnavailable(entry);
                Button row = CustomConsumableUi.Button(_content, entry.Content.Id,
                    entry.Content.Name + " ×" + entry.Item.CurrentCount + (reason != null ? "\n" + reason : ""), _font, () => Use(captured));
                row.gameObject.AddComponent<LayoutElement>().preferredHeight = 76; row.interactable = reason == null; _rows.Add(row.gameObject);
            }
            _panel.gameObject.SetActive(true);
        }
        private void Use(CustomConsumableRuntime.Entry entry)
        {
            string reason = CustomConsumableRuntime.CombatUnavailable(entry);
            if (reason != null) return;
            _panel.gameObject.SetActive(false);
            StartCoroutine(CustomConsumableRuntime.UseCombat(entry.Package, entry.Reference));
        }
        private void Update()
        {
            if (_button == null) return;
            bool available = CustomConsumableRuntime.Entries.Any(e => e.Content.Gameplay.UseIn == "combat");
            _button.gameObject.SetActive(available);
            string phase = CustomConsumableRuntime.CombatPhaseUnavailable();
            _button.interactable = available && phase == null;
            if ((!available || phase != null) && _panel != null) _panel.gameObject.SetActive(false);
        }
        private void OnDestroy()
        {
            if (_button != null) Destroy(_button.gameObject);
            if (_panel != null) Destroy(_panel.gameObject);
        }
    }

    [HarmonyPatch(typeof(ItemStatPanel), "UpdateData")]
    internal static class CustomConsumableInventoryListPatch
    {
        private static void Postfix(ItemStatPanel __instance)
        {
            if (ModPlayModeSession.Selected != ModPlayModeKind.Original && CustomConsumableRuntime.Entries.Any())
                CustomConsumableInventoryUi.Attach(__instance).Rebuild();
        }
    }
    [HarmonyPatch(typeof(ItemStatPanel), "OnItemSelected")]
    internal static class CustomConsumableInventorySelectionPatch
    {
        private static void Postfix(ItemStatPanel __instance, IGameItem item)
        {
            var view = __instance.GetComponent<CustomConsumableInventoryUi>();
            if (view != null) view.Selected(item);
        }
    }
    [HarmonyPatch(typeof(CombatUIManager), "Start")]
    internal static class CustomConsumableBattlePanelPatch
    {
        private static void Postfix(CombatUIManager __instance)
        {
            if (ModPlayModeSession.Selected == ModPlayModeKind.Original
                || !CustomConsumableRuntime.Entries.Any(e => e.Content.Gameplay.UseIn == "combat")) return;
            if (__instance.GetComponent<CustomConsumableBattleUi>() == null) __instance.gameObject.AddComponent<CustomConsumableBattleUi>();
        }
    }
}
