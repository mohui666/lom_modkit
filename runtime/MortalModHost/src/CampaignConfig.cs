using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// manifest.campaign（契约 §2）：战役模式配置。挂在 <see cref="ModPackage.Campaign"/>，null 表示无战役模式。
    /// </summary>
    internal sealed class CampaignConfig
    {
        /// <summary>true 时出现在 mod 菜单"开始新战役"区（隔离存档槽开新游戏）。</summary>
        public bool NewGame;

        /// <summary>自由模式位置触发器（type="position"）。</summary>
        public readonly List<CampaignTrigger> Triggers = new List<CampaignTrigger>();
    }

    /// <summary>
    /// 一个位置触发器：点击地图位置 Position 且无官方任务占用时，默认活动脚本替换为 Script（同包脚本 id）。
    /// </summary>
    internal sealed class CampaignTrigger
    {
        /// <summary>PositionType 枚举的契约 id（Mall/Center/Alchemy/Forge/BackMountain/Room1/Door/Study/Kitchen/Room2/Secret）。</summary>
        public string Position;

        /// <summary>同包剧情脚本 id（manifest 解析时已校验存在）。</summary>
        public string Script;

        /// <summary>可空：该剧情 flag 已在 StoryKeyList 时触发器才生效。</summary>
        public string WhenFlagSet;

        /// <summary>可空：该剧情 flag 不在 StoryKeyList 时触发器才生效。</summary>
        public string WhenFlagClear;

        /// <summary>
        /// flag 条件判定（契约 §2：when_flag_set 已设置 / when_flag_clear 未设置时才生效；都不写则无条件）。
        /// storyKeys = PlayerStatManagerData.StoryKeyList，允许传 null（视为无任何 flag）。
        /// </summary>
        public bool IsConditionMet(ICollection<string> storyKeys)
        {
            if (!string.IsNullOrEmpty(WhenFlagSet) && (storyKeys == null || !storyKeys.Contains(WhenFlagSet)))
                return false;
            if (!string.IsNullOrEmpty(WhenFlagClear) && storyKeys != null && storyKeys.Contains(WhenFlagClear))
                return false;
            return true;
        }
    }
}
