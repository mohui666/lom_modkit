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

        /// <summary>
        /// true 时该 mod 战役期间禁用原版事件：Free 场景位置点击不再触发官方默认故事脚本
        /// （manifest.campaign.disable_official_events，契约 §2，缺省 false；只有 mod 自己的位置触发器命中）。
        /// 运行态由 <see cref="ModCampaignState"/> 记录，FreePositionPatch 据此抑制官方脚本。
        /// </summary>
        public bool DisableOfficialEvents;

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

        /// <summary>可空：仅指定月份（1~12）时触发器才生效（契约 §2.1，与 flag 条件全部 AND）。</summary>
        public int? WhenMonth;

        /// <summary>可空：仅指定月份阶段（1=上旬 2=中旬 3=下旬）时触发器才生效（契约 §2.1）。</summary>
        public int? WhenStage;

        /// <summary>可空：角色好感度下限条件（character+min，契约 §2.1）。</summary>
        public AffinityCondition WhenAffinity;

        /// <summary>
        /// flag 条件判定（契约 §2：when_flag_set 已设置 / when_flag_clear 未设置时才生效；都不写则无条件）。
        /// storyKeys = PlayerStatManagerData.StoryKeyList，允许传 null（视为无任何 flag）。
        /// 时间/好感条件（when_month/when_stage/when_affinity）需要游戏运行时状态，由
        /// FreePositionPatch.IsTimeAndAffinityMet 评估（本方法保持纯静态、无游戏依赖，便于离线单测）。
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

    /// <summary>
    /// 触发器好感度条件（契约 §2.1）：character 为 RelationshipStatType 的 StringValue 契约 id
    /// （sister1 / brother1~4 / master / girl1 等），min 为好感最低值（当前值 ≥ min 才生效）。
    /// </summary>
    internal sealed class AffinityCondition
    {
        public string Character;

        public int Min;
    }
}
