using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>原版+MOD 存档的包依赖记录；不读取或改写原版 GameSave。</summary>
    internal static class AddonSaveDependencies
    {
        private const int MaxBytes = 256 * 1024;
        private const int MaxPackages = 256;
        private static readonly Regex ModId = new Regex("^[a-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);
        private static string _root;

        internal static void Initialize(string root)
        {
            if (string.IsNullOrWhiteSpace(root)) throw new ArgumentException("MOD 依赖记录目录不能为空", nameof(root));
            _root = Path.GetFullPath(root);
        }

        internal static bool HasRecord(string slot)
        {
            return File.Exists(PathFor(slot));
        }

        // The caller supplies its actual enabled package collection, not every installed ZIP.
        internal static void Record(string slot, IEnumerable<ModPackage> enabledPackages)
        {
            string target = PathFor(slot);
            var packages = EnabledAddons(enabledPackages);
            var text = new StringBuilder("{\"format\":1,\"slot\":").Append(Quote(slot)).Append(",\"mods\":[");
            bool first = true;
            foreach (var entry in packages)
            {
                if (!first) text.Append(',');
                first = false;
                ModPackage package = entry.Value;
                text.Append("{\"id\":").Append(Quote(package.Id))
                    .Append(",\"name\":").Append(Quote(string.IsNullOrWhiteSpace(package.Name) ? package.Id : package.Name))
                    .Append(",\"version\":").Append(Quote(package.Version ?? "")).Append('}');
            }
            byte[] payload = new UTF8Encoding(false, true).GetBytes(text.Append("]}").ToString());
            if (payload.Length > MaxBytes) throw new InvalidDataException("MOD 依赖记录超过 256 KiB");
            Directory.CreateDirectory(_root);
            string temporary = target + ".tmp." + Guid.NewGuid().ToString("N");
            try
            {
                using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    stream.Write(payload, 0, payload.Length);
                    stream.Flush(true);
                }
                if (File.Exists(target)) File.Replace(temporary, target, null);
                else File.Move(temporary, target);
            }
            finally
            {
                if (File.Exists(temporary)) File.Delete(temporary);
            }
        }

        internal static List<string> Missing(string slot, IEnumerable<ModPackage> enabledPackages)
        {
            string path = PathFor(slot);
            if (!File.Exists(path))
                throw new InvalidDataException("此存档没有 MOD 依赖记录，无法确认依赖是否齐全；请先核对原存档使用的 MOD");
            if (new FileInfo(path).Length > MaxBytes)
                throw new InvalidDataException("MOD 依赖记录超过 256 KiB");
            Dictionary<string, object> data;
            try { data = MiniJson.Parse(File.ReadAllText(path, Encoding.UTF8)) as Dictionary<string, object>; }
            catch (FormatException exception) { throw new InvalidDataException("MOD 依赖记录不是有效 JSON", exception); }
            object value;
            if (data == null || !data.TryGetValue("format", out value) || !(value is double) || (double)value != 1)
                throw new InvalidDataException("MOD 依赖记录格式必须为 1");
            if (!data.TryGetValue("slot", out value) || value as string != slot)
                throw new InvalidDataException("MOD 依赖记录的存档归属不匹配");
            if (!data.TryGetValue("mods", out value) || !(value is IList<object>))
                throw new InvalidDataException("MOD 依赖记录缺少 mods 列表");
            var rows = (IList<object>)value;
            if (rows.Count > MaxPackages) throw new InvalidDataException("MOD 依赖记录最多 256 个包");
            var enabled = EnabledAddons(enabledPackages);
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var missing = new List<string>();
            foreach (object item in rows)
            {
                var row = item as Dictionary<string, object>;
                string id = RequiredText(row, "id"), name = RequiredText(row, "name"), version = RequiredText(row, "version");
                if (!ModId.IsMatch(id) || !seen.Add(id))
                    throw new InvalidDataException("MOD 依赖记录的包 ID 非法或重复");
                if (!enabled.ContainsKey(id))
                    missing.Add((string.IsNullOrWhiteSpace(name) ? id : name) + "（" + id
                        + (string.IsNullOrEmpty(version) ? "" : "，" + version) + "）");
            }
            return missing;
        }

        private static SortedDictionary<string, ModPackage> EnabledAddons(IEnumerable<ModPackage> enabledPackages)
        {
            if (enabledPackages == null) throw new ArgumentNullException(nameof(enabledPackages));
            var result = new SortedDictionary<string, ModPackage>(StringComparer.Ordinal);
            foreach (ModPackage package in enabledPackages)
            {
                if (package == null) throw new ArgumentException("启用包列表含有空项", nameof(enabledPackages));
                if (package.PlayMode != "addon") continue;
                if (string.IsNullOrEmpty(package.Id) || !ModId.IsMatch(package.Id) || result.ContainsKey(package.Id))
                    throw new ArgumentException("启用附加包的 ID 非法或重复", nameof(enabledPackages));
                result.Add(package.Id, package);
                if (result.Count > MaxPackages) throw new ArgumentException("最多记录 256 个启用附加包", nameof(enabledPackages));
            }
            return result;
        }

        private static string PathFor(string slot)
        {
            if (!ModSaveSlotPolicy.IsAddonSlot(slot)) throw new ArgumentException("只能访问原版+MOD 的隔离存档依赖记录", nameof(slot));
            if (string.IsNullOrEmpty(_root)) throw new InvalidOperationException("MOD 依赖记录目录尚未初始化");
            string path = Path.GetFullPath(Path.Combine(_root, slot + ".addons.json"));
            string prefix = _root.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar) + Path.DirectorySeparatorChar;
            if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("MOD 依赖记录路径越出 Host 数据目录");
            return path;
        }

        private static string RequiredText(Dictionary<string, object> row, string key)
        {
            object value;
            if (row == null || !row.TryGetValue(key, out value) || !(value is string))
                throw new InvalidDataException("MOD 依赖记录的 " + key + " 必须是字符串");
            return (string)value;
        }

        private static string Quote(string value)
        {
            var text = new StringBuilder("\"");
            foreach (char character in value)
            {
                switch (character)
                {
                    case '\\': text.Append("\\\\"); break;
                    case '"': text.Append("\\\""); break;
                    default:
                        if (character < 32) text.Append("\\u" + ((int)character).ToString("x4"));
                        else text.Append(character);
                        break;
                }
            }
            return text.Append('"').ToString();
        }
    }
}
