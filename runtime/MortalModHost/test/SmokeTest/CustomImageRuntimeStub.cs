namespace MortalModHost
{
    /// <summary>离线测试替代品：真实背景恢复由 Unity Story Stage Runtime 提供。</summary>
    internal static class CustomImageRuntime
    {
        internal static string ActiveBackgroundReference { get; set; }

        internal static void RestoreBackgroundWhenStageReady(string raw)
        {
            ActiveBackgroundReference = raw ?? "";
        }
    }
}
