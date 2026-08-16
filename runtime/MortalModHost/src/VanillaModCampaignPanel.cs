using System;
using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// 临时接管标题场景的原版 LoadGamePanel/LoadSlotPanel，把原版槽位预制体、
    /// 字体、悬停、音效、滚动与手柄导航复用于 MOD 战役。关闭时重新执行原版
    /// CreateSlot/SetButtonNavigation，普通 001~020 读档不会被永久改写。
    /// </summary>
    internal static class VanillaModCampaignPanel
    {
        private static readonly FieldInfo TitleSlotPanelField =
            AccessTools.Field(typeof(TitleManager), "_slotPanel");
        private static readonly FieldInfo SaveSlotsField =
            AccessTools.Field(typeof(LoadGamePanel), "_saveSlots");
        private static readonly FieldInfo AutoSaveSlotsField =
            AccessTools.Field(typeof(LoadGamePanel), "_autoSaveSlot");
        private static readonly FieldInfo RecentSaveField =
            AccessTools.Field(typeof(LoadGamePanel), "_recentSavePanel");
        private static readonly MethodInfo CreateSlotMethod =
            AccessTools.Method(typeof(LoadGamePanel), "CreateSlot");
        private static readonly MethodInfo SetNavigationMethod =
            AccessTools.Method(typeof(LoadGamePanel), "SetButtonNavigation");

        private static readonly FieldInfo SlotLabelField =
            AccessTools.Field(typeof(LoadSlotPanel), "_slotText");
        private static readonly FieldInfo TitleLabelsField =
            AccessTools.Field(typeof(LoadSlotPanel), "_titleText");
        private static readonly FieldInfo TimeLabelsField =
            AccessTools.Field(typeof(LoadSlotPanel), "_timeText");
        private static readonly FieldInfo DeleteButtonField =
            AccessTools.Field(typeof(LoadSlotPanel), "_deleteButton");
        private static readonly FieldInfo FocusObjectField =
            AccessTools.Field(typeof(LoadSlotPanel), "_focusObj");
        private static readonly FieldInfo PlusIconField =
            AccessTools.Field(typeof(LoadSlotPanel), "_newGamePlusIcon");

        private sealed class CampaignSave
        {
            public ModPackage Package;
            public GameSave Save;
        }

        private static LoadGamePanel _panel;
        private static CommonPanel _commonPanel;
        private static LoadSlotPanel[] _slots;
        private static AutoSaveSlotPanel[] _autoSlots;
        private static RecentSaveSlotPanel _recentSlot;
        private static bool[] _autoVisibility;
        private static bool _recentVisibility;
        private static readonly List<ModPackage> Campaigns = new List<ModPackage>();
        private static Action<ModPackage> _startCampaign;
        private static Action<ModPackage> _loadCampaign;
        private static Action<string> _logInfo;
        private static Action<string> _logWarning;
        private static bool _active;

        internal static bool IsActive { get { return _active; } }

        internal static bool Open(
            IList<ModPackage> packages,
            Action<ModPackage> startCampaign,
            Action<ModPackage> loadCampaign,
            Action<string> logInfo,
            Action<string> logWarning)
        {
            Remove();
            if (startCampaign == null) throw new ArgumentNullException(nameof(startCampaign));
            if (loadCampaign == null) throw new ArgumentNullException(nameof(loadCampaign));
            Campaigns.Clear();
            if (packages != null)
            {
                for (int i = 0; i < packages.Count; i++)
                {
                    ModPackage package = packages[i];
                    if (package != null && package.Campaign != null
                        && package.Campaign.NewGame)
                        Campaigns.Add(package);
                }
            }
            Campaigns.Sort(delegate(ModPackage left, ModPackage right)
            {
                return string.Compare(left.Id, right.Id, StringComparison.Ordinal);
            });

            if (Campaigns.Count == 0)
            {
                if (logWarning != null) logWarning(I18n.T("campaign.none"));
                return false;
            }

            try
            {
                TitleManager title = TitleManager.Instance;
                if (title == null)
                    throw new InvalidOperationException("TitleManager 尚未就绪");
                _commonPanel = TitleSlotPanelField != null
                    ? TitleSlotPanelField.GetValue(title) as CommonPanel : null;
                if (_commonPanel == null)
                    throw new InvalidOperationException("找不到原版 TitleManager._slotPanel");
                // 先保存面板引用再调用原版打开逻辑。这样 OpenSlot 中途失败时，
                // catch/Remove 仍能关闭并恢复原版槽位，不会把标题页留在半开状态。
                title.OpenSlot();

                _panel = LoadGamePanel.Instance;
                if (_panel == null && _commonPanel != null)
                    _panel = _commonPanel.GetComponentInChildren<LoadGamePanel>(true);
                if (_panel == null || _commonPanel == null)
                    throw new InvalidOperationException("找不到原版 LoadGamePanel/_slotPanel");

                _slots = SaveSlotsField != null
                    ? SaveSlotsField.GetValue(_panel) as LoadSlotPanel[] : null;
                if (_slots == null || _slots.Length < 2)
                    throw new InvalidOperationException("原版读档槽不足，无法保留新战役入口");

                _autoSlots = AutoSaveSlotsField != null
                    ? AutoSaveSlotsField.GetValue(_panel) as AutoSaveSlotPanel[] : null;
                _recentSlot = RecentSaveField != null
                    ? RecentSaveField.GetValue(_panel) as RecentSaveSlotPanel : null;
                RememberAndHideAuxiliarySlots();

                _startCampaign = startCampaign;
                _loadCampaign = loadCampaign;
                _logInfo = logInfo;
                _logWarning = logWarning;
                _active = true;
                _panel.enabled = false;
                RenderSaveSlots();
                if (_logInfo != null)
                    _logInfo("已用原版 LoadGamePanel 打开 MOD 战役存档页。");
                return true;
            }
            catch (Exception ex)
            {
                if (logWarning != null)
                    logWarning("原版 MOD 战役存档页建立失败，将使用兼容菜单：" + ex.Message);
                Remove();
                return false;
            }
        }

        internal static void Maintain()
        {
            if (!_active) return;
            if (_commonPanel == null || !_commonPanel.gameObject.activeInHierarchy)
                RestoreOriginalSlots();
        }

        internal static void Remove()
        {
            CommonPanel opened = _commonPanel;
            RestoreOriginalSlots();
            if (opened != null && opened.gameObject.activeInHierarchy)
            {
                try { opened.Show(false); }
                catch { }
            }
        }

        private static void RenderSaveSlots()
        {
            var saves = new List<CampaignSave>();
            SaveSystem saveSystem = SaveSystem.Instance;
            for (int i = 0; i < Campaigns.Count; i++)
            {
                GameSave data = null;
                try { data = saveSystem != null ? saveSystem.GetSaveData("mod_" + Campaigns[i].Id) : null; }
                catch (Exception ex)
                {
                    if (_logWarning != null)
                        _logWarning("读取 MOD 战役槽失败（" + Campaigns[i].Id + "）：" + ex.Message);
                }
                if (data != null)
                    saves.Add(new CampaignSave { Package = Campaigns[i], Save = data });
            }
            saves.Sort(delegate(CampaignSave left, CampaignSave right)
            {
                return right.Save.TimeTick.CompareTo(left.Save.TimeTick);
            });

            int row = 0;
            int savedLimit = Math.Min(saves.Count, _slots.Length - 1);
            for (; row < savedLimit; row++)
            {
                CampaignSave entry = saves[row];
                string when = "";
                try { when = new DateTime(entry.Save.TimeTick).ToString("yyyy/MM/dd HH:mm:ss"); }
                catch { }
                ConfigureSlot(
                    _slots[row],
                    I18n.T("campaign.slot", row + 1),
                    I18n.T("campaign.continue", ModDisclosurePolicy.SafePackageName(entry.Package)),
                    when,
                    delegate { SelectAndClose(delegate { _loadCampaign(entry.Package); }); });
            }

            ConfigureSlot(
                _slots[row++],
                I18n.T("campaign.new_slot"),
                I18n.T("campaign.choose_new"),
                I18n.T("campaign.choose_hint"),
                ShowCampaignPicker);
            HideRemaining(row);
            SelectFirstSlot();
        }

        private static void ShowCampaignPicker()
        {
            if (!_active) return;
            int row = 0;
            ConfigureSlot(
                _slots[row++],
                I18n.T("campaign.back"),
                I18n.T("campaign.back_to_saves"),
                "",
                RenderSaveSlots);

            SaveSystem saveSystem = SaveSystem.Instance;
            for (int i = 0; i < Campaigns.Count && row < _slots.Length; i++)
            {
                ModPackage package = Campaigns[i];
                bool hasSave = false;
                try { hasSave = saveSystem != null && saveSystem.GetSaveData("mod_" + package.Id) != null; }
                catch (Exception ex)
                {
                    // 损坏或旧版本存档不能把战役从新建列表永久隐藏。新建时原版
                    // SaveGameData 会覆盖该 MOD 自己的隔离槽，不会碰到官方槽。
                    if (_logWarning != null)
                        _logWarning("读取 MOD 战役槽失败，将允许重建（" + package.Id + "）：" + ex.Message);
                    hasSave = false;
                }
                if (hasSave) continue;
                string detail = ModDisclosurePolicy.SafePackageDescription(package);
                if (string.IsNullOrEmpty(detail))
                    detail = I18n.T("campaign.author", ModDisclosurePolicy.SafePackageAuthor(package));
                ModPackage captured = package;
                ConfigureSlot(
                    _slots[row],
                    I18n.T("campaign.option", row),
                    ModDisclosurePolicy.SafePackageName(package),
                    detail,
                    delegate { SelectAndClose(delegate { _startCampaign(captured); }); });
                row++;
            }
            if (row == 1)
            {
                ConfigureSlot(
                    _slots[row++],
                    I18n.T("campaign.new_slot"),
                    I18n.T("campaign.no_unused"),
                    I18n.T("campaign.no_unused_hint"),
                    RenderSaveSlots,
                    false);
            }
            HideRemaining(row);
            SelectFirstSlot();
        }

        private static void ConfigureSlot(
            LoadSlotPanel slot,
            string slotLabel,
            string title,
            string detail,
            Action click,
            bool interactable = true)
        {
            if (slot == null) return;
            slot.gameObject.SetActive(true);
            SetText(SlotLabelField != null ? SlotLabelField.GetValue(slot) as Text : null, slotLabel);
            SetTexts(TitleLabelsField != null ? TitleLabelsField.GetValue(slot) as Text[] : null, title);
            SetTexts(TimeLabelsField != null ? TimeLabelsField.GetValue(slot) as Text[] : null, detail);
            SetActive(DeleteButtonField != null ? DeleteButtonField.GetValue(slot) as Component : null, false);
            SetActive(FocusObjectField != null ? FocusObjectField.GetValue(slot) as GameObject : null, false);
            SetActive(PlusIconField != null ? PlusIconField.GetValue(slot) as GameObject : null, false);
            Button button = slot.SlotButton;
            if (button == null)
                throw new InvalidOperationException("原版 LoadSlotPanel 缺少 SlotButton");
            button.onClick = new Button.ButtonClickedEvent();
            if (click != null) button.onClick.AddListener(delegate { click(); });
            button.interactable = interactable;
            button.navigation = new Navigation { mode = Navigation.Mode.Automatic };
        }

        private static void SelectAndClose(Action action)
        {
            if (!_active || action == null) return;
            CommonPanel panel = _commonPanel;
            RestoreOriginalSlots();
            if (panel != null) panel.Show(false);
            action();
        }

        private static void HideRemaining(int first)
        {
            for (int i = first; i < _slots.Length; i++)
                if (_slots[i] != null) _slots[i].gameObject.SetActive(false);
        }

        private static void SelectFirstSlot()
        {
            if (_slots != null && _slots.Length > 0 && _slots[0] != null
                && _slots[0].SlotButton != null)
                _slots[0].SlotButton.Select();
        }

        private static void RememberAndHideAuxiliarySlots()
        {
            if (_autoSlots != null)
            {
                _autoVisibility = new bool[_autoSlots.Length];
                for (int i = 0; i < _autoSlots.Length; i++)
                {
                    _autoVisibility[i] = _autoSlots[i] != null && _autoSlots[i].gameObject.activeSelf;
                    if (_autoSlots[i] != null) _autoSlots[i].gameObject.SetActive(false);
                }
            }
            _recentVisibility = _recentSlot != null && _recentSlot.gameObject.activeSelf;
            if (_recentSlot != null) _recentSlot.gameObject.SetActive(false);
        }

        private static void RestoreOriginalSlots()
        {
            LoadGamePanel panel = _panel;
            try
            {
                if (panel != null)
                {
                    panel.enabled = true;
                    if (CreateSlotMethod != null) CreateSlotMethod.Invoke(panel, null);
                    if (_autoSlots != null && _autoVisibility != null)
                    {
                        for (int i = 0; i < _autoSlots.Length && i < _autoVisibility.Length; i++)
                            if (_autoSlots[i] != null) _autoSlots[i].gameObject.SetActive(_autoVisibility[i]);
                    }
                    if (_recentSlot != null) _recentSlot.gameObject.SetActive(_recentVisibility);
                    if (SetNavigationMethod != null) SetNavigationMethod.Invoke(panel, null);
                }
            }
            catch (Exception ex)
            {
                if (_logWarning != null)
                    _logWarning("恢复原版读档槽失败；离开标题场景后会由原版场景重建：" + ex.Message);
            }
            _active = false;
            _panel = null;
            _commonPanel = null;
            _slots = null;
            _autoSlots = null;
            _recentSlot = null;
            _autoVisibility = null;
            _startCampaign = null;
            _loadCampaign = null;
            _logInfo = null;
            _logWarning = null;
            Campaigns.Clear();
        }

        private static void SetText(Text label, string value)
        {
            if (label != null) label.text = value ?? "";
        }

        private static void SetTexts(Text[] labels, string value)
        {
            if (labels == null) return;
            for (int i = 0; i < labels.Length; i++) SetText(labels[i], value);
        }

        private static void SetActive(Component component, bool active)
        {
            if (component != null) component.gameObject.SetActive(active);
        }

        private static void SetActive(GameObject owner, bool active)
        {
            if (owner != null) owner.SetActive(active);
        }
    }
}
