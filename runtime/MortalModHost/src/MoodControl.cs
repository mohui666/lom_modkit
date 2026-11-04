using System;
using System.Collections.Generic;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 心情气泡硬控（契约 §6.10）：编译器在每个 mod 脚本开头发射裸全局调用
    /// mod_set_mood(false/true)（story 顶层 mood 字段，默认 false），LuaManagerPatch 注册的
    /// MoonSharp 全局函数把状态写进 <see cref="Disabled"/>；官方脚本演出前会复位为 false。
    /// Disabled=true 时隐藏自动气泡；显式单人设置直接复用本场人物的官方面板。
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
        /// <summary>true = 禁止自动气泡；显式单人气泡不依赖人物表情。</summary>
        internal static bool Disabled;

        // Runtime choices only: never change a character's portrait or official assets.
        private sealed class Setting
        {
            internal string Icon;
            internal Vector2 Offset;
        }
        private static readonly Dictionary<StoryCharacterController, Setting> Overrides =
            new Dictionary<StoryCharacterController, Setting>();

        internal static void ClearOverrides()
        {
            foreach (var controller in Overrides.Keys) RestorePosition(controller);
            Overrides.Clear();
            CustomCharacterRuntime.ClearMoodOverrides();
        }

        internal static CharacterMoodPanel CreatePanel(Transform parent)
        {
            var stage = Fungus.Stage.GetActiveStage() as StoryStageController;
            var prefab = stage == null ? null : Traverse.Create(stage).Field("_moodPrefab").GetValue<GameObject>();
            if (prefab == null) throw new InvalidOperationException("当前舞台缺少原版气泡面板");
            var panel = UnityEngine.Object.Instantiate(prefab, parent, false).GetComponent<CharacterMoodPanel>();
            if (panel == null) throw new InvalidOperationException("原版气泡预制体缺少 CharacterMoodPanel");
            panel.Hide();
            return panel;
        }

        internal static void Forget(Fungus.Character character)
        {
            StoryCharacterController controller = character as StoryCharacterController;
            if (controller != null && Overrides.Remove(controller))
            {
                RestorePosition(controller);
            }
        }

        private static void RestorePosition(StoryCharacterController controller)
        {
            if (controller != null && controller.Mood != null && controller.Data != null)
                controller.Mood.SetPosition(controller.Data.MoodPosition.x, controller.Data.MoodPosition.y);
        }

        internal static void SetCharacter(string id, string icon, float x = 0f, float y = 0f)
        {
            if (string.IsNullOrEmpty(id)) throw new ArgumentException("指定气泡需要人物 ID");
            if (float.IsNaN(x) || float.IsNaN(y) || Math.Abs(x) > 1920f || Math.Abs(y) > 1080f)
                throw new ArgumentException("气泡偏移超出范围");
            if (Array.IndexOf(new[] { "auto", "hide", "shock", "silent", "angry2", "nervous1", "laugh1",
                "shy", "nervous3", "nervous4", "nervous2" }, icon) < 0)
                throw new ArgumentException("未知气泡样式：" + icon);
            if (id.StartsWith("user:", StringComparison.Ordinal))
            {
                CustomCharacterRuntime.SetMood(id, icon, x, y);
                return;
            }
            CharacterPlaceholder placeholder = CharacterPlaceholder.Instance;
            StoryCharacterController controller = placeholder != null ? placeholder.Get(id) as StoryCharacterController : null;
            if (controller == null || controller.State == null || !controller.State.onScreen
                || controller.State.holder == null || controller.Mood == null)
                throw new InvalidOperationException("指定气泡的人物尚未登场：" + id);
            if (icon == "auto" && x == 0f && y == 0f) Forget(controller);
            else Overrides[controller] = new Setting { Icon = icon, Offset = new Vector2(x, y) };
            if (controller.State.dimmed) controller.Mood.Hide();
            else controller.ShowMood();
        }

        internal static void RefreshCharacter(Fungus.Character character)
        {
            StoryCharacterController controller = character as StoryCharacterController;
            if (controller != null && Overrides.ContainsKey(controller) && controller.State != null && controller.State.onScreen
                && !controller.State.dimmed && controller.State.holder != null && controller.Mood != null)
                controller.ShowMood();
        }

        internal static void RefreshVisible()
        {
            Fungus.Stage stage = Fungus.Stage.GetActiveStage();
            if (stage == null) return;
            foreach (Fungus.Character character in stage.CharactersOnStage)
            {
                StoryCharacterController controller = character as StoryCharacterController;
                if (controller != null && controller.State.onScreen && !controller.State.dimmed
                    && controller.State.holder != null && controller.Mood != null)
                    controller.ShowMood();
            }
        }

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

        /// <summary>在原版自动匹配之后应用单人选择，否则执行自动开关。</summary>
        private static void Postfix(StoryCharacterController __instance)
        {
            int n = ++_postfixCalls;
            if (n <= PostfixLogLimit && Log != null)
                Log.LogInfo("[mood-diag] StoryCharacterController.ShowMood postfix 触发 #" + n + "（Disabled=" + Disabled + "）");
            if (__instance.Mood == null || __instance.State == null || __instance.State.holder == null) return;
            Setting setting;
            if (Overrides.TryGetValue(__instance, out setting))
            {
                Vector2 position = __instance.Data.MoodPosition + setting.Offset;
                __instance.Mood.SetPosition(position.x, position.y);
                if (setting.Icon == "hide" || !__instance.State.onScreen || __instance.State.dimmed || setting.Icon == "auto" && Disabled)
                    __instance.Mood.Hide();
                else if (setting.Icon != "auto")
                    __instance.Mood.Show(setting.Icon, __instance.State.holder.localScale.x);
            }
            else if (Disabled)
                __instance.Mood.Hide();
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

    [HarmonyPatch(typeof(StoryStageController), "Hide")]
    internal static class MoodCharacterExitPatch
    {
        private static void Postfix(Fungus.PortraitOptions options) { MoodControl.Forget(options.character); }
    }

    [HarmonyPatch(typeof(StoryStageController), "SetDimmed")]
    internal static class MoodCharacterFocusPatch
    {
        private static void Postfix(Fungus.Character character) { MoodControl.RefreshCharacter(character); }
    }
}
