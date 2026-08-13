using System;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using Mortal.Free;
using Mortal.Story;

namespace MortalModHost
{
    /// <summary>
    /// 全局临时开关（会话级、不持久化）：F7 切换「禁用原版游戏剧情」。
    ///
    /// 背景：Free 自由场景点地图位置时，官方默认故事脚本优先于 mod 触发器；已有的战役级开关
    /// manifest.campaign.disable_official_events（见 CampaignPatch.FreePositionPatch）依赖 mod 战役
    /// 运行态（ModCampaignState）。本开关与之相互独立：不依赖战役，可在任意场景切换，
    /// 于返回 Free 的自动任务判定和下一次地点点击生效；不进 cfg（插件重载/游戏重启即复位为 false）。
    ///
    /// 抑制语义：mod 触发器仍优先；无 mod 触发器命中时，压掉官方主线、支线和
    /// 地点默认脚本。开关关闭后一切恢复原状，战役级开关语义不变。
    /// </summary>
    internal static class VanillaStorySwitch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        /// <summary>true 表示禁用原版剧情。会话级默认 false，不写 cfg、不随存档持久化。</summary>
        internal static bool Enabled;

        /// <summary>F7 全局开关或当前 mod 战役声明了禁用时，均应抑制官方剧情。</summary>
        internal static bool ShouldSuppress
        {
            get
            {
                return Enabled || (ModCampaignState.Active && ModCampaignState.DisableOfficialEvents);
            }
        }

        /// <summary>
        /// 翻转开关，并给出屏幕提示（走 StoryMainUI.Instance.DisplayMessageText）。
        /// 反编译证据（ilspycmd，Mortal.Story.dll）：Mortal.Story.StoryMainUI 为 MonoBehaviour 单例，
        /// <c>public void DisplayMessageText(string message)</c>（Mortal.Story.decompiled.cs:4891）
        /// 直接把文本投给 _messagePanel.DisplayMessage——官方编译器 message 节点就是发射这个调用。
        /// 单例可能为 null（不在演出场景时），提示失败只记日志不抛异常。
        /// </summary>
        internal static void Toggle()
        {
            Enabled = !Enabled;
            string message = Enabled
                ? "原版剧情已禁用（返回自由模式或下一次行动生效，F7 恢复）"
                : "原版剧情已恢复（F7 禁用）";
            Log.LogInfo("F7 全局开关切换：" + (Enabled ? "原版剧情已禁用。" : "原版剧情已恢复。"));
            try
            {
                if (StoryMainUI.Instance == null)
                {
                    Log.LogWarning("F7 开关屏幕提示失败：StoryMainUI 单例未就绪（当前可能不在演出场景）。");
                    return;
                }
                StoryMainUI.Instance.DisplayMessageText(message);
            }
            catch (Exception ex)
            {
                Log.LogWarning("F7 开关屏幕提示失败：" + ex.Message);
            }
        }
    }

    /// <summary>
    /// PositionController.OnPositionClick 的原版顺序是：主线 → 行动点 → 支线 → 地点脚本。
    /// 旧实现只 patch 最后的 GetExecuteScript，所以主线/支线会在到达 mod 触发器前就抢先进入。
    /// 这里仅在一次同步点击调用期间标记「跳过官方任务判定」，让原方法自然落到
    /// GetExecuteScript；后者仍由 FreePositionPatch 先选 mod 触发器，无命中时再返回 null。
    /// </summary>
    [HarmonyPatch(typeof(PositionController), "OnPositionClick")]
    internal static class PositionClickStorySuppressionPatch
    {
        [ThreadStatic]
        private static bool _suppressMissionChecks;

        internal static bool SuppressMissionChecks
        {
            get { return _suppressMissionChecks; }
        }

        private static void Prefix()
        {
            _suppressMissionChecks = VanillaStorySwitch.ShouldSuppress;
        }

        /// <summary>Harmony finalizer 无论原方法是否抛异常都执行，避免标记泄漏到后续点击。</summary>
        private static Exception Finalizer(Exception __exception)
        {
            _suppressMissionChecks = false;
            return __exception;
        }
    }

    /// <summary>只在被上方 OnPositionClick 标记的同步调用内隐藏官方主线状态，不修改任务数据。</summary>
    [HarmonyPatch(typeof(MissionManagerData), "get_MainMissionStart")]
    internal static class MainMissionSuppressionPatch
    {
        private static void Postfix(ref bool __result)
        {
            if (PositionClickStorySuppressionPatch.SuppressMissionChecks
                || MissionRefreshStorySuppressionPatch.SuppressMissionChecks)
                __result = false;
        }
    }

    /// <summary>在同一点击内跳过官方支线，避免 HasTriggerSubMissions 先写入官方脚本。</summary>
    [HarmonyPatch(typeof(PositionController), "HasTriggerSubMissions")]
    internal static class SubMissionSuppressionPatch
    {
        private static bool Prefix(ref bool __result)
        {
            if (!PositionClickStorySuppressionPatch.SuppressMissionChecks)
                return true;
            __result = false;
            return false;
        }
    }

    /// <summary>
    /// 回到 Free 时，LuaManager.ChangeScene("Free") 会先调用 UpdateCheckMissions，再调用
    /// HasAnyMissionTrigger。原版 UpdateCheckMissions 在方法内部发现无地点主线后会立即写入
    /// StoryScript，并在 HasNextState 时推进任务状态；仅拦地点点击无法阻止这条自动触发链。
    ///
    /// 这里在一次 UpdateCheckMissions 调用期间让 get_MainMissionStart 临时返回 false，保留
    /// 官方的时间更新、条件计算和 ActiveSubMissions 刷新，但阻止尚未播放的主线被提前推进。
    /// 不修改 MainMissionStart 的真实值，关闭开关后原版任务仍可正常继续。
    /// </summary>
    [HarmonyPatch(typeof(MissionManagerData), "UpdateCheckMissions")]
    internal static class MissionRefreshStorySuppressionPatch
    {
        [ThreadStatic]
        private static bool _suppressMissionChecks;

        internal static bool SuppressMissionChecks
        {
            get { return _suppressMissionChecks; }
        }

        private static void Prefix()
        {
            _suppressMissionChecks = VanillaStorySwitch.ShouldSuppress;
        }

        private static Exception Finalizer(Exception __exception)
        {
            _suppressMissionChecks = false;
            return __exception;
        }
    }

    /// <summary>
    /// UpdateCheckMissions 之后的 HasAnyMissionTrigger 会启动无地点主线或支线。
    /// 禁用原版剧情时直接返回 false，使 ChangeScene("Free") 按原方法进入自由模式；
    /// mod 地点触发器仍由玩家点击地点时的 FreePositionPatch 处理，不经过本方法。
    /// </summary>
    [HarmonyPatch(typeof(MissionManagerData), "HasAnyMissionTrigger")]
    internal static class AutomaticMissionSuppressionPatch
    {
        private static bool Prefix(ref bool __result)
        {
            if (!VanillaStorySwitch.ShouldSuppress)
                return true;
            __result = false;
            return false;
        }
    }
}
