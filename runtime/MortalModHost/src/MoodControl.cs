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
    /// patch 目标说明：任务描述中的签名是 "public void ShowMood()，内部用自己 _originPortrait"——
    /// 该方法是 <c>StoryCharacterController.ShowMood()</c>；StoryStageController 的
    /// ShowMood(Character) 是 private 且带角色参数，最终也委托到前者。show/face/move 等节点的
    /// stage.show 显示路径（StoryStageController.Show → ShowMood(Character)）以及其他一切显示
    /// 路径都要经过 StoryCharacterController.ShowMood()，故 postfix 挂在这里即全量隐藏的唯一收口。
    /// postfix 拿不到对应角色引用，按约定直接全量隐藏（FindObjectsOfType 逐个 Mood?.Hide()）。
    /// </summary>
    [HarmonyPatch(typeof(StoryCharacterController), "ShowMood")]
    internal static class MoodControl
    {
        /// <summary>true = mod 演出中禁止官方心情气泡显示（mod_set_mood(false) 或缺参）。</summary>
        internal static bool Disabled;

        /// <summary>日志通道，由 Plugin.Awake 注入（patch 类是静态的，拿不到插件实例 Logger）。</summary>
        internal static ManualLogSource Log;

        /// <summary>HideAllMoodPanels 是否已打过一次诊断日志（只记第一次，避免刷屏）。</summary>
        private static bool _hideLogged;

        /// <summary>角色 ShowMood 之后立刻收尾：Disabled 时把刚显示出的气泡全量隐藏。</summary>
        private static void Postfix()
        {
            if (Disabled)
                HideAllMoodPanels();
        }

        /// <summary>
        /// 隐藏全部角色的圆形情绪面板（ShowMood postfix 与 Lua 侧 mod_hide_mood 回调共用）。
        /// 角色尚未 SetupMood 时 Mood 为 null，跳过；失败只 LogWarning 不中断演出。
        /// </summary>
        internal static void HideAllMoodPanels()
        {
            try
            {
                StoryCharacterController[] controllers = UnityEngine.Object.FindObjectsOfType<StoryCharacterController>();
                if (!_hideLogged)
                {
                    _hideLogged = true;
                    if (Log != null)
                        Log.LogInfo("mod 心情气泡隐藏：找到 " + controllers.Length + " 个角色");
                }
                for (int i = 0; i < controllers.Length; i++)
                {
                    CharacterMoodPanel mood = controllers[i].Mood;
                    if (mood != null)
                        mood.Hide();
                }
            }
            catch (Exception ex)
            {
                if (Log != null)
                    Log.LogWarning("mod 心情气泡隐藏失败：" + ex);
            }
        }
    }
}
