using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// 旧默认热键 F9 → F8 的一次性配置迁移（纯字符串处理，可离线测试）。
    /// 只改写值恰为 F9 的 MenuHotkey 设置行；注释行、用户改过的值、带修饰键的值一律不动。
    /// </summary>
    internal static class HotkeyMigration
    {
        private static readonly Regex HotkeyLine = new Regex(
            @"(?m)^(?<prefix>\s*MenuHotkey\s*=\s*)F9(?<suffix>[ \t]*\r?)$",
            RegexOptions.Compiled);

        /// <summary>cfg 文本里存在 MenuHotkey = F9 行时改写为 F8 并返回 true；无匹配返回 false。</summary>
        public static bool TryRewriteLegacyHotkey(string cfgText, out string migrated)
        {
            migrated = null;
            if (string.IsNullOrEmpty(cfgText)) return false;
            bool changed = false;
            string result = HotkeyLine.Replace(cfgText, match =>
            {
                changed = true;
                return match.Groups["prefix"].Value + "F8" + match.Groups["suffix"].Value;
            });
            if (!changed) return false;
            migrated = result;
            return true;
        }

        /// <summary>
        /// 一次性迁移门闩。completed 为 true 后永不再改写，因此用户之后主动设回 F9
        /// 会被保留。调用方无论当前值是否需要改写，都应持久化 completed=true。
        /// </summary>
        public static bool TryRewriteLegacyHotkeyOnce(
            string cfgText, bool completed, out string migrated, out bool markCompleted)
        {
            migrated = null;
            markCompleted = !completed;
            return !completed && TryRewriteLegacyHotkey(cfgText, out migrated);
        }
    }
}
