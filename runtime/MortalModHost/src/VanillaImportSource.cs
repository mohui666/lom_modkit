using System;
using System.Collections.Generic;
using System.Text;

namespace MortalModHost
{
    /// <summary>Source classification is derived from the packaged chapter, once.</summary>
    internal sealed class VanillaImportSource
    {
        internal const string CompletionSignal = "__lom_vanilla_import_complete__";
        private static readonly HashSet<string> TransientKinds = new HashSet<string>(StringComparer.Ordinal)
        {
            "say", "wait", "choice", "flag_check", "stat_check", "affinity_check",
            "item_check", "talent_check", "quest_check", "persistent_check"
        };
        internal string Target { get; private set; }
        internal string OriginalSource { get; private set; }
        internal bool LogicChanged { get; private set; }
        internal bool OriginalVerified { get; private set; }
        private readonly HashSet<string> _original = new HashSet<string>(StringComparer.Ordinal);
        private readonly HashSet<string> _persistent = new HashSet<string>(StringComparer.Ordinal);
        private bool _changed;

        internal static VanillaImportSource Parse(string json)
        {
            var story = MiniJson.Parse(json) as Dictionary<string, object>;
            object value;
            if (story == null || !story.TryGetValue("vanilla_import", out value)) return null;
            var metadata = value as Dictionary<string, object>;
            if (metadata == null || !Equals(metadata["schema"], 1d))
                throw new FormatException("原版整章来源格式无效");
            var result = new VanillaImportSource { Target = metadata["target"] as string };
            if (!VanillaStoryPatch.ValidTarget(result.Target))
                throw new FormatException("原版整章来源缺少章节编号");
            var origins = metadata["origins"] as List<object>;
            var nodes = story["nodes"] as List<object>;
            if (origins == null || origins.Count == 0 || origins.Count > 16384 || nodes == null)
                throw new FormatException("原版整章来源缺少步骤");
            var source = new StringBuilder();
            foreach (Dictionary<string, object> origin in origins)
                source.Append((string)origin["code"]);
            result.OriginalSource = source.ToString();
            if (Encoding.UTF8.GetByteCount(result.OriginalSource) > 4 * 1024 * 1024)
                throw new FormatException("原版来源文本超过 4 MiB");

            var claimed = new HashSet<int>();
            int previous = -1;
            foreach (Dictionary<string, object> node in nodes)
            {
                string id = (string)node["id"];
                int index = OriginIndex(node);
                bool native = index >= 0 && index < origins.Count && claimed.Add(index);
                if (native)
                {
                    if (index <= previous) result.LogicChanged = true;
                    previous = index;
                    var baseline = (Dictionary<string, object>)((Dictionary<string, object>)origins[index])["node"];
                    if (EqualNode(baseline, node))
                    {
                        result._original.Add(id);
                        continue;
                    }
                    if ((string)baseline["type"] == "raw") result.LogicChanged = true;
                }
                string kind = (string)node["type"];
                object resume;
                if (kind == "end" && node.TryGetValue("resume_original", out resume) && Equals(resume, true)) continue;
                if (!TransientKinds.Contains(kind)) result._persistent.Add(id);
                object inherit;
                if (kind == "say" && !native
                    && (!node.TryGetValue("inherit_dialog", out inherit) || !Equals(inherit, true)))
                    result._persistent.Add(id);
                // Speaker, portrait and layout edits continue to affect later lines.
                if (kind == "say" && native)
                {
                    var baseline = new Dictionary<string, object>((Dictionary<string, object>)((Dictionary<string, object>)origins[index])["node"]);
                    foreach (string key in new[] { "text", "font_size", "voice", "mood_icon", "mood_character" })
                    {
                        object text;
                        if (node.TryGetValue(key, out text)) baseline[key] = text;
                        else baseline.Remove(key);
                    }
                    if (!EqualNode(baseline, node)) result._persistent.Add(id);
                }
            }
            if (claimed.Count != origins.Count) result.LogicChanged = true;
            object mood;
            if (story.TryGetValue("mood", out mood) && Equals(mood, false)) result.LogicChanged = true;
            // Authored translations can change otherwise untouched native text.
            object localization;
            var localized = story.TryGetValue("localization", out localization)
                ? localization as Dictionary<string, object> : null;
            object translations;
            if (localized != null && localized.TryGetValue("translations", out translations))
            {
                foreach (var language in ((Dictionary<string, object>)translations).Values)
                    foreach (string key in ((Dictionary<string, object>)language).Keys)
                    {
                        int split = key.IndexOf('.');
                        if (split > 0 && !key.StartsWith("story.", StringComparison.Ordinal))
                            result._original.Remove(key.Substring(0,split));
                    }
            }
            return result;
        }

        internal void VerifyOriginal(string installedSource)
        {
            if (OriginalVerified) return;
            if (!string.Equals(OriginalSource.TrimStart('\uFEFF'),
                (installedSource ?? "").TrimStart('\uFEFF'), StringComparison.Ordinal))
                throw new InvalidOperationException("原版章节已变化，请重新导入并核对修改：" + Target);
            OriginalVerified = true;
        }

        internal bool Begin()
        {
            if (!OriginalVerified) throw new InvalidOperationException("原版章节来源尚未核对");
            _changed = LogicChanged;
            return !_changed;
        }

        internal bool Node(string id)
        {
            if (_persistent.Contains(id)) _changed = true;
            return !_changed && _original.Contains(id);
        }

        internal bool ContinueOriginal() { return !_changed; }

        private static int OriginIndex(Dictionary<string, object> node)
        {
            object editor, ordinal;
            if (!node.TryGetValue("_editor", out editor)) return -1;
            var fields = editor as Dictionary<string, object>;
            if (fields == null || !fields.TryGetValue("vanilla_origin", out ordinal)) return -1;
            if (!(ordinal is double)) return -1;
            double number = (double)ordinal;
            if (number < 0 || number > int.MaxValue || number != Math.Truncate(number)) return -1;
            return (int)number;
        }

        private static bool EqualNode(Dictionary<string, object> left, Dictionary<string, object> right)
        {
            var leftFields = new Dictionary<string, object>(left);
            var rightFields = new Dictionary<string, object>(right);
            leftFields.Remove("id"); leftFields.Remove("_editor");
            rightFields.Remove("id"); rightFields.Remove("_editor");
            return EqualValue(leftFields, rightFields);
        }

        private static bool EqualValue(object left, object right)
        {
            var a = left as Dictionary<string, object>;
            var b = right as Dictionary<string, object>;
            if (a != null && b != null)
            {
                if (a.Count != b.Count) return false;
                foreach (var pair in a)
                {
                    object value;
                    if (!b.TryGetValue(pair.Key, out value) || !EqualValue(pair.Value, value)) return false;
                }
                return true;
            }
            var x = left as List<object>;
            var y = right as List<object>;
            if (x != null && y != null)
            {
                if (x.Count != y.Count) return false;
                for (int i = 0; i < x.Count; ++i)
                    if (!EqualValue(x[i], y[i])) return false;
                return true;
            }
            return Equals(left, right);
        }
    }
}
