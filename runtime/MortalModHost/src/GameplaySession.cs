using System;
using System.Collections.Generic;
using System.Globalization;
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
        private static string _lastOwner = "";
        private static string _lastStory = "";
        private static readonly Dictionary<string, string> _config =
            new Dictionary<string, string>(StringComparer.Ordinal);

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
            _lastOwner = "";
            _lastStory = "";
            _config.Clear();
        }

        internal static void Configure(string kind, string encoded)
        {
            if (!HasPending || !string.Equals(_kind, kind, StringComparison.Ordinal))
                throw new InvalidOperationException("Gameplay 配置必须紧跟对应的 prepare");
            if (encoded == null || encoded.Length > 8192)
                throw new ArgumentException("Gameplay 配置超过 8192 字符上限");
            _config.Clear();
            if (encoded.Length == 0) return;
            string[] fields = encoded.Split(';');
            if (fields.Length > 64)
                throw new ArgumentException("Gameplay 配置字段过多");
            foreach (string field in fields)
            {
                int pivot = field.IndexOf('=');
                if (pivot <= 0) throw new ArgumentException("Gameplay 配置格式错误");
                string key = field.Substring(0, pivot);
                string value = field.Substring(pivot + 1);
                RequireSegment("config key", key);
                if (value.Length > 4096)
                    throw new ArgumentException("Gameplay 配置值过长：" + key);
                if (_config.ContainsKey(key))
                    throw new ArgumentException("Gameplay 配置字段重复：" + key);
                _config.Add(key, value);
            }
        }

        internal static string ConfigString(string key)
        {
            string value;
            return _config.TryGetValue(key, out value) ? value : "";
        }

        internal static bool HasConfig(string key)
        {
            return _config.ContainsKey(key);
        }

        internal static bool TryConfigInt(string key, int min, int max, out int value)
        {
            value = 0;
            string raw;
            if (!_config.TryGetValue(key, out raw)) return false;
            if (!int.TryParse(raw, NumberStyles.Integer, CultureInfo.InvariantCulture, out value)
                || value < min || value > max)
                throw new InvalidOperationException("Gameplay 整数配置越界：" + key);
            return true;
        }

        internal static bool TryConfigFloat(string key, float min, float max, out float value)
        {
            value = 0f;
            string raw;
            if (!_config.TryGetValue(key, out raw)) return false;
            if (!float.TryParse(raw, NumberStyles.Float, CultureInfo.InvariantCulture, out value)
                || float.IsNaN(value) || float.IsInfinity(value) || value < min || value > max)
                throw new InvalidOperationException("Gameplay 小数配置越界：" + key);
            return true;
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
            _lastOwner = _owner;
            _lastStory = _story;
            return true;
        }

        internal static string ReadLastResult(ModPackage package, string story, string kind)
        {
            string owner = Owner(package);
            RequireSegment("story", story);
            if (!string.IsNullOrEmpty(kind)) RequireSegment("kind", kind);
            if (!string.Equals(_lastOwner, owner, StringComparison.Ordinal)
                || !string.Equals(_lastStory, story, StringComparison.Ordinal))
                return "";
            if (!string.IsNullOrEmpty(kind)
                && !string.Equals(_lastKind, kind, StringComparison.Ordinal))
                return "";
            return _lastResult;
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
            _lastOwner = "";
            _lastStory = "";
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
            _config.Clear();
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
