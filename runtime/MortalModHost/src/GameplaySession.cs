using System;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// 原版 Gameplay 场景与 MOD Story 之间的一次性结果信箱。这里只保存宿主状态，
    /// 不修改游戏存档；包 id + 完整 SHA-256 共同绑定所有权，避免同 id 换包接管结果。
    /// </summary>
    internal static class GameplaySession
    {
        private static readonly Regex Segment = new Regex(
            "^[A-Za-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);

        private static string _owner = "";
        private static string _kind = "";
        private static string _story = "";
        private static string _node = "";
        private static string _winTarget = "";
        private static string _loseTarget = "";
        private static string _result = "";
        private static string _lastKind = "";
        private static string _lastResult = "";

        internal static bool HasPending { get { return _owner.Length > 0; } }
        internal static bool PendingCombat
        {
            get { return HasPending && string.Equals(_kind, "combat", StringComparison.Ordinal); }
        }
        internal static bool PendingBattle
        {
            get { return HasPending && string.Equals(_kind, "battle", StringComparison.Ordinal); }
        }
        internal static bool ShouldForceCombatReturn { get { return PendingCombat; } }
        internal static string LastKind { get { return _lastKind; } }
        internal static string LastResult { get { return _lastResult; } }

        internal static void Prepare(
            ModPackage package, string kind, string story, string node,
            string winTarget, string loseTarget)
        {
            string owner = Owner(package);
            RequireSegment("kind", kind);
            RequireSegment("story", story);
            RequireSegment("node", node);
            RequireSegment("win", winTarget);
            RequireSegment("lose", loseTarget);
            if (!string.Equals(kind, "combat", StringComparison.Ordinal)
                && !string.Equals(kind, "battle", StringComparison.Ordinal))
                throw new InvalidOperationException("尚未支持的 Gameplay 类型：" + kind);
            if (HasPending)
                throw new InvalidOperationException(
                    "上一场 Gameplay 尚未完成，拒绝嵌套启动：" + _kind + "/" + _story + "/" + _node);

            _owner = owner;
            _kind = kind;
            _story = story;
            _node = node;
            _winTarget = winTarget;
            _loseTarget = loseTarget;
            _result = "";
            _lastKind = "";
            _lastResult = "";
        }

        internal static bool RecordResult(string kind, string result)
        {
            if (!HasPending || !string.Equals(_kind, kind, StringComparison.Ordinal))
                return false;
            if (!string.Equals(result, "win", StringComparison.Ordinal)
                && !string.Equals(result, "lose", StringComparison.Ordinal))
                return false;
            if (_result.Length > 0)
                return string.Equals(_result, result, StringComparison.Ordinal);
            _result = result;
            _lastKind = kind;
            _lastResult = result;
            return true;
        }

        internal static string ConsumeResume(ModPackage package, string story)
        {
            if (!HasPending) return "";
            string owner = Owner(package);
            if (!string.Equals(_owner, owner, StringComparison.Ordinal)
                || !string.Equals(_story, story, StringComparison.Ordinal))
                throw new InvalidOperationException("Gameplay 结果所有者或剧情不匹配，拒绝消费");
            if (_result.Length == 0) return "";
            string target = string.Equals(_result, "win", StringComparison.Ordinal)
                ? _winTarget : _loseTarget;
            ClearPending();
            return target;
        }

        internal static void Reset()
        {
            ClearPending();
            _lastKind = "";
            _lastResult = "";
        }

        private static void ClearPending()
        {
            _owner = "";
            _kind = "";
            _story = "";
            _node = "";
            _winTarget = "";
            _loseTarget = "";
            _result = "";
        }

        private static string Owner(ModPackage package)
        {
            if (package == null) throw new ArgumentNullException("package");
            if (string.IsNullOrEmpty(package.Id)
                || string.IsNullOrEmpty(package.PackageFingerprint)
                || package.PackageFingerprint.Length != 64)
                throw new InvalidOperationException("Gameplay 会话缺少可信的包 id / 完整 SHA-256 身份");
            return package.Id + "\n" + package.PackageFingerprint;
        }

        private static void RequireSegment(string name, string value)
        {
            if (string.IsNullOrEmpty(value) || !Segment.IsMatch(value))
                throw new ArgumentException(name + " 必须是 1~64 位字母、数字、下划线或短横线");
        }
    }
}
