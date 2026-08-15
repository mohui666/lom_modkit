using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// Bounded, serialization-safe failure snapshot. It deliberately contains no
    /// Unity/BepInEx types so capture and tests remain available when the game runtime
    /// is already partly torn down.
    /// </summary>
    internal sealed class StructuredRuntimeError
    {
        internal readonly string TimestampUtc;
        internal readonly string ModId;
        internal readonly string ModName;
        internal readonly string Version;
        internal readonly string Story;
        internal readonly string Node;
        internal readonly string Category;
        internal readonly string Error;
        internal readonly string[] RecentTrace;

        internal StructuredRuntimeError(
            string timestampUtc, string modId, string modName, string version,
            string story, string node, string category, string error,
            string[] recentTrace)
        {
            TimestampUtc = timestampUtc ?? "";
            ModId = modId ?? "";
            ModName = modName ?? "";
            Version = version ?? "";
            Story = story ?? "";
            Node = node ?? "";
            Category = category ?? "unknown";
            Error = error ?? "unknown runtime error";
            RecentTrace = recentTrace ?? new string[0];
        }

        internal string ToJson()
        {
            try
            {
                var text = new StringBuilder(1024);
                text.Append('{');
                AppendField(text, "timestamp_utc", TimestampUtc, true);
                AppendField(text, "mod_id", ModId, false);
                AppendField(text, "mod_name", ModName, false);
                AppendField(text, "version", Version, false);
                AppendField(text, "story", Story, false);
                AppendField(text, "node", Node, false);
                AppendField(text, "category", Category, false);
                AppendField(text, "error", Error, false);
                text.Append(",\"recent_trace\":[");
                for (int i = 0; i < RecentTrace.Length; i++)
                {
                    if (i > 0) text.Append(',');
                    AppendJsonString(text, RecentTrace[i]);
                }
                text.Append("]}");
                return text.ToString();
            }
            catch
            {
                return "{\"category\":\"error_report_failure\",\"error\":\"structured runtime error serialization failed\",\"recent_trace\":[]}";
            }
        }

        private static void AppendField(StringBuilder text, string key, string value, bool first)
        {
            if (!first) text.Append(',');
            AppendJsonString(text, key);
            text.Append(':');
            AppendJsonString(text, value);
        }

        private static void AppendJsonString(StringBuilder text, string value)
        {
            text.Append('"');
            string source = value ?? "";
            for (int i = 0; i < source.Length; i++)
            {
                char c = source[i];
                switch (c)
                {
                    case '"': text.Append("\\\""); break;
                    case '\\': text.Append("\\\\"); break;
                    case '\b': text.Append("\\b"); break;
                    case '\f': text.Append("\\f"); break;
                    case '\n': text.Append("\\n"); break;
                    case '\r': text.Append("\\r"); break;
                    case '\t': text.Append("\\t"); break;
                    default:
                        if (c < 0x20 || c == '\u2028' || c == '\u2029'
                            || char.IsSurrogate(c))
                            text.Append("\\u").Append(((int)c).ToString("X4", CultureInfo.InvariantCulture));
                        else
                            text.Append(c);
                        break;
                }
            }
            text.Append('"');
        }
    }

    /// <summary>
    /// Last-resort reporter. Every individual read, trace capture, serialization and
    /// logging step is isolated so reporting an original failure cannot cause another.
    /// </summary>
    internal static class RuntimeErrorReporter
    {
        private const int IdentityLimit = 256;
        private const int ErrorLimit = 8192;
        private const int TraceLimit = 16;
        private const int TraceLineLimit = 512;
        private static readonly object Gate = new object();
        private static StructuredRuntimeError _last;

        internal static StructuredRuntimeError Report(
            string category,
            string reason,
            Exception exception,
            ModPackage package = null,
            string registeredName = null,
            Action<string> logError = null)
        {
            StructuredRuntimeError report;
            try
            {
                report = Capture(category, reason, exception, package, registeredName);
            }
            catch
            {
                report = Fallback(category, reason);
            }

            try { lock (Gate) _last = report; }
            catch { /* reporting must never escape */ }
            try
            {
                if (!string.Equals(report.Category, "script_lookup", StringComparison.Ordinal))
                    RuntimeTrace.RuntimeError(report.Error);
            }
            catch { /* trace may be unavailable during teardown */ }
            try
            {
                if (logError != null)
                    logError("[mod-runtime-error] " + report.ToJson());
            }
            catch { /* a broken logger must not mask the original error */ }
            return report;
        }

        internal static StructuredRuntimeError LastSnapshot()
        {
            try { lock (Gate) return _last; }
            catch { return null; }
        }

        internal static void ResetForTests()
        {
            try { lock (Gate) _last = null; }
            catch { }
        }

        private static StructuredRuntimeError Capture(
            string category, string reason, Exception exception,
            ModPackage package, string registeredName)
        {
            string currentModRaw = Read(delegate { return RuntimeTrace.CurrentMod; }, IdentityLimit);
            bool registeredMatchesCurrent = string.IsNullOrEmpty(registeredName)
                || (currentModRaw.Length > 0 && registeredName.StartsWith(
                    "MOD_" + currentModRaw + "_", StringComparison.Ordinal));
            bool unknownContext = package == null && (
                string.Equals(category, "script_lookup", StringComparison.Ordinal)
                || !registeredMatchesCurrent);
            string currentMod = unknownContext ? "" : currentModRaw;
            string modId = package != null
                ? Read(delegate { return package.Id; }, IdentityLimit) : currentMod;
            string modName = package != null
                ? Read(delegate { return package.Name; }, IdentityLimit)
                : unknownContext ? "" : Read(delegate { return RuntimeTrace.CurrentModName; }, IdentityLimit);
            string version = package != null
                ? Read(delegate { return package.Version; }, IdentityLimit)
                : unknownContext ? "" : Read(delegate { return RuntimeTrace.CurrentVersion; }, IdentityLimit);
            string story = unknownContext ? Limit(registeredName, IdentityLimit)
                : StoryFromRegisteredName(package, registeredName);
            if (story.Length == 0)
                story = Read(delegate { return RuntimeTrace.CurrentStory; }, IdentityLimit);
            string node = "";
            if (!unknownContext && (modId.Length == 0
                || string.Equals(modId, currentMod, StringComparison.Ordinal)))
                node = Read(delegate { return RuntimeTrace.CurrentNode; }, IdentityLimit);

            string safeReason = Limit(reason, ErrorLimit);
            string exceptionText = exception == null ? "" : Read(
                delegate { return exception.ToString(); }, ErrorLimit);
            string error = safeReason;
            if (exceptionText.Length > 0)
                error = safeReason.Length > 0 ? safeReason + ": " + exceptionText : exceptionText;
            if (error.Length == 0) error = "unknown runtime error";
            error = Limit(error, ErrorLimit);

            var recent = new List<string>();
            List<RuntimeTrace.Entry> entries = null;
            try { entries = unknownContext ? null : RuntimeTrace.DiagnosticSnapshot(TraceLimit); }
            catch { entries = null; }
            if (entries != null)
            {
                foreach (RuntimeTrace.Entry item in entries)
                {
                    try
                    {
                        if (modId.Length > 0 && !string.Equals(item.ModId, modId, StringComparison.Ordinal))
                            continue;
                        string line = "#" + item.Sequence.ToString(CultureInfo.InvariantCulture)
                            + " " + (item.EventType ?? "unknown")
                            + " " + (item.StoryId ?? "") + "/" + (item.NodeId ?? "")
                            + (string.IsNullOrEmpty(item.Detail) ? "" : " " + item.Detail);
                        recent.Add(Limit(line, TraceLineLimit));
                    }
                    catch { /* skip one damaged breadcrumb */ }
                }
            }
            return new StructuredRuntimeError(
                DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture),
                modId, modName, version, story, node,
                Limit(category, 64).Length > 0 ? Limit(category, 64) : "unknown",
                error, recent.ToArray());
        }

        private static string StoryFromRegisteredName(ModPackage package, string registeredName)
        {
            try
            {
                if (package == null || string.IsNullOrEmpty(registeredName)) return "";
                string prefix = "MOD_" + (package.Id ?? "") + "_";
                return registeredName.StartsWith(prefix, StringComparison.Ordinal)
                    ? Limit(registeredName.Substring(prefix.Length), IdentityLimit) : "";
            }
            catch { return ""; }
        }

        private static StructuredRuntimeError Fallback(string category, string reason)
        {
            string safeCategory;
            string safeReason;
            try { safeCategory = Limit(category, 64); }
            catch { safeCategory = "error_report_failure"; }
            try { safeReason = Limit(reason, ErrorLimit); }
            catch { safeReason = "runtime failure (details unavailable)"; }
            return new StructuredRuntimeError(
                "", "", "", "", "", "",
                string.IsNullOrEmpty(safeCategory) ? "error_report_failure" : safeCategory,
                string.IsNullOrEmpty(safeReason) ? "runtime failure (details unavailable)" : safeReason,
                new string[0]);
        }

        private static string Read(Func<string> read, int limit)
        {
            try { return Limit(read != null ? read() : "", limit); }
            catch { return ""; }
        }

        private static string Limit(string value, int limit)
        {
            string text = value ?? "";
            if (limit < 0 || text.Length <= limit) return text;
            int count = limit;
            if (count > 0 && char.IsHighSurrogate(text[count - 1])) count--;
            return text.Substring(0, count) + "…";
        }
    }
}
