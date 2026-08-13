namespace MortalModHost
{
    /// <summary>
    /// mod 战役运行态（契约 §2 disable_official_events）：
    /// 当前是否正处于某个 mod 的新战役（隔离存档槽 mod_&lt;modid&gt;），以及该战役 mod 是否声明了
    /// 禁用原版事件——FreePositionPatch 据此把未命中 mod 触发器的位置点击的官方默认脚本抑制掉。
    ///
    /// 设置时机：Plugin.StartCampaign（点击"开始新战役"按钮时，用该 mod 的 campaign.disable_official_events 值）。
    /// 清除时机：NewGameDataPatch 观察到官方开局（PendingCampaign 为空，即玩家用官方方式新开游戏）
    /// 或 mod 战役开局失败（PlayerStatManagerData 未就绪）时。注意：LuaManagerPatch 官方脚本分支
    /// 不重置本状态——mod 战役期间可能穿插官方脚本演出，演出完回到 Free 场景时禁用仍需生效。
    /// </summary>
    internal static class ModCampaignState
    {
        /// <summary>true 表示当前正处于某个 mod 新战役。</summary>
        internal static bool Active;

        /// <summary>当前战役 mod 的 manifest.campaign.disable_official_events 值（仅 Active 时有意义）。</summary>
        internal static bool DisableOfficialEvents;

        internal static void Enter(bool disableOfficialEvents)
        {
            Active = true;
            DisableOfficialEvents = disableOfficialEvents;
        }

        internal static void Clear()
        {
            Active = false;
            DisableOfficialEvents = false;
        }
    }
}
