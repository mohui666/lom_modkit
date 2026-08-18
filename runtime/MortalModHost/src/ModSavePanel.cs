using System;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// MOD 战役的保存页完整复用官方 SaveGamePanel 的 001~020 栏位。每个可见栏位
    /// 都只改写其实际存档名到当前 campaign 的隔离命名空间；原版的空槽新建、已有
    /// 槽确认覆盖和确认框交互保持不变。
    /// </summary>
    internal static class ModSavePanel
    {
        private static readonly FieldInfo SaveSlotsField =
            AccessTools.Field(typeof(SaveGamePanel), "_saveSlots");
        private static readonly FieldInfo SlotField =
            AccessTools.Field(typeof(SaveSlotPanel), "_slot");
        private static readonly FieldInfo SlotTextField =
            AccessTools.Field(typeof(SaveSlotPanel), "_slotText");
        private static readonly FieldInfo TitleTextField =
            AccessTools.Field(typeof(SaveSlotPanel), "_titleText");
        private static readonly FieldInfo TimeTextField =
            AccessTools.Field(typeof(SaveSlotPanel), "_timeText");
        private static readonly FieldInfo FocusObjectField =
            AccessTools.Field(typeof(SaveSlotPanel), "_focusObj");
        private static readonly FieldInfo NewGamePlusIconField =
            AccessTools.Field(typeof(SaveSlotPanel), "_newGamePlusIcon");
        private static readonly FieldInfo CurrentDataField =
            AccessTools.Field(typeof(SaveSlotPanel), "_currentData");

        private static SaveGamePanel _panel;
        private static SaveSlotPanel[] _managedSlots;
        private static ModPackage _package;
        private static bool _active;

        internal static bool IsActive { get { return _active; } }

        internal static bool TryOpen(SaveGamePanel panel)
        {
            RestoreOriginalSlots();
            if (panel == null) return false;
            ModPackage package = CurrentCampaignPackage();
            if (package == null) return false;

            SaveSlotPanel[] slots = SaveSlotsField != null
                ? SaveSlotsField.GetValue(panel) as SaveSlotPanel[] : null;
            if (slots == null || slots.Length == 0 || slots[0] == null
                || SlotField == null || CurrentDataField == null)
            {
                PersistentModState.Log?.LogWarning(
                    "官方 SaveGamePanel 字段不完整，保留原版保存页以避免误写存档");
                return false;
            }

            _panel = panel;
            _managedSlots = slots;
            _package = package;
            _active = true;
            for (int i = 0; i < slots.Length; i++)
                ConfigureManagedSlot(slots[i], i + 1);
            if (slots[0].SlotButton != null) slots[0].SlotButton.Select();
            PersistentModState.Log?.LogInfo(
                "已打开当前 MOD 的 20 个独立保存栏位：" + package.CampaignId);
            return true;
        }

        internal static bool TryConfigureSlot(SaveSlotPanel slot)
        {
            if (!_active || slot == null || _managedSlots == null)
                return false;
            for (int i = 0; i < _managedSlots.Length; i++)
            {
                if (!ReferenceEquals(slot, _managedSlots[i])) continue;
                ConfigureManagedSlot(slot, i + 1);
                return true;
            }
            return false;
        }

        internal static void RestoreOriginalSlots()
        {
            SaveGamePanel panel = _panel;
            _active = false;
            _managedSlots = null;
            _package = null;
            _panel = null;
            if (panel == null || SaveSlotsField == null) return;
            try
            {
                SaveSlotPanel[] slots = SaveSlotsField.GetValue(panel) as SaveSlotPanel[];
                if (slots == null) return;
                for (int i = 0; i < slots.Length; i++)
                {
                    SaveSlotPanel slot = slots[i];
                    if (slot == null) continue;
                    slot.gameObject.SetActive(true);
                    slot.SetSlot(i + 1);
                    slot.Setup();
                }
            }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogWarning("恢复原版保存槽失败：" + ex.Message);
            }
        }

        private static ModPackage CurrentCampaignPackage()
        {
            SaveSystem saves = SaveSystem.Instance;
            if (saves == null || !ModCampaignState.Active
                || !CampaignIdentity.OwnsSlot(ModCampaignState.ActiveCampaignId, saves.CurrentSlot))
                return null;
            ModPackage package = Plugin.FindCampaignPackage(ModCampaignState.ActiveCampaignId);
            if (package == null || !string.Equals(package.Id, ModCampaignState.ActiveModId,
                StringComparison.Ordinal))
                return null;
            return package;
        }

        private static void ConfigureManagedSlot(SaveSlotPanel slot, int index)
        {
            if (!_active || slot == null || _package == null) return;
            try
            {
                SaveSystem saves = SaveSystem.Instance;
                if (saves == null) throw new InvalidOperationException("SaveSystem 尚未就绪");
                string isolatedSlot = ModSaveSlotPolicy.IsolatedManualSlot(
                    _package.CampaignId, index);
                GameSave data = saves.GetSaveData(isolatedSlot);
                SlotField.SetValue(slot, isolatedSlot);
                CurrentDataField.SetValue(slot, data);
                SetText(SlotTextField != null ? SlotTextField.GetValue(slot) as Text : null,
                    OfficialSaveSlotText(ModSaveSlotPolicy.OfficialManualLabel(index)));
                string title = data == null ? OfficialNoDataText()
                    : "MOD · " + ModDisclosurePolicy.SafePackageName(_package);
                SetTexts(TitleTextField != null ? TitleTextField.GetValue(slot) as Text[] : null,
                    title);
                SetTexts(TimeTextField != null ? TimeTextField.GetValue(slot) as Text[] : null,
                    data == null ? "" : OfficialSaveTimeText(data));
                SetActive(FocusObjectField != null
                    ? FocusObjectField.GetValue(slot) as GameObject : null,
                    string.Equals(saves.CurrentSlot, isolatedSlot, StringComparison.Ordinal));
                SetActive(NewGamePlusIconField != null
                    ? NewGamePlusIconField.GetValue(slot) as GameObject : null,
                    data != null && data.NewGamePlus);
                slot.gameObject.SetActive(true);
            }
            catch (Exception ex)
            {
                PersistentModState.Log?.LogError("配置 MOD 独立保存槽失败：" + ex);
                RestoreOriginalSlots();
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
            return "No Data";
        }

        private static string OfficialSaveSlotText(string slot)
        {
            try
            {
                if (LocalizationManager.Instance != null
                    && LocalizationManager.Instance.LocaleResolver != null)
                {
                    string format = LocalizationManager.Instance.LocaleResolver.GetString(
                        "System/SaveSlotText");
                    if (!string.IsNullOrEmpty(format)) return string.Format(format, slot);
                }
            }
            catch { }
            return slot ?? "";
        }

        private static string OfficialSaveTimeText(GameSave data)
        {
            try { return new DateTime(data.TimeTick).ToString("yyyy/MM/dd HH:mm:ss"); }
            catch { return ""; }
        }

        private static void SetText(Text text, string value)
        {
            if (text != null) text.text = value ?? "";
        }

        private static void SetTexts(Text[] texts, string value)
        {
            if (texts == null) return;
            for (int i = 0; i < texts.Length; i++) SetText(texts[i], value);
        }

        private static void SetActive(GameObject obj, bool active)
        {
            if (obj != null) obj.SetActive(active);
        }
    }

    [HarmonyPatch(typeof(SaveGamePanel), "OnPanelOpen")]
    internal static class ModSaveGamePanelOpenPatch
    {
        private static bool Prefix(SaveGamePanel __instance)
        {
            return !ModSavePanel.TryOpen(__instance);
        }
    }

    [HarmonyPatch(typeof(SaveSlotPanel), "Setup")]
    internal static class ModSaveSlotSetupPatch
    {
        private static bool Prefix(SaveSlotPanel __instance)
        {
            return !ModSavePanel.TryConfigureSlot(__instance);
        }
    }
}
