namespace MortalModHost
{
    /// <summary>
    /// 决斗详情面板的显示上限。CombatStat 上性情/处世/道德/修养/内功的
    /// inspector MaxValue 是 100；原版 SetSliderValue 却拿玩家 GameStat.Max
    ///（通常 50）做分母，作者填 100 时滑条看起来像被截在一半。
    /// </summary>
    internal static class CombatStatDisplayPolicy
    {
        internal const int SliderMax = 100;
    }
}
