using System;

namespace MortalModHost
{
    /// <summary>
    /// 官方背景 Addressables 地址的纯逻辑边界。游戏资源同时包含 PNG 与 JPG，
    /// 扩展名不是权限边界；目录前缀才是。
    /// </summary>
    internal static class CombatBackgroundAddressPolicy
    {
        private const string OfficialImagePrefix = "Assets/__Project/Images/";
        private const string CombatPrefix = "combat:";

        internal static bool TryGetCombatSpriteName(string key, out string name)
        {
            name = null;
            if (key == null || !key.StartsWith(CombatPrefix, StringComparison.Ordinal)
                || key.Length == CombatPrefix.Length) return false;
            name = key.Substring(CombatPrefix.Length);
            return true;
        }

        internal static bool IsOfficialImageAddress(string address)
        {
            if (string.IsNullOrEmpty(address)
                || !address.StartsWith(OfficialImagePrefix, StringComparison.Ordinal))
                return false;
            return address.EndsWith(".png", StringComparison.OrdinalIgnoreCase)
                || address.EndsWith(".jpg", StringComparison.OrdinalIgnoreCase)
                || address.EndsWith(".jpeg", StringComparison.OrdinalIgnoreCase);
        }
    }
}
