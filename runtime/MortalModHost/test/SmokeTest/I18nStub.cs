namespace MortalModHost
{
    /// <summary>Offline-only replacement for Unity/LeanLocalization-backed I18n.</summary>
    internal static class I18n
    {
        internal static string CurrentStoryLocale = "zh_CN";
        internal static string StoryLocale { get { return CurrentStoryLocale; } }
    }
}
