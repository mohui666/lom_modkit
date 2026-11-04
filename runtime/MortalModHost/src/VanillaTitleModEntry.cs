using System;
using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using Lean.Localization;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>One Start Game entry, then an explicit choice of the three experiences.</summary>
    internal static class VanillaTitleModEntry
    {
        private static readonly FieldInfo StartButtonField = AccessTools.Field(typeof(TitleManager), "_startButton");
        private static readonly FieldInfo ButtonsField = AccessTools.Field(typeof(TitleManager), "_buttons");
        private static TitleManager _owner;
        private static Button _original, _button;
        private static GameObject _root;
        private static bool _originalActive, _missingTemplateLogged;
        internal static bool IsVisible { get { return _root != null && _root.activeInHierarchy; } }

        internal static void Maintain(bool shouldExist, Action openModes,
            Action<string> logInfo, Action<string> logWarning)
        {
            if (!shouldExist) { Remove(); return; }
            TitleManager owner = TitleManager.Instance;
            if (owner == null || (_root != null && _owner == owner)) return;
            Remove();
            try
            {
                Button original = StartButtonField != null ? StartButtonField.GetValue(owner) as Button : null;
                if (original == null || original.transform.parent == null)
                    throw new InvalidOperationException("找不到原版开始游戏按钮");
                _owner = owner;
                _original = original;
                _originalActive = original.gameObject.activeSelf;
                _root = UnityEngine.Object.Instantiate(original.gameObject, original.transform.parent, false);
                _root.name = "MortalModHost_StartGame";
                _root.transform.SetSiblingIndex(original.transform.GetSiblingIndex());
                _button = _root.GetComponent<Button>();
                _button.onClick = new Button.ButtonClickedEvent();
                _button.onClick.AddListener(delegate { if (openModes != null) openModes(); });
                _button.navigation = new Navigation { mode = Navigation.Mode.Automatic };
                foreach (LeanLocalizedText localizer in _root.GetComponentsInChildren<LeanLocalizedText>(true))
                    localizer.enabled = false;
                foreach (Text label in _root.GetComponentsInChildren<Text>(true))
                {
                    label.supportRichText = false;
                    label.text = "开始游戏";
                }
                original.gameObject.SetActive(false);
                StartButtonField.SetValue(owner, _button);
                if (ButtonsField != null)
                {
                    var buttons = new List<Button>((Button[])ButtonsField.GetValue(owner) ?? new Button[0]);
                    buttons.Remove(original);
                    buttons.Add(_button);
                    ButtonsField.SetValue(owner, buttons.ToArray());
                }
                _missingTemplateLogged = false;
                if (logInfo != null) logInfo("开始游戏入口已接入：纯原版 / 原版＋MOD / 纯MOD");
            }
            catch (Exception ex)
            {
                Remove();
                if (!_missingTemplateLogged && logWarning != null)
                    logWarning("无法创建玩法选择入口：" + ex.Message);
                _missingTemplateLogged = true;
            }
        }

        internal static void OpenOriginalGame()
        {
            if (_original == null) throw new InvalidOperationException("原版开始游戏入口尚未就绪");
            if (Plugin.Instance != null) Plugin.Instance.EnterOriginalMode();
            // Keep the original serialized choice flow, including slot selection and confirmation.
            _original.onClick.Invoke();
        }

        internal static void Remove()
        {
            if (_owner != null)
            {
                if (_original != null)
                {
                    _original.gameObject.SetActive(_originalActive);
                    if (StartButtonField != null && ReferenceEquals(StartButtonField.GetValue(_owner), _button))
                        StartButtonField.SetValue(_owner, _original);
                }
                if (ButtonsField != null)
                {
                    var kept = new List<Button>();
                    foreach (Button current in (Button[])ButtonsField.GetValue(_owner) ?? new Button[0])
                        if (current != null && current != _button) kept.Add(current);
                    if (_original != null && !kept.Contains(_original)) kept.Insert(0, _original);
                    ButtonsField.SetValue(_owner, kept.ToArray());
                }
            }
            if (_root != null) UnityEngine.Object.Destroy(_root);
            _owner = null;
            _original = _button = null;
            _root = null;
        }
    }
}
