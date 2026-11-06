using System;
using System.Collections.Generic;
using System.Text;

namespace MortalModHost
{
    /// <summary>Apply all anchored changes before compiling the installed Lua.
    /// Untouched game code, branch scopes and progression commands retain their exact text.</summary>
    internal static class VanillaStoryPatch
    {
        internal static bool ValidTarget(string target)
        {
            if (string.IsNullOrEmpty(target) || target.Length > 64 || target.StartsWith("MOD_", StringComparison.Ordinal))
                return false;
            foreach (char c in target)
                if (!(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_' || c == '-'))
                    return false;
            return true;
        }

        private sealed class Replacement
        {
            internal int Start;
            internal int End;
            internal string Text;
        }

        internal static string Apply(string source, string editsJson)
        {
            if (source == null) throw new ArgumentNullException("source");
            var edits = MiniJson.Parse(editsJson) as List<object>;
            if (edits == null || edits.Count > 4096) throw new FormatException("原版剧情补丁必须是最多 4096 项的修改列表");
            var replacements = new List<Replacement>();
            foreach (object value in edits)
            {
                var edit = value as Dictionary<string, object>;
                object findValue, replaceValue, occurrenceValue;
                if (edit == null || edit.Count != 3 || !edit.TryGetValue("find", out findValue)
                    || !edit.TryGetValue("replace", out replaceValue) || !edit.TryGetValue("occurrence", out occurrenceValue)
                    || !(findValue is string) || !(replaceValue is string)
                    || !(occurrenceValue is double || occurrenceValue is long || occurrenceValue is int))
                    throw new FormatException("原版剧情补丁修改格式无效");
                string find = (string)findValue;
                string replacement = (string)replaceValue;
                double count = Convert.ToDouble(occurrenceValue, System.Globalization.CultureInfo.InvariantCulture);
                if (find.Length == 0 || find.Length > 65536 || replacement.Length > 262144
                    || count < 1 || count > 1000000 || count != Math.Floor(count))
                    throw new FormatException("原版剧情补丁的定位或替换文本无效");
                int start = -find.Length;
                for (int i = 0; i < (int)count; i++)
                {
                    start = source.IndexOf(find, start + find.Length, StringComparison.Ordinal);
                    if (start < 0)
                        throw new InvalidOperationException("补丁定位原文不存在，游戏章节可能已更新；请重新读取原版章节后核对修改");
                }
                replacements.Add(new Replacement {Start = start, End = start + find.Length, Text = replacement});
            }
            replacements.Sort((a, b) => a.Start.CompareTo(b.Start));
            int end = 0;
            var result = new StringBuilder();
            foreach (Replacement replacement in replacements)
            {
                if (replacement.Start < end) throw new FormatException("原版剧情补丁修改范围重叠");
                result.Append(source, end, replacement.Start - end);
                result.Append(replacement.Text);
                end = replacement.End;
            }
            result.Append(source, end, source.Length - end);
            return result.ToString();
        }
    }
}
