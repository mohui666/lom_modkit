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
    /// 临时接管标题场景的原版 LoadGamePanel/LoadSlotPanel，把原版 001~020 槽位
    /// 预制体、字体、悬停、音效、滚动与手柄导航复用于当前 MOD。关闭时重新执行
    /// CreateSlot/SetButtonNavigation，普通原版读档不会被永久改写。
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
        private static readonly FieldInfo RecentSlotLabelField =
            AccessTools.Field(typeof(RecentSaveSlotPanel), "_slotText");
        private static readonly FieldInfo RecentTitleField =
            AccessTools.Field(typeof(RecentSaveSlotPanel), "_titleText");
        private static readonly FieldInfo RecentTimeField =
            AccessTools.Field(typeof(RecentSaveSlotPanel), "_timeText");
        private static readonly FieldInfo RecentButtonField =
            AccessTools.Field(typeof(RecentSaveSlotPanel), "_slotButton");
        private static readonly FieldInfo AutoSlotLabelField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_slotText");
        private static readonly FieldInfo AutoTitleField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_titleText");
        private static readonly FieldInfo AutoTimeField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_timeText");
        private static readonly FieldInfo AutoDeleteField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_deleteButton");
        private static readonly FieldInfo AutoPlusIconField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_newGamePlusIcon");
        private static readonly FieldInfo AutoSlotKindField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_slot");
        private static readonly FieldInfo AutoCurrentDataField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_currentData");
        private static readonly FieldInfo AutoMenuToggleField =
            AccessTools.Field(typeof(AutoSaveSlotPanel), "_menuToggleButton");

        private static LoadGamePanel _panel;
        private static CommonPanel _commonPanel;
        private static LoadSlotPanel[] _slots;
        private static AutoSaveSlotPanel[] _autoSlots;
        private static RecentSaveSlotPanel _recentSlot;
        private static bool[] _autoVisibility;
        private static bool _recentVisibility;
        private static readonly List<ModPackage> Campaigns = new List<ModPackage>();
        private static readonly Dictionary<LoadSlotPanel, Action> ManualSlotActions =
            new Dictionary<LoadSlotPanel, Action>();
        private static Action<ModPackage, string> _startCampaign;
        private static Action<ModPackage, string> _loadCampaign;
        private static Action<ModPackage, string> _loadAutoCampaign;
        private static Action<string> _logInfo;
        private static Action<string> _logWarning;
        private static Action<string> _campaignSelected;
        private static CampaignMenuFlow _flow;
        private static bool _active;
        private static bool _openedFromTitle;

        internal static bool IsActive { get { return _active; } }

        internal static string SelectedCampaignId
        {
            get
            {
                return _active && _flow != null ? _flow.SelectedCampaignId : null;
            }
        }

        internal static bool Open(
            IList<ModPackage> packages,
            Action<ModPackage, string> startCampaign,
            Action<ModPackage, string> loadCampaign,
            Action<ModPackage, string> loadAutoCampaign,
            string recentCampaignId,
            Action<string> campaignSelected,
            Action<string> logInfo,
            Action<string> logWarning)
        {
            Remove();
            if (startCampaign == null) throw new ArgumentNullException(nameof(startCampaign));
            if (loadCampaign == null) throw new ArgumentNullException(nameof(loadCampaign));
            if (loadAutoCampaign == null) throw new ArgumentNullException(nameof(loadAutoCampaign));
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

                _autoSlots = CollectOfficialAutoSlots(_panel);
                _recentSlot = RecentSaveField != null
                    ? RecentSaveField.GetValue(_panel) as RecentSaveSlotPanel : null;
                RememberAndHideAuxiliarySlots();

                _openedFromTitle = true;
                return BindOpenedPanel(
                    startCampaign, loadCampaign, loadAutoCampaign,
                    recentCampaignId, campaignSelected, logInfo, logWarning,
                    "");
            }
            catch (Exception ex)
            {
                if (logWarning != null)
                    logWarning("原版 MOD 战役存档页建立失败；为避免混用非官方 UI，本次不打开战役页：" + ex.Message);
                Remove();
                return false;
            }
        }

        internal static bool OpenExisting(
            CommonPanel host,
            LoadGamePanel panel,
            IList<ModPackage> packages,
            Action<ModPackage, string> startCampaign,
            Action<ModPackage, string> loadCampaign,
            Action<ModPackage, string> loadAutoCampaign,
            string recentCampaignId,
            string preferredCampaignId,
            Action<string> campaignSelected,
            Action<string> logInfo,
            Action<string> logWarning)
        {
            Remove();
            if (startCampaign == null) throw new ArgumentNullException(nameof(startCampaign));
            if (loadCampaign == null) throw new ArgumentNullException(nameof(loadCampaign));
            if (loadAutoCampaign == null) throw new ArgumentNullException(nameof(loadAutoCampaign));
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
            try
            {
                if (host == null || panel == null)
                    throw new InvalidOperationException("游戏内读档面板不可用");
                _commonPanel = host;
                _panel = panel;
                if (!_commonPanel.gameObject.activeInHierarchy)
                    _commonPanel.Show(true);
                _slots = SaveSlotsField != null
                    ? SaveSlotsField.GetValue(_panel) as LoadSlotPanel[] : null;
                if (_slots == null || _slots.Length < 2)
                    throw new InvalidOperationException("原版读档槽不足，无法打开 MOD 存档页");
                _autoSlots = CollectOfficialAutoSlots(_panel);
                _recentSlot = RecentSaveField != null
                    ? RecentSaveField.GetValue(_panel) as RecentSaveSlotPanel : null;
                RememberAndHideAuxiliarySlots();
                _openedFromTitle = false;
                return BindOpenedPanel(
                    startCampaign, loadCampaign, loadAutoCampaign,
                    recentCampaignId, campaignSelected, logInfo, logWarning,
                    preferredCampaignId);
            }
            catch (Exception ex)
            {
                if (logWarning != null)
                    logWarning("游戏内 MOD 读档页建立失败：" + ex.Message);
                Remove();
                return false;
            }
        }

        private static bool BindOpenedPanel(
            Action<ModPackage, string> startCampaign,
            Action<ModPackage, string> loadCampaign,
            Action<ModPackage, string> loadAutoCampaign,
            string recentCampaignId,
            Action<string> campaignSelected,
            Action<string> logInfo,
            Action<string> logWarning,
            string preferredCampaignId)
        {
            _startCampaign = startCampaign;
            _loadCampaign = loadCampaign;
            _loadAutoCampaign = loadAutoCampaign;
            _campaignSelected = campaignSelected;
            _flow = new CampaignMenuFlow(Campaigns, recentCampaignId);
            if (!string.IsNullOrEmpty(recentCampaignId) && _flow.RecentPackage == null
                && _campaignSelected != null)
                _campaignSelected("");
            _logInfo = logInfo;
            _logWarning = logWarning;
            _active = true;
            _panel.enabled = false;
            string initial = preferredCampaignId;
            if (!string.IsNullOrEmpty(initial) && _flow.Select(initial))
            {
                if (_campaignSelected != null) _campaignSelected(initial);
                RenderSelectedCampaign();
            }
            else
                RenderCampaignList();
            if (_logInfo != null)
                _logInfo("已用原版 LoadGamePanel 打开 MOD 战役存档页。");
            return true;
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

        private static void RenderCampaignList()
        {
            if (!_active) return;
            HideCustomAutoSlots();
            if (Campaigns.Count == 0)
            {
                RenderNoCampaignState();
                return;
            }
            int row = 0;
            for (int i = 0; i < Campaigns.Count && row < _slots.Length; i++, row++)
            {
                ModPackage package = Campaigns[i];
                ModPackage captured = package;
                string detail = ModDisclosurePolicy.SafePackageDescription(package);
                if (string.IsNullOrEmpty(detail))
                    detail = I18n.T("campaign.author", ModDisclosurePolicy.SafePackageAuthor(package));
                ConfigureSlot(
                    _slots[row],
                    I18n.T("campaign.option", row + 1),
                    ModDisclosurePolicy.SafePackageName(package),
                    detail,
                    delegate { SelectCampaign(captured); });
            }
            HideRemaining(row);
            ConfigureRecentEntry();
            SelectFirstSlot();
        }

        private static void RenderNoCampaignState()
        {
            if (_slots == null || _slots.Length == 0) return;
            ConfigureSlot(
                _slots[0],
                I18n.T("close"),
                I18n.T("campaign.none"),
                I18n.T("empty"),
                delegate { SelectAndClose(delegate { }); });
            HideRemaining(1);
            ConfigureRecentEntry();
            SelectFirstSlot();
        }

        private static void SelectCampaign(ModPackage package)
        {
            if (!_active || package == null || _flow == null
                || !_flow.Select(package.CampaignId)) return;
            if (_campaignSelected != null) _campaignSelected(package.CampaignId);
            RenderSelectedCampaign();
        }

        private static void RenderSelectedCampaign()
        {
            if (!_active || _flow == null || _flow.SelectedPackage == null) return;
            ModPackage package = _flow.SelectedPackage;
            SaveSystem saveSystem = SaveSystem.Instance;
            ModPackage captured = package;
            for (int i = 0; i < _slots.Length; i++)
            {
                int index = i + 1;
                string manualSlot = ModSaveSlotPolicy.IsolatedManualSlot(package.CampaignId, index);
                GameSave save = null;
                try
                {
                    save = saveSystem != null ? saveSystem.GetSaveData(manualSlot) : null;
                }
                catch (Exception ex)
                {
                    if (_logWarning != null)
                        _logWarning("读取 MOD 手动槽失败（" + manualSlot + "）:" + ex.Message);
                }
                Action click = null;
                if (save != null)
                {
                    Action<ModPackage, string> load = _loadCampaign;
                    string capturedSlot = manualSlot;
                    click = delegate
                    {
                        SelectAndClose(delegate
                        {
                            if (load == null)
                                throw new InvalidOperationException("MOD 战役读档回调已经失效");
                            load(captured, capturedSlot);
                        });
                    };
                }
                else if (_openedFromTitle)
                {
                    Action<ModPackage, string> start = _startCampaign;
                    string capturedSlot = manualSlot;
                    click = delegate
                    {
                        SelectAndClose(delegate
                        {
                            if (start == null)
                                throw new InvalidOperationException("MOD 新战役回调已经失效");
                            start(captured, capturedSlot);
                        });
                    };
                }
                ConfigureSlot(
                    _slots[i], ModSaveSlotPolicy.OfficialManualLabel(index),
                    save == null && _openedFromTitle ? I18n.T("campaign.start")
                        : save == null ? OfficialNoDataText()
                        : ModDisclosurePolicy.SafePackageName(package),
                    save == null ? "" : OfficialSaveTimeText(save),
                    click,
                    save != null,
                    save != null ? (Action)delegate
                    {
                        DeleteManualCampaignSave(captured, manualSlot);
                    } : null);
            }
            RenderOfficialAutoSlots(package, saveSystem);
            ConfigureRecentEntry();
            SelectFirstSlot();
        }

        private static void RenderOfficialAutoSlots(ModPackage package, SaveSystem saveSystem)
        {
            if (_autoSlots == null || _autoSlots.Length == 0)
            {
                if (_logWarning != null)
                    _logWarning("原版 LoadGamePanel._autoSaveSlot 缺少 auto/auto_free/auto_battle，无法显示隔离自动槽");
                return;
            }
            bool sawFree = false;
            for (int i = 0; i < _autoSlots.Length; i++)
            {
                AutoSaveSlotPanel slot = _autoSlots[i];
                string kind = OfficialAutoKind(slot);
                if (kind == null) continue;
                if (kind == "auto_free") sawFree = true;
                RevealUnderPanel(slot.gameObject);
                AutoGameSave auto = null;
                try
                {
                    string isolated = ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(
                        package.CampaignId, kind);
                    auto = saveSystem != null ? saveSystem.GetAutoSaveData(isolated) : null;
                }
                catch (Exception ex)
                {
                    if (_logWarning != null)
                        _logWarning("读取 MOD 隔离自动槽失败（" + kind + "）：" + ex.Message);
                }
                GameSave data = auto != null ? auto.GameSave : null;
                ConfigureAutoSlot(slot, package, kind, data);
            }
            if (!sawFree && _logWarning != null)
                _logWarning("当前 LoadGamePanel 没有 _slot=auto_free 的 AutoSaveSlotPanel，自由自动栏无法显示");
        }

        private static void ConfigureAutoSlot(
            AutoSaveSlotPanel slot, ModPackage package, string kind, GameSave save)
        {
            if (slot == null) return;
            RevealUnderPanel(slot.gameObject);
            SetTextObject(AutoSlotLabelField != null ? AutoSlotLabelField.GetValue(slot) : null,
                I18n.T("campaign.auto." + kind));
            if (save != null)
            {
                SetTextObject(AutoTitleField != null ? AutoTitleField.GetValue(slot) : null,
                    ModDisclosurePolicy.SafePackageName(package));
                SetTextObject(AutoTimeField != null ? AutoTimeField.GetValue(slot) : null,
                    OfficialSaveTimeText(save));
            }
            else
            {
                SetTextObject(AutoTitleField != null ? AutoTitleField.GetValue(slot) : null,
                    OfficialNoDataText());
                SetTextObject(AutoTimeField != null ? AutoTimeField.GetValue(slot) : null, "");
            }
            if (AutoCurrentDataField != null)
            {
                AutoGameSave current = null;
                if (save != null)
                {
                    current = new AutoGameSave();
                    current.Slot = CampaignIdentity.SaveSlot(package.CampaignId);
                    current.GameSave = save;
                }
                AutoCurrentDataField.SetValue(slot, current);
            }
            SetActive(AutoDeleteField != null ? AutoDeleteField.GetValue(slot) as Component : null, false);
            SetActive(AutoPlusIconField != null ? AutoPlusIconField.GetValue(slot) as GameObject : null, false);
            MenuToggleButton toggle = AutoMenuToggleField != null
                ? AutoMenuToggleField.GetValue(slot) as MenuToggleButton : null;
            if (toggle != null) toggle.SetActive(save != null);
            Button button = slot.SlotButton;
            if (button == null) throw new InvalidOperationException("原版 AutoSaveSlotPanel 缺少 SlotButton");
            Action<ModPackage, string> load = _loadAutoCampaign;
            button.onClick = new Button.ButtonClickedEvent();
            if (save != null)
            {
                button.onClick.AddListener(delegate
                {
                    TryLoadIsolatedAuto(package, kind);
                });
                button.interactable = true;
            }
            else
            {
                button.interactable = false;
            }
        }

        internal static bool TryHandleAutoClick(AutoSaveSlotPanel slot)
        {
            if (!_active || slot == null || _flow == null || _flow.SelectedPackage == null)
                return false;
            string kind = OfficialAutoKind(slot);
            if (kind == null) return true;
            TryLoadIsolatedAuto(_flow.SelectedPackage, kind);
            return true;
        }

        internal static bool TryHandleAutoSetup(AutoSaveSlotPanel slot)
        {
            if (!_active || slot == null || _flow == null || _flow.SelectedPackage == null)
                return false;
            string kind = OfficialAutoKind(slot);
            if (kind == null) return true;
            SaveSystem saveSystem = SaveSystem.Instance;
            AutoGameSave auto = null;
            try
            {
                string isolated = ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(
                    _flow.SelectedPackage.CampaignId, kind);
                auto = saveSystem != null ? saveSystem.GetAutoSaveData(isolated) : null;
            }
            catch (Exception ex)
            {
                if (_logWarning != null)
                    _logWarning("官方 AutoSaveSlotPanel.Setup 改读隔离槽失败（" + kind + "）：" + ex.Message);
            }
            ConfigureAutoSlot(slot, _flow.SelectedPackage, kind, auto != null ? auto.GameSave : null);
            return true;
        }

        internal static bool TryHandleManualSlotClick(LoadSlotPanel slot)
        {
            if (!_active || slot == null || _slots == null || _slots.Length == 0)
                return false;
            bool known = false;
            for (int i = 0; i < _slots.Length; i++)
            {
                if (!ReferenceEquals(_slots[i], slot)) continue;
                known = true;
                break;
            }
            if (!known) return false;
            Action action;
            if (ManualSlotActions.TryGetValue(slot, out action) && action != null)
            {
                action();
            }
            // 已知的 MOD 槽即使暂时没有绑定动作，也必须阻止原版
            // LoadSlotPanel.OnTitleClick 把空槽写入 001～020。
            return true;
        }

        private static void TryLoadIsolatedAuto(ModPackage package, string kind)
        {
            if (package == null || !ModSaveSlotPolicy.IsOfficialAutoSlot(kind)) return;
            Action<ModPackage, string> load = _loadAutoCampaign;
            SelectAndClose(delegate
            {
                if (load == null) throw new InvalidOperationException("MOD 自动存档回调已经失效");
                load(package, kind);
            });
        }

        private static AutoSaveSlotPanel[] CollectOfficialAutoSlots(LoadGamePanel panel)
        {
            var byKind = new Dictionary<string, AutoSaveSlotPanel>(StringComparer.Ordinal);
            AbsorbAutoSlots(byKind, AutoSaveSlotsField != null
                ? AutoSaveSlotsField.GetValue(panel) as AutoSaveSlotPanel[] : null);
            if (panel != null)
                AbsorbAutoSlots(byKind, panel.GetComponentsInChildren<AutoSaveSlotPanel>(true));
            string[] kinds = { "auto", "auto_free", "auto_battle" };
            var list = new List<AutoSaveSlotPanel>();
            for (int i = 0; i < kinds.Length; i++)
            {
                AutoSaveSlotPanel slot;
                if (byKind.TryGetValue(kinds[i], out slot)) list.Add(slot);
            }
            return list.ToArray();
        }

        private static void AbsorbAutoSlots(
            Dictionary<string, AutoSaveSlotPanel> byKind, AutoSaveSlotPanel[] slots)
        {
            if (slots == null) return;
            for (int i = 0; i < slots.Length; i++)
            {
                string kind = OfficialAutoKind(slots[i]);
                if (kind != null && !byKind.ContainsKey(kind))
                    byKind.Add(kind, slots[i]);
            }
        }

        private static string OfficialAutoKind(AutoSaveSlotPanel slot)
        {
            if (slot == null || AutoSlotKindField == null) return null;
            string kind = AutoSlotKindField.GetValue(slot) as string;
            return ModSaveSlotPolicy.IsOfficialAutoSlot(kind) ? kind : null;
        }

        private static void RevealUnderPanel(GameObject obj)
        {
            if (obj == null) return;
            Transform current = obj.transform;
            while (current != null)
            {
                current.gameObject.SetActive(true);
                if (current.GetComponent<LoadGamePanel>() != null) break;
                current = current.parent;
            }
        }

        private static string OfficialNoDataText()
        {
            try
            {
                if (LocalizationManager.Instance != null
                    && LocalizationManager.Instance.LocaleResolver != null)
                {
                    string text = LocalizationManager.Instance.LocaleResolver.GetString("System/NoData");
                    if (!string.IsNullOrEmpty(text)) return text;
                }
            }
            catch { }
            return I18n.T("empty");
        }

        private static string OfficialSaveSlotText(string slot)
        {
            try
            {
                if (LocalizationManager.Instance != null
                    && LocalizationManager.Instance.LocaleResolver != null)
                {
                    string format = LocalizationManager.Instance.LocaleResolver.GetString("System/SaveSlotText");
                    if (!string.IsNullOrEmpty(format)) return string.Format(format, slot);
                }
            }
            catch { }
            return slot ?? "";
        }

        private static string OfficialGameTimeText(GameSave data)
        {
            if (data == null) return OfficialNoDataText();
            try
            {
                GameTime gameTime = new GameTime(data.CurrentYear, data.CurrentMonth, data.CurrentStage);
                var resolver = LocalizationManager.Instance.LocaleResolver;
                return string.Format(
                    resolver.GetString("System/GameTimeText"),
                    resolver.GetString(gameTime.YearText),
                    resolver.GetString(gameTime.MonthText),
                    resolver.GetString(gameTime.StageText));
            }
            catch
            {
                return OfficialSaveTimeText(data);
            }
        }

        private static string OfficialSaveTimeText(GameSave data)
        {
            if (data == null) return "";
            try { return new DateTime(data.TimeTick).ToString("yyyy/MM/dd HH:mm:ss"); }
            catch { return ""; }
        }

        private static void ConfigureRecentEntry()
        {
            if (_recentSlot == null) return;
            if (_flow != null && _flow.SelectedPackage != null)
            {
                _recentSlot.gameObject.SetActive(true);
                SetTextObject(RecentSlotLabelField != null ? RecentSlotLabelField.GetValue(_recentSlot) : null,
                    I18n.T("campaign.back"));
                SetTextObject(RecentTitleField != null ? RecentTitleField.GetValue(_recentSlot) : null,
                    I18n.T("campaign.back_to_list"));
                SetTextObject(RecentTimeField != null ? RecentTimeField.GetValue(_recentSlot) : null, "");
                Button back = RecentButtonField != null
                    ? RecentButtonField.GetValue(_recentSlot) as Button : null;
                if (back == null) return;
                back.onClick = new Button.ButtonClickedEvent();
                back.onClick.AddListener(delegate { _flow.Back(); RenderCampaignList(); });
                back.interactable = true;
                return;
            }
            ModPackage recent = _flow != null ? _flow.RecentPackage : null;
            _recentSlot.gameObject.SetActive(recent != null);
            if (recent == null) return;
            SetTextObject(RecentSlotLabelField != null ? RecentSlotLabelField.GetValue(_recentSlot) : null,
                I18n.T("campaign.recent"));
            SetTextObject(RecentTitleField != null ? RecentTitleField.GetValue(_recentSlot) : null,
                ModDisclosurePolicy.SafePackageName(recent));
            SetTextObject(RecentTimeField != null ? RecentTimeField.GetValue(_recentSlot) : null,
                recent.CampaignId);
            Button button = RecentButtonField != null
                ? RecentButtonField.GetValue(_recentSlot) as Button : null;
            if (button == null) return;
            ModPackage captured = recent;
            button.onClick = new Button.ButtonClickedEvent();
            button.onClick.AddListener(delegate { SelectCampaign(captured); });
            button.interactable = true;
        }

        private static void HideCustomAutoSlots()
        {
            if (_autoSlots == null) return;
            for (int i = 0; i < _autoSlots.Length; i++)
                if (_autoSlots[i] != null) _autoSlots[i].gameObject.SetActive(false);
        }

        private static void ConfigureSlot(
            LoadSlotPanel slot,
            string slotLabel,
            string title,
            string detail,
            Action click,
            bool allowDelete = false,
            Action delete = null)
        {
            if (slot == null) return;
            slot.gameObject.SetActive(true);
            SetText(SlotLabelField != null ? SlotLabelField.GetValue(slot) as Text : null, slotLabel);
            SetTexts(TitleLabelsField != null ? TitleLabelsField.GetValue(slot) as Text[] : null, title);
            SetTexts(TimeLabelsField != null ? TimeLabelsField.GetValue(slot) as Text[] : null, detail);
            Button deleteButton = slot.DeleteButton;
            SetActive(deleteButton, allowDelete && delete != null);
            if (deleteButton != null)
            {
                deleteButton.onClick = new Button.ButtonClickedEvent();
                if (allowDelete && delete != null)
                    deleteButton.onClick.AddListener(delegate { delete(); });
            }
            SetActive(FocusObjectField != null ? FocusObjectField.GetValue(slot) as GameObject : null, false);
            SetActive(PlusIconField != null ? PlusIconField.GetValue(slot) as GameObject : null, false);
            Button button = slot.SlotButton;
            if (button == null)
                throw new InvalidOperationException("原版 LoadSlotPanel 缺少 SlotButton");
            if (click != null) ManualSlotActions[slot] = click;
            else ManualSlotActions.Remove(slot);
            button.onClick = new Button.ButtonClickedEvent();
            if (click != null) button.onClick.AddListener(delegate { click(); });
            button.interactable = click != null;
            button.navigation = new Navigation { mode = Navigation.Mode.Automatic };
        }

        private static void DeleteManualCampaignSave(ModPackage package, string isolatedSlot)
        {
            if (package == null || SaveSystem.Instance == null
                || string.IsNullOrEmpty(isolatedSlot)) return;
            try
            {
                SaveSystem.Instance.DeleteSaveData(isolatedSlot);
            }
            catch (Exception ex)
            {
                if (_logWarning != null)
                    _logWarning("删除 MOD 手动槽失败：" + ex.Message);
            }
            RenderSelectedCampaign();
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
            _openedFromTitle = false;
            _panel = null;
            _commonPanel = null;
            _slots = null;
            _autoSlots = null;
            _recentSlot = null;
            _autoVisibility = null;
            _startCampaign = null;
            _loadCampaign = null;
            _loadAutoCampaign = null;
            _campaignSelected = null;
            _flow = null;
            _logInfo = null;
            _logWarning = null;
            ManualSlotActions.Clear();
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

        private static void SetTextObject(object labels, string value)
        {
            Text one = labels as Text;
            if (one != null) { SetText(one, value); return; }
            SetTexts(labels as Text[], value);
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

    /// <summary>
    /// 原版 AutoSaveSlotPanel.OnTitleClick 读的是预制体 _slot（auto /
    /// auto_free / auto_battle）。标题页 CurrentSlot 仍是原版槽时会进原版战役档。
    /// MOD 存档页期间改走隔离槽。
    /// </summary>
    [HarmonyPatch(typeof(AutoSaveSlotPanel), "OnTitleClick")]
    internal static class ModAutoSaveSlotClickPatch
    {
        private static bool Prefix(AutoSaveSlotPanel __instance)
        {
            return !VanillaModCampaignPanel.TryHandleAutoClick(__instance);
        }
    }

    [HarmonyPatch(typeof(AutoSaveSlotPanel), "Setup")]
    internal static class ModAutoSaveSlotSetupPatch
    {
        private static bool Prefix(AutoSaveSlotPanel __instance)
        {
            return !VanillaModCampaignPanel.TryHandleAutoSetup(__instance);
        }
    }

    /// <summary>
    /// 原版空 LoadSlotPanel.OnTitleClick 会 NewGameData 写进 001～020。
    /// MOD 页必须拦住，否则右侧空槽会开原版新周目。
    /// </summary>
    [HarmonyPatch(typeof(LoadSlotPanel), "OnTitleClick")]
    internal static class ModLoadSlotClickPatch
    {
        private static bool Prefix(LoadSlotPanel __instance)
        {
            return !VanillaModCampaignPanel.TryHandleManualSlotClick(__instance);
        }
    }

    /// <summary>
    /// 游戏内菜单「读取」打开的是 MenuPanel._loadPanel 里的原版 LoadGamePanel。
    /// MOD 战役中改为直接复用标题那套战役存档页，并按当前 campaign 显示存档。
    /// </summary>
    [HarmonyPatch(typeof(MenuPanel), "LoadButtonClick")]
    internal static class InGameLoadMenuPatch
    {
        private static void Postfix(MenuPanel __instance)
        {
            if (__instance == null || VanillaModCampaignPanel.IsActive) return;
            if (Plugin.Instance == null) return;
            Plugin.Instance.TryOpenInGameModLoadMenu(__instance);
        }
    }

    /// <summary>
    /// 战役暂停页「读取」走的是 Mortal.Battle.PausePanel.LoadButtonClick，
    /// 只 Show(_loadPanel)，不会经过 MenuPanel。同样接管为当前 MOD 战役存档页。
    /// </summary>
    [HarmonyPatch(typeof(Mortal.Battle.PausePanel), "LoadButtonClick")]
    internal static class BattlePauseLoadMenuPatch
    {
        private static void Postfix(Mortal.Battle.PausePanel __instance)
        {
            if (__instance == null || VanillaModCampaignPanel.IsActive) return;
            if (Plugin.Instance == null) return;
            Plugin.Instance.TryOpenInGameModLoadMenu(__instance);
        }
    }
}
