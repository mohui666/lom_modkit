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
        private readonly Dictionary<string, string> _characterTitles =
            new Dictionary<string, string>(StringComparer.Ordinal);
        private readonly Dictionary<string, string> _characterNames =
            new Dictionary<string, string>(StringComparer.Ordinal);
        private readonly Dictionary<string, int> _characterAffinities =
            new Dictionary<string, int>(StringComparer.Ordinal);
        private sealed class ProfileState {internal bool? Known;internal int Unlocked,Locked;}
        private readonly Dictionary<string,ProfileState> _characterProfiles =
            new Dictionary<string,ProfileState>(StringComparer.Ordinal);
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

        internal void Open(ModPackage package, string slot)
        {
            Bind(package, slot);
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

        internal bool TryGetCharacterTitle(ModPackage package, string slot, string character, out string title)
        {
            Bind(package, slot);
            return _characterTitles.TryGetValue(character, out title);
        }

        internal void SetCharacterTitle(ModPackage package, string slot, string character, string title)
        {
            Bind(package, slot);
            if (string.IsNullOrEmpty(character) || character.Length > 128)
                throw new ArgumentException("称号人物标识无效");
            if (title != null && (title.Length > 80 || title.IndexOfAny(new[] { '\r', '\n' }) >= 0))
                throw new ArgumentException("人物称号需要不超过80字符的单行文字");
            if (title == null) _characterTitles.Remove(character);
            else _characterTitles[character] = title;
            _dirty = true;
        }

        internal bool TryGetCharacterName(ModPackage package, string slot, string character, out string name)
        {
            Bind(package, slot);
            return _characterNames.TryGetValue(character, out name);
        }

        internal void SetCharacterName(ModPackage package, string slot, string character, string name)
        {
            Bind(package, slot);
            if (string.IsNullOrEmpty(character) || character.Length > 128)
                throw new ArgumentException("姓名人物标识无效");
            if (name != null && (string.IsNullOrWhiteSpace(name) || name.Length > 80 || name.IndexOfAny(new[] { '\r', '\n' }) >= 0))
                throw new ArgumentException("人物姓名需要1～80字符的单行文字");
            if (name == null) _characterNames.Remove(character);
            else _characterNames[character] = name;
            _dirty = true;
        }

        internal int CharacterAffinity(ModPackage package, string slot, string character, int initial)
        {
            Bind(package, slot);
            int value;
            return _characterAffinities.TryGetValue(character, out value) ? value : initial;
        }

        internal void SetCharacterAffinity(ModPackage package, string slot, string character, int value)
        {
            Bind(package, slot);
            if (string.IsNullOrEmpty(character) || !character.StartsWith("user:", StringComparison.Ordinal)
                || character.Length > 128 || value < 0 || value > 100)
                throw new ArgumentException("自定义人物好感标识或数值无效（0～100）");
            _characterAffinities[character] = value;
            _dirty = true;
        }

        internal bool CharacterKnown(ModPackage package,string slot,string character,bool initial)
        {
            Bind(package,slot);
            ProfileState state;
            return _characterProfiles.TryGetValue(character,out state) && state.Known.HasValue ? state.Known.Value : initial;
        }
        // The social panel reads these together every frame. Bind the owning
        // slot once rather than once for affinity and each biography page.
        internal void ReadCharacterProfile(ModPackage package,string slot,string character,int initialAffinity,bool initiallyKnown,
            out int affinity,out bool known,out int unlocked,out int locked)
        {
            Bind(package,slot);
            if(!_characterAffinities.TryGetValue(character,out affinity))affinity=initialAffinity;
            ProfileState state;
            bool exists=_characterProfiles.TryGetValue(character,out state);
            known=exists && state.Known.HasValue ? state.Known.Value : initiallyKnown;
            unlocked=exists ? state.Unlocked : 0;
            locked=exists ? state.Locked : 0;
        }
        internal void SetCharacterKnown(ModPackage package,string slot,string character,bool known)
        {
            Bind(package,slot);
            Profile(character).Known=known;
            _dirty=true;
        }
        internal int BiographyOverride(ModPackage package,string slot,string character,int page)
        {
            Bind(package,slot);
            int bit=CharacterProfilePolicy.PageBit(page);
            ProfileState state;
            if(!_characterProfiles.TryGetValue(character,out state))return 0;
            return (state.Locked&bit)!=0 ? -1 : (state.Unlocked&bit)!=0 ? 1 : 0;
        }
        internal void SetBiographyOverride(ModPackage package,string slot,string character,int page,bool unlocked)
        {
            Bind(package,slot);
            int bit=CharacterProfilePolicy.PageBit(page);
            ProfileState state=Profile(character);
            state.Unlocked=unlocked ? state.Unlocked|bit : state.Unlocked&~bit;
            state.Locked=unlocked ? state.Locked&~bit : state.Locked|bit;
            _dirty=true;
        }
        private ProfileState Profile(string character)
        {
            ContentRef reference;string error;
            if(!ContentRef.TryParse(character,out reference,out error) || !ContentRef.IsUserRef(character))
                throw new ArgumentException("人物资料状态需要合法的 user: 自定义人物标识");
            ProfileState state;
            if(!_characterProfiles.TryGetValue(character,out state)) {
                state=new ProfileState();_characterProfiles.Add(character,state);
            }
            return state;
        }

        internal void BeginNewCampaign(ModPackage package, string slot)
        {
            ValidateIdentity(package, slot);
            _modId = package.Id;
            _campaignId = package.CampaignId;
            _slot = slot;
            _fingerprint = package.PackageFingerprint;
            _values.Clear();
            _characterTitles.Clear();
            _characterNames.Clear();
            _characterAffinities.Clear();
            _characterProfiles.Clear();
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
            _characterTitles.Clear();
            _characterNames.Clear();
            _characterAffinities.Clear();
            _characterProfiles.Clear();
            _loaded = false;
            _dirty = false;
        }

        // Save-as and autosave must capture live values for the actual destination,
        // without advancing the sidecar belonging to an older manual save.
        internal void SaveToSlot(ModPackage package, string slot)
        {
            ValidateIdentity(package, slot);
            if (!_loaded) return;
            if (_modId != package.Id || _campaignId != package.CampaignId)
                throw new InvalidOperationException("持久变量另存的包身份不匹配");
            if (_slot == slot) { Flush(); return; }
            string sourceSlot = _slot;
            bool sourceDirty = _dirty;
            try { _slot = slot; _dirty = true; Flush(); }
            finally { _slot = sourceSlot; _dirty = sourceDirty; }
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
            object rawTitles;
            if (root.TryGetValue("character_titles", out rawTitles))
            {
                var titles = rawTitles as Dictionary<string, object>;
                if (titles == null) throw new InvalidDataException("人物称号存档数据必须是对象");
                foreach (var pair in titles)
                {
                    string title = pair.Value as string;
                    if (string.IsNullOrEmpty(pair.Key) || pair.Key.Length > 128 || title == null
                        || title.Length > 80 || title.IndexOfAny(new[] { '\r', '\n' }) >= 0)
                        throw new InvalidDataException("人物称号存档字段无效：" + pair.Key);
                    _characterTitles.Add(pair.Key, title);
                }
            }
            object rawNames;
            if (root.TryGetValue("character_names", out rawNames))
            {
                var names = rawNames as Dictionary<string, object>;
                if (names == null) throw new InvalidDataException("人物姓名存档数据必须是对象");
                foreach (var pair in names)
                {
                    string name = pair.Value as string;
                    if (string.IsNullOrEmpty(pair.Key) || pair.Key.Length > 128 || string.IsNullOrWhiteSpace(name)
                        || name.Length > 80 || name.IndexOfAny(new[] { '\r', '\n' }) >= 0)
                        throw new InvalidDataException("人物姓名存档字段无效：" + pair.Key);
                    _characterNames.Add(pair.Key, name);
                }
            }
            object rawAffinities;
            if (root.TryGetValue("character_affinities", out rawAffinities))
            {
                var affinities = rawAffinities as Dictionary<string, object>;
                if (affinities == null) throw new InvalidDataException("人物好感存档数据必须是对象");
                foreach (var pair in affinities)
                {
                    int value = ToInt32(pair.Value, "character_affinities." + pair.Key);
                    if (!pair.Key.StartsWith("user:", StringComparison.Ordinal) || pair.Key.Length > 128 || value < 0 || value > 100)
                        throw new InvalidDataException("人物好感存档字段无效：" + pair.Key);
                    _characterAffinities.Add(pair.Key, value);
                }
            }
            object rawProfiles;
            if(root.TryGetValue("character_profiles",out rawProfiles)) {
                var profiles=rawProfiles as Dictionary<string,object>;
                if(profiles==null)throw new InvalidDataException("人物资料存档数据必须是对象");
                foreach(var pair in profiles) {
                    var data=pair.Value as Dictionary<string,object>;
                    if(data==null || data.Keys.Any(key=>key!="known" && key!="unlocked" && key!="locked"))
                        throw new InvalidDataException("人物资料存档字段无效："+pair.Key);
                    ProfileState state=Profile(pair.Key);
                    object known;
                    if(data.TryGetValue("known",out known)) {
                        if(!(known is bool))throw new InvalidDataException("人物认识状态必须是布尔值");
                        state.Known=(bool)known;
                    }
                    state.Unlocked=ReadInt(data,"unlocked");state.Locked=ReadInt(data,"locked");
                    if(state.Unlocked<0 || state.Unlocked>15 || state.Locked<0 || state.Locked>15 || (state.Unlocked&state.Locked)!=0)
                        throw new InvalidDataException("人物传记解锁存档位无效："+pair.Key);
                }
            }
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
            sb.Append("  }");
            if (_characterTitles.Count > 0)
            {
                sb.Append(",\n  \"character_titles\": {");
                first = true;
                foreach (string key in _characterTitles.Keys.OrderBy(value => value, StringComparer.Ordinal))
                {
                    sb.Append(first ? "\n" : ",\n"); first = false;
                    sb.Append("    ").Append(Quote(key)).Append(": ").Append(Quote(_characterTitles[key]));
                }
                sb.Append("\n  }");
            }
            if (_characterNames.Count > 0)
            {
                sb.Append(",\n  \"character_names\": {");
                first = true;
                foreach (string key in _characterNames.Keys.OrderBy(value => value, StringComparer.Ordinal))
                {
                    sb.Append(first ? "\n" : ",\n"); first = false;
                    sb.Append("    ").Append(Quote(key)).Append(": ").Append(Quote(_characterNames[key]));
                }
                sb.Append("\n  }");
            }
            if (_characterAffinities.Count > 0)
            {
                sb.Append(",\n  \"character_affinities\": {");
                first = true;
                foreach (string key in _characterAffinities.Keys.OrderBy(value => value, StringComparer.Ordinal))
                {
                    sb.Append(first ? "\n" : ",\n"); first = false;
                    sb.Append("    ").Append(Quote(key)).Append(": ")
                        .Append(_characterAffinities[key].ToString(CultureInfo.InvariantCulture));
                }
                sb.Append("\n  }");
            }
            if(_characterProfiles.Count>0) {
                sb.Append(",\n  \"character_profiles\": {");first=true;
                foreach(string key in _characterProfiles.Keys.OrderBy(value=>value,StringComparer.Ordinal)) {
                    ProfileState state=_characterProfiles[key];
                    sb.Append(first ? "\n" : ",\n");first=false;
                    sb.Append("    ").Append(Quote(key)).Append(": {");
                    if(state.Known.HasValue)sb.Append("\"known\": ").Append(state.Known.Value ? "true, " : "false, ");
                    sb.Append("\"unlocked\": ").Append(state.Unlocked).Append(", \"locked\": ").Append(state.Locked).Append('}');
                }
                sb.Append("\n  }");
            }
            sb.Append("\n}\n");
            return sb.ToString();
        }

        private string StatePath(string slot)
        {
            string path = Path.GetFullPath(Path.Combine(
                _root, slot + (ModSaveSlotPolicy.IsAddonSlot(slot) ? "." + _modId : "") + ".state.json"));
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
            if (!(package.PlayMode == "addon" && ModSaveSlotPolicy.IsAddonSlot(slot))
                && !CampaignIdentity.OwnsSlot(package.CampaignId, slot))
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
