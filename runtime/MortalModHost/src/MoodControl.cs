using System;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 心情气泡硬控（契约 §B）：编译器在每个 mod 脚本开头发射裸全局调用
    /// mod_set_mood(false/true)（story 顶层 mood 字段，默认 false），LuaManagerPatch 注册的
    /// MoonSharp 全局函数把状态写进 <see cref="Disabled"/>；官方脚本演出前会复位为 false。
    /// Disabled=true 时，任何角色 ShowMood 显示出的圆形情绪面板立即被全量隐藏。
    ///
    /// patch 目标核实（2026-08 ilspycmd 反编译 Mortal.Story.dll，v 与游戏 7 月更新一致）：
    /// <list type="bullet">
    /// <item><c>Mortal.Story.StoryCharacterController</c>：<c>public void ShowMood()</c>
    ///   实现为 <c>Mood.Show(_originPortrait, State.holder.localScale.x)</c>；<c>public
    ///   CharacterMoodPanel Mood { get; private set; }</c>（SetupMood 时 GetComponent 挂到自身
    ///   GameObject 上，无其他赋值点）。类名/签名与 patch 目标完全一致，无需修正。</item>
    /// <item><c>Mortal.Story.CharacterMoodPanel : MonoBehaviour</c>：<c>public void
    ///   Show(string, float)</c> / <c>public void Hide()</c>，Hide 遍历 <c>_moods</c> 逐个
    ///   Left/Right SetActive(false)。</item>
    /// <item>全部显示路径收口已确认：StoryStageController.Show/Hide/SetDimmed 里的
    ///   ShowMood(Character)/HideMood(Character) 均为 private，最终委托到
    ///   StoryCharacterController.ShowMood()/Mood.Hide()；全游戏仅 Mortal.Story.dll
    ///   出现 ShowMood/SetupMood/HideMood 字符串。postfix 挂 ShowMood 即全量隐藏的唯一收口。</item>
    /// </list>
    ///
    /// 隐藏实现（双路径，每次全跑）：
    /// <list type="number">
    /// <item>直接找面板：Resources.FindObjectsOfTypeAll&lt;CharacterMoodPanel&gt;() 逐个 Hide()。
    ///   用 FindObjectsOfTypeAll 而非 FindObjectsOfType，因为后者只返回激活对象——实测诊断
    ///   首次调用"找到 0 个角色"即由此引起（对象处于未激活状态时两者结果天差地别）。</item>
    /// <item>控制器双保险：FindObjectsOfTypeAll&lt;StoryCharacterController&gt;() 逐个
    ///   Mood?.Hide()，保留原引用链路径以防面板路径有遗漏。</item>
    /// </list>
    ///
    /// 诊断：HideAllMoodPanels 前 5 次、ShowMood postfix 前 3 次打 [mood-diag] 前缀
    /// LogInfo（面板数/控制器数/Disabled 状态），用于游戏内实测验证 patch 确实生效。
    /// </summary>
    [HarmonyPatch(typeof(StoryCharacterController), "ShowMood")]
    internal static class MoodControl
    {
        /// <summary>true = mod 演出中禁止官方心情气泡显示（mod_set_mood(false) 或缺参）。</summary>
        internal static bool Disabled;

        /// <summary>日志通道，由 Plugin.Awake 注入（patch 类是静态的，拿不到插件实例 Logger）。</summary>
        internal static ManualLogSource Log;

        /// <summary>HideAllMoodPanels 已调用次数（含 Lua 侧 mod_hide_mood 触发）。</summary>
        private static int _hideCalls;

        /// <summary>ShowMood postfix 已触发次数。</summary>
        private static int _postfixCalls;

        /// <summary>HideAllMoodPanels 打 Info 诊断日志的前 N 次调用。</summary>
        private const int HideLogLimit = 5;

        /// <summary>ShowMood postfix 打 Info 诊断日志的前 N 次触发。</summary>
        private const int PostfixLogLimit = 3;

        /// <summary>角色 ShowMood 之后立刻收尾：Disabled 时把刚显示出的气泡全量隐藏。</summary>
        private static void Postfix()
        {
            int n = ++_postfixCalls;
            if (n <= PostfixLogLimit && Log != null)
                Log.LogInfo("[mood-diag] StoryCharacterController.ShowMood postfix 触发 #" + n + "（Disabled=" + Disabled + "）");
            if (Disabled)
                HideAllMoodPanels();
        }

        /// <summary>
        /// 隐藏全部角色的圆形情绪面板（ShowMood postfix 与 Lua 侧 mod_hide_mood 回调共用）。
        /// 双路径全跑：先直接找 CharacterMoodPanel 逐个 Hide()（绕开控制器与 Mood 引用链），
        /// 再走控制器 Mood 引用链 Hide() 作双保险；失败只 LogWarning 不中断演出。
        /// </summary>
        internal static void HideAllMoodPanels()
        {
            int n = ++_hideCalls;
            try
            {
                // 路径 1：直接找面板对象（含未激活对象），逐个 Hide()
                CharacterMoodPanel[] panels = Resources.FindObjectsOfTypeAll<CharacterMoodPanel>();
                for (int i = 0; i < panels.Length; i++)
                {
                    if (panels[i] != null)
                        panels[i].Hide();
                }

                // 路径 2：控制器引用链双保险
                StoryCharacterController[] controllers = Resources.FindObjectsOfTypeAll<StoryCharacterController>();
                for (int i = 0; i < controllers.Length; i++)
                {
                    CharacterMoodPanel mood = controllers[i].Mood;
                    if (mood != null)
                        mood.Hide();
                }

                if (n <= HideLogLimit && Log != null)
                    Log.LogInfo("[mood-diag] HideAllMoodPanels #" + n + "：找到面板 " + panels.Length +
                                " 个，控制器 " + controllers.Length + " 个（Disabled=" + Disabled + "）");
            }
            catch (Exception ex)
            {
                if (Log != null)
                    Log.LogWarning("[mood-diag] 心情气泡隐藏失败：" + ex);
            }
        }
    }
}
