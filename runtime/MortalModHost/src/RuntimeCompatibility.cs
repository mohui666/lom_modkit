using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal sealed class CompatibilityResult
    {
        public bool IsCompatible = true;
        public string Error;
        public readonly List<string> Warnings = new List<string>();
    }

    /// <summary>Pure compatibility evaluation for optional manifest metadata.</summary>
    internal static class RuntimeCompatibility
    {
        private sealed class SemanticVersion : IComparable<SemanticVersion>
        {
            public int Major;
            public int Minor;
            public int Patch;
            public string[] PreRelease;

            public int CompareTo(SemanticVersion other)
            {
                int core = Major.CompareTo(other.Major);
                if (core == 0) core = Minor.CompareTo(other.Minor);
                if (core == 0) core = Patch.CompareTo(other.Patch);
                if (core != 0) return core;
                bool thisStable = PreRelease == null;
                bool otherStable = other.PreRelease == null;
                if (thisStable || otherStable)
                    return thisStable == otherStable ? 0 : (thisStable ? 1 : -1);
                int count = Math.Min(PreRelease.Length, other.PreRelease.Length);
                for (int i = 0; i < count; i++)
                {
                    string left = PreRelease[i];
                    string right = other.PreRelease[i];
                    if (left == right) continue;
                    int leftNumber, rightNumber;
                    bool leftNumeric = int.TryParse(left, out leftNumber);
                    bool rightNumeric = int.TryParse(right, out rightNumber);
                    if (leftNumeric && rightNumeric) return leftNumber.CompareTo(rightNumber);
                    if (leftNumeric != rightNumeric) return leftNumeric ? -1 : 1;
                    return string.CompareOrdinal(left, right);
                }
                return PreRelease.Length.CompareTo(other.PreRelease.Length);
            }

            public static bool TryParse(string text, out SemanticVersion value)
            {
                value = null;
                if (string.IsNullOrEmpty(text) || text.Length > 64)
                    return false;
                string core = text;
                string prerelease = null;
                int plus = core.IndexOf('+');
                if (plus >= 0)
                {
                    if (plus == core.Length - 1 || core.IndexOf('+', plus + 1) >= 0)
                        return false;
                    string[] buildParts = core.Substring(plus + 1).Split('.');
                    foreach (string identifier in buildParts)
                    {
                        if (identifier.Length == 0) return false;
                        foreach (char c in identifier)
                            if (!(IsAsciiLetterOrDigit(c) || c == '-')) return false;
                    }
                    core = core.Substring(0, plus);
                }
                int dash = core.IndexOf('-');
                if (dash >= 0)
                {
                    if (dash == core.Length - 1) return false;
                    prerelease = core.Substring(dash + 1);
                    core = core.Substring(0, dash);
                }
                string[] parts = core.Split('.');
                int major, minor, patch;
                if (parts.Length != 3
                    || !TryParseNumber(parts[0], out major)
                    || !TryParseNumber(parts[1], out minor)
                    || !TryParseNumber(parts[2], out patch))
                    return false;
                string[] pre = null;
                if (prerelease != null)
                {
                    pre = prerelease.Split('.');
                    foreach (string identifier in pre)
                    {
                        if (identifier.Length == 0) return false;
                        foreach (char c in identifier)
                            if (!(IsAsciiLetterOrDigit(c) || c == '-')) return false;
                        int numeric;
                        bool allDigits = true;
                        foreach (char c in identifier)
                            if (c < '0' || c > '9') { allDigits = false; break; }
                        if (allDigits && ((identifier.Length > 1 && identifier[0] == '0')
                            || !int.TryParse(identifier, out numeric))) return false;
                    }
                }
                value = new SemanticVersion
                {
                    Major = major,
                    Minor = minor,
                    Patch = patch,
                    PreRelease = pre
                };
                return true;
            }

            private static bool TryParseNumber(string text, out int number)
            {
                number = 0;
                if (string.IsNullOrEmpty(text) || (text.Length > 1 && text[0] == '0'))
                    return false;
                foreach (char c in text)
                    if (c < '0' || c > '9') return false;
                return int.TryParse(text, out number) && number >= 0;
            }
        }

        public static CompatibilityResult Evaluate(
            ModPackage package, string currentHostVersion, string currentGameVersion)
        {
            var result = new CompatibilityResult();
            SemanticVersion currentHost;
            if (!SemanticVersion.TryParse(currentHostVersion, out currentHost))
            {
                result.IsCompatible = false;
                result.Error = "Host 自身版本无法解析：" + (currentHostVersion ?? "<null>");
                return result;
            }

            SemanticVersion minimum = null;
            if (!string.IsNullOrEmpty(package.MinHostVersion))
            {
                if (!SemanticVersion.TryParse(package.MinHostVersion, out minimum))
                    return Invalid(result, "manifest.min_host_version 不是合法 SemVer");
                if (currentHost.CompareTo(minimum) < 0)
                    return Invalid(result, "需要 MortalModHost >= " + package.MinHostVersion
                        + "，当前为 " + currentHostVersion);
            }
            SemanticVersion tested = null;
            if (!string.IsNullOrEmpty(package.TestedHostVersion))
            {
                if (!SemanticVersion.TryParse(package.TestedHostVersion, out tested))
                    return Invalid(result, "manifest.tested_host_version 不是合法 SemVer");
                if (currentHost.CompareTo(tested) > 0)
                    result.Warnings.Add("作者仅测试到 Host " + package.TestedHostVersion
                        + "，当前为 " + currentHostVersion);
            }
            if (minimum != null && tested != null && minimum.CompareTo(tested) > 0)
                return Invalid(result, "min_host_version 不能高于 tested_host_version");

            string game = currentGameVersion ?? "";
            if (!string.IsNullOrEmpty(package.GameVersion)
                && !IsGameVersionToken(package.GameVersion))
                return Invalid(result, "manifest.game_version 不是合法版本标识");
            if (!string.IsNullOrEmpty(package.TestedGameVersion)
                && !IsGameVersionToken(package.TestedGameVersion))
                return Invalid(result, "manifest.tested_game_version 不是合法版本标识");
            if (!string.IsNullOrEmpty(package.GameVersion)
                && !string.IsNullOrEmpty(package.TestedGameVersion)
                && !string.Equals(package.GameVersion, package.TestedGameVersion,
                    StringComparison.OrdinalIgnoreCase))
                return Invalid(result, "game_version 与 tested_game_version 互相矛盾");
            if (!string.IsNullOrEmpty(package.GameVersion)
                && !string.Equals(package.GameVersion, game, StringComparison.OrdinalIgnoreCase))
                return Invalid(result, "需要游戏版本 " + package.GameVersion
                    + "，当前为 " + (game.Length == 0 ? "<unknown>" : game));
            if (!string.IsNullOrEmpty(package.TestedGameVersion)
                && !string.Equals(package.TestedGameVersion, game, StringComparison.OrdinalIgnoreCase))
                result.Warnings.Add("作者测试的游戏版本为 " + package.TestedGameVersion
                    + "，当前为 " + (game.Length == 0 ? "<unknown>" : game));
            return result;
        }

        private static bool IsGameVersionToken(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > 64) return false;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (!(IsAsciiLetterOrDigit(c) || c == '.' || c == '_' || c == '+' || c == '-'))
                    return false;
            }
            return true;
        }

        private static bool IsAsciiLetterOrDigit(char c)
        {
            return (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z')
                || (c >= 'a' && c <= 'z');
        }

        private static CompatibilityResult Invalid(CompatibilityResult result, string error)
        {
            result.IsCompatible = false;
            result.Error = error;
            return result;
        }
    }
}
