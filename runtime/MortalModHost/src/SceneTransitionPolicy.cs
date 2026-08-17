namespace MortalModHost
{
    /// <summary>
    /// SceneController 的 IsPrepare/IsLoading 只描述目标场景载入阶段；目标场景已经
    /// 激活但其 Loading1 仍等待目标控制器卸载时，两个属性都可能为 false。
    /// 此时再次开始战役会重复载入同名 Loading1，并让后续 UnloadLoading 永久等待。
    /// </summary>
    internal static class SceneTransitionPolicy
    {
        internal static bool IsReady(
            bool isPrepare, bool isLoading, string currentLoadingScene)
        {
            return !isPrepare && !isLoading
                && string.IsNullOrEmpty(currentLoadingScene);
        }
    }
}
