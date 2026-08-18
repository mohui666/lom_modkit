using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// 与 mod_campaign_&lt;campaign_id&gt; 隔离槽一一绑定的纯 C# sidecar 存储。
    /// 不引用 Unity / BepInEx / Mortal.Core，也不修改游戏 GameSave schema。
    /// </summary>
    internal sealed class PersistentStateStore
    {
        private const int Format = 1;
        private const int MaxEntries = 256;
        private const int MaxBytes = 256 * 1024;
        private static readonly Regex PublicKey = new Regex(
            "^[A-Za-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);
        private static readonly Regex ModId = new Regex(
            "^[a-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);
        private static readonly Regex Sha256 = new Regex(
            "^[A-Fa-f0-9]{64}$", RegexOptions.CultureInvariant);
        private readonly string _root;
        private readonly Dictionary<string, int> _values =
            new Dictionary<string, int>(StringComparer.Ordinal);
        private string _modId = "";
        private string _campaignId = "";
        private string _slot = "";
        private string _fingerprint = "";
        private bool _loaded;
        private bool _dirty;

        internal PersistentStateStore(string root)
        {
            if (string.IsNullOrWhiteSpace(root))
                throw new ArgumentException("持久变量目录不能为空");
            _root = Path.GetFullPath(root);
        }

        internal int Get(ModPackage package, string slot, string key)
        {
            Bind(package, slot);
            RequirePublicKey(key);
            int value;
            return _values.TryGetValue(key, out value) ? value : 0;
        }

        internal void Set(ModPackage package, string slot, string key, int value)
        {
            Bind(package, slot);
            RequirePublicKey(key);
            if (!_values.ContainsKey(key) && _values.Count >= MaxEntries)
                throw new InvalidOperationException("单个 MOD 隔离槽最多允许 256 个持久变量");
            _values[key] = value;
            _dirty = true;
        }

        internal int Add(ModPackage package, string slot, string key, int delta)
        {
            int current = Get(package, slot, key);
            int result = checked(current + delta);
            Set(package, slot, key, result);
            return result;
        }

        internal void BeginNewCampaign(ModPackage package, string slot)
        {
            ValidateIdentity(package, slot);
            _modId = package.Id;
            _campaignId = package.CampaignId;
            _slot = slot;
            _fingerprint = package.PackageFingerprint;
            _values.Clear();
            _loaded = true;
            _dirty = true;
        }

        internal void Flush()
        {
            if (!_loaded || !_dirty) return;
            Directory.CreateDirectory(_root);
            string target = StatePath(_slot);
            string temporary = target + ".tmp." + Guid.NewGuid().ToString("N");
            try
            {
                byte[] payload = new UTF8Encoding(false, true).GetBytes(Serialize());
                if (payload.Length > MaxBytes)
                    throw new InvalidDataException("持久变量 sidecar 超过 256 KiB 上限");
                using (var stream = new FileStream(
                    temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    stream.Write(payload, 0, payload.Length);
                    stream.Flush(true);
                }
                if (File.Exists(target))
                    File.Replace(temporary, target, null);
                else
                    File.Move(temporary, target);
                _dirty = false;
            }
            finally
            {
                if (File.Exists(temporary)) File.Delete(temporary);
            }
        }

        internal void ResetMemory()
        {
            _modId = "";
            _campaignId = "";
            _slot = "";
            _fingerprint = "";
            _values.Clear();
            _loaded = false;
            _dirty = false;
        }

        internal IReadOnlyDictionary<string, int> Snapshot()
        {
            return new Dictionary<string, int>(_values, StringComparer.Ordinal);
        }

        private void Bind(ModPackage package, string slot)
        {
            ValidateIdentity(package, slot);
            if (_loaded && string.Equals(_modId, package.Id, StringComparison.Ordinal)
                && string.Equals(_slot, slot, StringComparison.Ordinal))
            {
                _fingerprint = package.PackageFingerprint;
                return;
            }
            ResetMemory();
            _modId = package.Id;
            _campaignId = package.CampaignId;
            _slot = slot;
            _fingerprint = package.PackageFingerprint;
            Load();
            _loaded = true;
        }

        private void Load()
        {
            string path = StatePath(_slot);
            if (!File.Exists(path)) return;
            var info = new FileInfo(path);
            if (info.Length > MaxBytes)
                throw new InvalidDataException("持久变量 sidecar 超过 256 KiB，已拒绝读取");
            object parsed = MiniJson.Parse(File.ReadAllText(path, new UTF8Encoding(false, true)));
            var root = parsed as Dictionary<string, object>;
            if (root == null || ReadInt(root, "format") != Format
                || ReadString(root, "mod_id") != _modId
                || ReadString(root, "campaign_id") != _campaignId
                || ReadString(root, "slot") != _slot)
                throw new InvalidDataException("持久变量 sidecar 身份或格式不匹配");
            object rawValues;
            var values = root.TryGetValue("values", out rawValues)
                ? rawValues as Dictionary<string, object> : null;
            if (values == null || values.Count > MaxEntries)
                throw new InvalidDataException("持久变量 sidecar 的 values 无效或超过 256 项");
            var loaded = new Dictionary<string, int>(StringComparer.Ordinal);
            foreach (var pair in values)
            {
                RequirePublicKey(pair.Key);
                loaded[pair.Key] = ToInt32(pair.Value, "values." + pair.Key);
            }
            foreach (var pair in loaded) _values[pair.Key] = pair.Value;
        }

        private string Serialize()
        {
            var sb = new StringBuilder();
            sb.Append("{\n  \"format\": 1,\n  \"mod_id\": ").Append(Quote(_modId))
              .Append(",\n  \"slot\": ").Append(Quote(_slot))
              .Append(",\n  \"campaign_id\": ").Append(Quote(_campaignId))
              .Append(",\n  \"package_fingerprint\": ").Append(Quote(_fingerprint))
              .Append(",\n  \"values\": {");
            bool first = true;
            foreach (var key in _values.Keys.OrderBy(value => value, StringComparer.Ordinal))
            {
                sb.Append(first ? "\n" : ",\n");
                first = false;
                sb.Append("    ").Append(Quote(key)).Append(": ")
                  .Append(_values[key].ToString(CultureInfo.InvariantCulture));
            }
            if (!first) sb.Append('\n');
            sb.Append("  }\n}\n");
            return sb.ToString();
        }

        private string StatePath(string slot)
        {
            string path = Path.GetFullPath(Path.Combine(
                _root, slot + ".state.json"));
            string prefix = _root.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)
                + Path.DirectorySeparatorChar;
            if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("持久变量路径越出 Host 数据目录");
            return path;
        }

        private static void ValidateIdentity(ModPackage package, string slot)
        {
            if (package == null || string.IsNullOrEmpty(package.Id)
                || !ModId.IsMatch(package.Id)
                || !CampaignIdentity.IsValid(package.CampaignId)
                || string.IsNullOrEmpty(package.PackageFingerprint)
                || !Sha256.IsMatch(package.PackageFingerprint))
                throw new InvalidOperationException("持久变量缺少可信的包 id / 完整 SHA-256 身份");
            if (!CampaignIdentity.OwnsSlot(package.CampaignId, slot))
                throw new InvalidOperationException(
                    "持久变量只允许写入当前 MOD 的隔离槽 "
                    + CampaignIdentity.SaveSlot(package.CampaignId));
        }

        private static void RequirePublicKey(string key)
        {
            if (string.IsNullOrEmpty(key) || !PublicKey.IsMatch(key))
                throw new ArgumentException("持久变量名必须是 1~64 位字母、数字、下划线或短横线");
        }

        private static int ReadInt(Dictionary<string, object> dict, string key)
        {
            object value;
            if (!dict.TryGetValue(key, out value))
                throw new InvalidDataException("持久变量 sidecar 缺少 " + key);
            return ToInt32(value, key);
        }

        private static int ToInt32(object value, string label)
        {
            if (!(value is double))
                throw new InvalidDataException(label + " 必须是整数");
            double number = (double)value;
            if (double.IsNaN(number) || double.IsInfinity(number)
                || number < int.MinValue || number > int.MaxValue || Math.Truncate(number) != number)
                throw new InvalidDataException(label + " 必须是 Int32 范围内整数");
            return (int)number;
        }

        private static string ReadString(Dictionary<string, object> dict, string key)
        {
            object value;
            string text;
            if (!dict.TryGetValue(key, out value) || (text = value as string) == null)
                throw new InvalidDataException("持久变量 sidecar 的 " + key + " 必须是字符串");
            return text;
        }

        private static string Quote(string value)
        {
            var sb = new StringBuilder("\"");
            foreach (char c in value ?? "")
            {
                switch (c)
                {
                    case '\\': sb.Append("\\\\"); break;
                    case '"': sb.Append("\\\""); break;
                    case '\b': sb.Append("\\b"); break;
                    case '\f': sb.Append("\\f"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("X4"));
                        else sb.Append(c);
                        break;
                }
            }
            return sb.Append('"').ToString();
        }
    }
}
