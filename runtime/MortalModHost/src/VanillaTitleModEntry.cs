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
    /// <summary>
    /// 在标题画面的原版按钮列中克隆“开始游戏”按钮，作为 MOD 战役入口。
    /// 只复用原版序列化的视觉、悬停动画、音效与导航；点击事件由 Host 重新建立，
    /// 绝不保留模板按钮里指向 TitleManager.NewGame 的持久 UnityEvent。
    /// </summary>
    internal static class VanillaTitleModEntry
    {
        private const string ObjectName = "MortalModHost_StartModCampaign";

        private static readonly FieldInfo StartButtonField =
            AccessTools.Field(typeof(TitleManager), "_startButton");
        private static readonly FieldInfo ButtonsField =
            AccessTools.Field(typeof(TitleManager), "_buttons");

        private static TitleManager _owner;
        private static GameObject _root;
        private static Button _button;
        private static bool _missingTemplateLogged;

        internal static bool IsVisible
        {
            get { return _root != null && _root.activeInHierarchy; }
        }

        internal static void Maintain(
            bool shouldExist,
            Action openMenu,
            Action<string> logInfo,
            Action<string> logWarning)
        {
            if (!shouldExist)
            {
                Remove();
                return;
            }

            TitleManager owner = TitleManager.Instance;
            if (owner == null)
                return;

            if (_root != null && _owner == owner)
            {
                ApplyLabel(_root);
                return;
            }

            Remove();
            GameObject clone = null;
            try
            {
                Button template = FindStartButton(owner);
                if (template == null || template.transform.parent == null)
                {
                    if (!_missingTemplateLogged && logWarning != null)
                    {
                        _missingTemplateLogged = true;
                        logWarning("标题画面已加载，但找不到原版开始游戏按钮；本次不提供非官方战役 UI 回退。");
                    }
                    return;
                }

                clone = UnityEngine.Object.Instantiate(
                    template.gameObject, template.transform.parent, false);
                clone.name = ObjectName;

                // 克隆对象默认排在末尾；放到模板当前序号会把原版“开始游戏”向后推一位。
                clone.transform.SetSiblingIndex(template.transform.GetSiblingIndex());

                Button button = clone.GetComponent<Button>();
                if (button == null)
                    throw new InvalidOperationException("原版开始游戏模板缺少 UnityEngine.UI.Button");

                // RemoveAllListeners 不保证移除模板里序列化的持久事件。直接替换整个事件对象，
                // 防止点击 MOD 入口时同时执行原版 NewGame。
                button.onClick = new Button.ButtonClickedEvent();
                button.onClick.AddListener(delegate
                {
                    if (openMenu != null)
                        openMenu();
                });
                button.navigation = new Navigation { mode = Navigation.Mode.Automatic };

                // 原按钮的文本本地化组件会在语言刷新时把自定义标签改回“开始游戏”。
                // 禁用文字翻译组件，但保留 LeanLocalizedTextFont 以继续使用原版多语言字体。
                LeanLocalizedText[] localizers = clone.GetComponentsInChildren<LeanLocalizedText>(true);
                for (int i = 0; i < localizers.Length; i++)
                    localizers[i].enabled = false;

                ApplyLabel(clone);
                _owner = owner;
                _root = clone;
                _button = button;
                AppendManagedButton(owner, button);
                _missingTemplateLogged = false;

                if (logInfo != null)
                    logInfo("已在原版开始游戏按钮上方注入“" + I18n.T("title.mod_campaign") + "”入口。");
            }
            catch (Exception ex)
            {
                if (_root == null && clone != null)
                    UnityEngine.Object.Destroy(clone);
                Remove();
                if (!_missingTemplateLogged && logWarning != null)
                {
                    _missingTemplateLogged = true;
                    logWarning("原版标题按钮注入失败；本次不提供非官方战役 UI 回退：" + ex.Message);
                }
            }
        }

        internal static void Remove()
        {
            if (_owner != null && _button != null && ButtonsField != null)
            {
                try
                {
                    Button[] buttons = ButtonsField.GetValue(_owner) as Button[];
                    if (buttons != null)
                    {
                        var kept = new List<Button>(buttons.Length);
                        for (int i = 0; i < buttons.Length; i++)
                        {
                            if (buttons[i] != null && buttons[i] != _button)
                                kept.Add(buttons[i]);
                        }
                        ButtonsField.SetValue(_owner, kept.ToArray());
                    }
                }
                catch
                {
                    // 场景卸载期间对象可能已经销毁；标题管理器本身也会随场景释放。
                }
            }

            if (_root != null)
                UnityEngine.Object.Destroy(_root);
            _owner = null;
            _root = null;
            _button = null;
        }

        private static Button FindStartButton(TitleManager owner)
        {
            if (StartButtonField != null)
            {
                try
                {
                    Button value = StartButtonField.GetValue(owner) as Button;
                    if (value != null)
                        return value;
                }
                catch
                {
                    // 私有字段布局变化时继续走场景对象名兼容路径。
                }
            }

            Button[] candidates = owner.GetComponentsInChildren<Button>(true);
            for (int i = 0; i < candidates.Length; i++)
            {
                if (candidates[i] != null && candidates[i].gameObject.name == "StartGame")
                    return candidates[i];
            }
            return null;
        }

        private static void AppendManagedButton(TitleManager owner, Button button)
        {
            if (ButtonsField == null)
                return;
            Button[] current = ButtonsField.GetValue(owner) as Button[];
            if (current == null)
                current = new Button[0];
            var next = new Button[current.Length + 1];
            Array.Copy(current, next, current.Length);
            next[current.Length] = button;
            ButtonsField.SetValue(owner, next);
        }

        private static void ApplyLabel(GameObject root)
        {
            if (root == null)
                return;
            string label = I18n.T("title.mod_campaign");
            Text[] labels = root.GetComponentsInChildren<Text>(true);
            for (int i = 0; i < labels.Length; i++)
            {
                labels[i].supportRichText = false;
                if (labels[i].text != label)
                    labels[i].text = label;
            }
        }
    }
}
