using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal sealed class LegacyExtensionsConfig
    {
        internal readonly Dictionary<string, Dictionary<string, string>> Equivalents =
            new Dictionary<string, Dictionary<string, string>>(StringComparer.Ordinal);
        internal readonly List<LegacyTable> Tables = new List<LegacyTable>();

        internal static bool HasOriginalContent(ModPackage package)
        {
            if (package == null) return false;
            var resources = package.LegacyResources;
            if (resources != null && (resources.Sprites.Count != 0 || resources.Voices.Count != 0 || resources.Strings.Count != 0)) return true;
            var extensions = package.LegacyExtensions;
            if (extensions == null) return false;
            if (extensions.Tables.Count != 0) return true;
            foreach (var files in extensions.Equivalents.Values) if (files.Count != 0) return true;
            return false;
        }

        internal static LegacyExtensionsConfig Parse(Dictionary<string, object> legacy)
        {
            if (legacy == null) return null;
            object schema;
            if (!legacy.TryGetValue("schema", out schema) || !(schema is double) || (double)schema != 1)
                throw new FormatException("manifest.legacy.schema 必须为 1");
            var result = new LegacyExtensionsConfig();
            object value;
            if (legacy.TryGetValue("equivalents", out value))
            {
                var kinds = value as Dictionary<string, object>;
                if (kinds == null) throw new FormatException("legacy.equivalents 必须是对象");
                foreach (var kind in kinds)
                {
                    if (kind.Key != "Condition" && kind.Key != "Switch" && kind.Key != "Position")
                        throw new FormatException("未知 LuaEquivalent 类型：" + kind.Key);
                    var files = kind.Value as Dictionary<string, object>;
                    if (files == null) throw new FormatException("LuaEquivalent 类型必须是文件映射：" + kind.Key);
                    var sources = new Dictionary<string, string>(StringComparer.Ordinal);
                    foreach (var file in files)
                    {
                        if (string.IsNullOrEmpty(file.Key) || !(file.Value is string))
                            throw new FormatException("LuaEquivalent 文件必须是非空名称与 Lua 文本");
                        sources.Add(file.Key, (string)file.Value);
                    }
                    result.Equivalents.Add(kind.Key, sources);
                }
            }
            if (legacy.TryGetValue("tables", out value))
            {
                var rows = value as List<object>;
                if (rows == null) throw new FormatException("legacy.tables 必须是数组");
                var names = new HashSet<string>(StringComparer.Ordinal);
                foreach (object row in rows)
                {
                    var table = row as Dictionary<string, object>;
                    object type, name, fields;
                    if (table == null || !table.TryGetValue("type", out type) || !(type is string)
                        || !table.TryGetValue("name", out name) || !(name is string)
                        || !table.TryGetValue("fields", out fields) || !(fields is Dictionary<string, object>)
                        || string.IsNullOrEmpty((string)type) || string.IsNullOrEmpty((string)name))
                        throw new FormatException("DataTable 必须包含 type、name 与 fields 对象");
                    if (!names.Add(type + "/" + name)) throw new FormatException("DataTable 重复：" + type + "/" + name);
                    result.Tables.Add(new LegacyTable { Type = (string)type, Name = (string)name, Fields = (Dictionary<string, object>)fields });
                }
            }
            return result;
        }
    }

    internal sealed class LegacyTable
    {
        internal string Type, Name;
        internal Dictionary<string, object> Fields;
    }
}
