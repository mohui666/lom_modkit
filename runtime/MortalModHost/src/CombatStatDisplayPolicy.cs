namespace MortalModHost
{
    /// <summary>
    /// 决斗详情面板的显示上限。CombatStat 上性情/处世/道德/修养/内功的
    /// inspector MaxValue 是 100；玩家 GameStat.Max 官方默认也是 100。
    /// 原版 SetSliderValue 用 GameStat.Max 做分母和 LevelText；只有资产 Max
    /// 小于 100 时才按 CombatStat 100 重画填充。评语始终走官方 LevelText。
    /// </summary>
    internal static class CombatStatDisplayPolicy
    {
        internal const int SliderMax = 100;

        /// <summary>
        /// 对齐 GameStatUtils.GetGameStatLevel，但先把作者值夹到官方 Max，
        /// 避免 100 / 50 越出 LevelText。
        /// </summary>
        internal static int OfficialLevelIndex(int value, int officialMax, int levelLength)
        {
            if (levelLength <= 0) return 0;
            int max = officialMax > 0 ? officialMax : SliderMax;
            int clamped = value;
            if (clamped < 0) clamped = 0;
            if (clamped > max) clamped = max;
            int band = max / levelLength;
            if (band <= 0) return 0;
            int index = clamped / band;
            if (index == levelLength) index--;
            if (index < 0) index = 0;
            if (index >= levelLength) index = levelLength - 1;
            return index;
        }
    }
}
