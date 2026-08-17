using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// mod 包扫描与解析（纯静态、无 BepInEx/Unity 依赖，便于离线单测）。
    /// 行为契约见 docs/chs/mod_format.md §6：扫描 mods/*.lommod，解出 manifest.json、lua/*.lua、可选 texts.json（契约 §1）
    /// 与 assets/ 下图片（契约 §3.1）。单个包损坏只警告跳过，绝不抛出让插件崩溃。
    /// </summary>
    internal static class ModLoader
    {
        internal const int PackageFormat = 3;
        internal const int StorySchema = 2;
        internal const int ContentSchema = 1;

        /// <summary>结局卡背景图单张上限（字节，契约 §3.1，与编译器 pack 校验一致）。</summary>
        private const long MaxEndingImageBytes = 8L * 1024 * 1024;

        /// <summary>用户音频单条上限（与 compiler/lomc/content.py 一致）。</summary>
        private const long MaxAudioBytes = ContentRef.MaxAudioBytes;

        /// <summary>包结构防护：避免畸形 zip 以海量条目或超高解压比耗尽内存。</summary>
        private const int MaxArchiveEntries = 2048;
        private const long MaxEntryBytes = 32L * 1024 * 1024;
        private const long MaxArchiveBytes = 128L * 1024 * 1024;
        private const long MaxTextBytes = 4L * 1024 * 1024;
        private const long MaxPackageFileBytes = 160L * 1024 * 1024;
        private const int MaxIdentifierLength = 64;

        internal const string PackageContentHashEntry = "package-content.sha256";
        internal const string PackageContentHashAlgorithm = "lom-entry-sha256-v1";
        internal const string StoryLuaHashEntry = "story-lua.sha256";
        internal const string StoryLuaHashAlgorithm = "lom-story-lua-sha256-v1";

        private const string PreviewPackageId = "lom_modkit_preview";
        private const string PreviewPackageFileName = "__lom_modkit_preview.lommod";

        /// <summary>结局卡背景图允许的扩展名（契约 §3.1）。</summary>
        private static bool IsImageAsset(string path)
        {
            return path.EndsWith(".png", StringComparison.OrdinalIgnoreCase)
                || path.EndsWith(".jpg", StringComparison.OrdinalIgnoreCase)
                || path.EndsWith(".jpeg", StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>统一包内路径分隔符为正斜杠（与编译器打包/lua 传参一致）。</summary>
        private static string NormalizeAssetPath(string path)
        {
            return path.Replace('\\', '/');
        }

        /// <summary>
        /// 扫描 modsDir 下全部 *.lommod。目录不存在则创建。
        /// 返回成功解析的包列表；坏包经 logWarn 报告后跳过。
        /// </summary>
        public static List<ModPackage> ScanMods(string modsDir, Action<string> logInfo, Action<string> logWarn)
        {
            var result = new List<ModPackage>();
            var campaignOwners = new Dictionary<string, string>(StringComparer.Ordinal);
            if (!Directory.Exists(modsDir))
            {
                Directory.CreateDirectory(modsDir);
                if (logInfo != null) logInfo("mods 目录不存在，已创建：" + modsDir);
                return result;
            }

            string[] files = Directory.GetFiles(modsDir, "*.lommod");
            Array.Sort(files, StringComparer.OrdinalIgnoreCase); // 按文件名排序，加载顺序稳定
            var parsedPackages = new List<ModPackage>();
            foreach (string file in files)
            {
                try
                {
                    parsedPackages.Add(LoadPackage(file, logWarn));
                }
                catch (Exception ex)
                {
                    // 坏 zip / 缺 manifest / 缺 entry lua / JSON 非法等都走这里
                    if (logWarn != null)
                        logWarn("跳过损坏的 mod 包 " + Path.GetFileName(file) + "：" + ex.Message);
                }
            }

            // 编辑器的正式试玩包使用固定文件名和固定 manifest id。旧版本曾写出
            // lom_modkit_preview.lommod；若两者并存，绝不能让目录枚举/排序决定本次
            // 试玩加载哪一份。先找出已完整通过包校验的正式试玩包，再忽略所有占用
            // 同一 id 或 campaign_id 的旧残留。正式包自身损坏时不会进入此列表，旧包
            // 仍可正常加载，避免一次写包失败把最后可用的试玩也一并屏蔽。
            ModPackage preferredPreview = null;
            foreach (ModPackage package in parsedPackages)
            {
                if (IsPreferredPreviewPackage(package))
                {
                    preferredPreview = package;
                    break;
                }
            }

            foreach (ModPackage package in parsedPackages)
            {
                if (preferredPreview != null && !ReferenceEquals(package, preferredPreview)
                    && (string.Equals(package.Id, preferredPreview.Id, StringComparison.Ordinal)
                        || string.Equals(package.CampaignId, preferredPreview.CampaignId, StringComparison.Ordinal)))
                {
                    if (logWarn != null)
                        logWarn("已优先加载编辑器正式试玩包 " + PreviewPackageFileName
                            + "，忽略旧预览残留 " + Path.GetFileName(package.PackagePath));
                    continue;
                }

                string existingOwner;
                if (campaignOwners.TryGetValue(package.CampaignId, out existingOwner))
                {
                    if (logWarn != null)
                        logWarn("跳过 campaign_id 冲突的 mod 包 "
                            + Path.GetFileName(package.PackagePath) + "：与已加载 MOD "
                            + existingOwner + " 冲突：" + package.CampaignId);
                    continue;
                }
                campaignOwners.Add(package.CampaignId, package.Id);
                result.Add(package);
            }
            return result;
        }

        private static bool IsPreferredPreviewPackage(ModPackage package)
        {
            return package != null
                && string.Equals(package.Id, PreviewPackageId, StringComparison.Ordinal)
                && string.Equals(package.CampaignId, PreviewPackageId, StringComparison.Ordinal)
                && string.Equals(Path.GetFileName(package.PackagePath), PreviewPackageFileName,
                    StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>
        /// 解析单个 .lommod（zip 全程只走内存流，不解到磁盘）。校验失败抛异常，由调用方兜底。
        /// </summary>
        private static ModPackage LoadPackage(string path, Action<string> logWarn)
        {
            using (var stream = File.OpenRead(path))
            {
                if (stream.Length > MaxPackageFileBytes)
                    throw new FormatException("mod 包文件超过 160MB 上限");
                string fingerprint;
                using (var sha256 = SHA256.Create())
                    fingerprint = ToUpperHex(sha256.ComputeHash(stream));
                stream.Position = 0;

                using (var zip = new ZipArchive(stream, ZipArchiveMode.Read))
                {
                    ValidateArchiveLimits(zip);
                    var manifestEntry = zip.GetEntry("manifest.json");
                    if (manifestEntry == null)
                        throw new FormatException("包内缺少 manifest.json");

                    var package = ParseManifest(ReadEntryText(manifestEntry));
                    package.PackagePath = path;
                    package.PackageFingerprint = fingerprint;

                    ValidatePackageContentHash(zip, true);
                    ValidateStoryLuaIntegrity(zip, true);

                // 收集 lua/<id>.lua（仅 lua/ 直接子项，契约 §1）
                foreach (var entry in zip.Entries)
                {
                    if (!entry.FullName.StartsWith("lua/", StringComparison.Ordinal)) continue;
                    string rest = entry.FullName.Substring(4);
                    if (rest.Length == 0 || rest.IndexOf('/') >= 0) continue;       // 跳过子目录
                    if (!rest.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)) continue;
                    string scriptId = rest.Substring(0, rest.Length - 4);
                    if (scriptId.Length == 0) continue;
                    if (!IsValidScriptId(scriptId))
                        throw new FormatException("lua 脚本 id 非法：" + scriptId);
                    package.LuaScripts[scriptId] = ReadEntryText(entry);
                }

                if (package.LuaScripts.Count == 0)
                    throw new FormatException("包内 lua/ 目录没有任何 .lua 脚本");
                if (!package.LuaScripts.ContainsKey(package.Entry))
                    throw new FormatException("入口脚本 lua/" + package.Entry + ".lua 不存在");

                // texts.json（可选，契约 §1）：MOD_<modid>_<scriptid>_<nodeid> → 台词文本。
                // 老包没有该文件合法；存在但解析失败只警告跳过（不拖垮整包）。
                var textsEntry = zip.GetEntry("texts.json");
                if (textsEntry != null)
                {
                    try
                    {
                        ParseTexts(package, ReadEntryText(textsEntry));
                    }
                    catch (Exception ex)
                    {
                        if (logWarn != null)
                            logWarn("mod " + package.Id + " 的 texts.json 解析失败，已忽略：" + ex.Message);
                    }
                }

                // Story Localization v1. Invalid optional metadata falls back to the
                // legacy default scripts/texts without rejecting an otherwise valid mod.
                var localizationEntry = zip.GetEntry("localization.json");
                if (localizationEntry != null)
                {
                    try
                    {
                        ParseLocalization(package, ReadEntryText(localizationEntry));
                        LoadLocalizedScriptsAndTexts(package, zip);
                    }
                    catch (Exception ex)
                    {
                        package.LocalizedLuaScripts.Clear();
                        package.LocalizedTexts.Clear();
                        package.DefaultLocale = "chs";
                        package.FallbackLocale = "chs";
                        if (logWarn != null)
                            logWarn("mod " + package.Id + " 的 localization.json/locale 资源无效，已回退默认语言：" + ex.Message);
                    }
                }

                // assets/ 下的图片（契约 §3.1 结局卡背景图）：只收 .png/.jpg/.jpeg，
                // 单张 ≤8MB（超限警告跳过）。用户音频走 assets/user/，见 LoadUserContents。
                foreach (var entry in zip.Entries)
                {
                    if (!entry.FullName.StartsWith("assets/", StringComparison.Ordinal)) continue;
                    if (!IsImageAsset(entry.FullName)) continue;
                    if (entry.Length > MaxEndingImageBytes)
                    {
                        if (logWarn != null)
                            logWarn("mod " + package.Id + " 的资源 " + entry.FullName + " 超过 8MB，已忽略（结局卡背景图上限）");
                        continue;
                    }
                    try
                    {
                        using (var input = entry.Open())
                        {
                            package.Assets[NormalizeAssetPath(entry.FullName)] = ReadBoundedBytes(input, MaxEndingImageBytes, entry.FullName);
                        }
                    }
                    catch (Exception ex)
                    {
                        if (logWarn != null)
                            logWarn("mod " + package.Id + " 的资源 " + entry.FullName + " 读取失败，已忽略：" + ex.Message);
                    }
                }

                LoadUserContents(package, zip, logWarn);

                // 触发器 script 必须指向包内已有脚本；指向不存在脚本的触发器警告后丢弃（不拖垮整包）
                if (package.Campaign != null)
                {
                    for (int i = package.Campaign.Triggers.Count - 1; i >= 0; i--)
                    {
                        if (!package.LuaScripts.ContainsKey(package.Campaign.Triggers[i].Script))
                        {
                            if (logWarn != null)
                                logWarn("mod " + package.Id + " 的触发器脚本 " + package.Campaign.Triggers[i].Script + " 不存在，已丢弃该触发器");
                            package.Campaign.Triggers.RemoveAt(i);
                        }
                    }
                }

                    return package;
                }
            }
        }

        private static string ToUpperHex(byte[] bytes)
        {
            var text = new StringBuilder(bytes.Length * 2);
            for (int i = 0; i < bytes.Length; i++)
                text.Append(bytes[i].ToString("X2"));
            return text.ToString();
        }

        /// <summary>解析 manifest.json，校验契约 §2 的必填字段。</summary>
        private static ModPackage ParseManifest(string json)
        {
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null)
                throw new FormatException("manifest.json 顶层必须是 JSON 对象");

            // 1.0.1 起只接受 v3。v1/v2 没有稳定 campaign_id，禁止用 manifest.id
            // 猜测或迁移存档身份，否则同名/改名包会误读另一战役的存档。
            int packageFormat = RequireVersion(root, "package_format", PackageFormat, null, false);
            RequireVersion(root, "story_schema", StorySchema, null, false);
            RequireVersion(root, "content_schema", ContentSchema, null, false);

            string id = GetString(root, "id", required: true);
            string campaignId = GetString(root, "campaign_id", required: true);
            string entry = GetString(root, "entry", required: true);
            if (!IsValidModId(id))
                throw new FormatException("manifest.id 必须匹配 [a-z0-9_-]{1,64}");
            if (!IsValidScriptId(entry))
                throw new FormatException("manifest.entry 必须匹配 [A-Za-z0-9_-]{1,64}");
            if (!CampaignIdentity.IsValid(campaignId))
                throw new FormatException("manifest.campaign_id 必须匹配 [a-z0-9_-]{1,64}");

            var package = new ModPackage
            {
                Id = id,
                CampaignId = campaignId,
                PackageFormatVersion = packageFormat,
                Name = GetString(root, "name", required: false) ?? "",
                Version = GetString(root, "version", required: false) ?? "",
                MinHostVersion = GetOptionalCompatibilityString(root, "min_host_version"),
                TestedHostVersion = GetOptionalCompatibilityString(root, "tested_host_version"),
                GameVersion = GetOptionalCompatibilityString(root, "game_version"),
                TestedGameVersion = GetOptionalCompatibilityString(root, "tested_game_version"),
                Author = GetString(root, "author", required: false) ?? "",
                Description = GetString(root, "description", required: false) ?? "",
                Entry = entry,
                Campaign = ParseCampaign(root, campaignId)
            };
            return package;
        }

        private static int RequireVersion(
            Dictionary<string, object> root,
            string field,
            int current,
            string legacyField,
            bool allowMissing,
            bool allowPrevious = false)
        {
            object valueObj;
            bool hasExplicit = root.TryGetValue(field, out valueObj);
            object legacyObj = null;
            bool hasLegacy = legacyField != null && root.TryGetValue(legacyField, out legacyObj);
            if (!hasExplicit)
                valueObj = hasLegacy ? legacyObj : null;
            if (!hasExplicit && !hasLegacy && allowMissing)
                return current;
            if (!(valueObj is double)
                || ((double)valueObj != current && !(allowPrevious && (double)valueObj == current - 1)))
                throw new FormatException(
                    "manifest." + field + " 不是受支持版本 "
                    + (allowPrevious ? (current - 1) + " 或 " : "") + current
                    + "（本插件不支持该版本）");
            if (hasExplicit && hasLegacy
                && (!(legacyObj is double) || (double)legacyObj != (double)valueObj))
                throw new FormatException(
                    "manifest." + field + " 与旧字段 " + legacyField + " 不一致");
            return (int)(double)valueObj;
        }

        private static bool IsValidModId(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > MaxIdentifierLength) return false;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_' || c == '-'))
                    return false;
            }
            return true;
        }

        private static bool IsValidScriptId(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > MaxIdentifierLength) return false;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
                    || (c >= '0' && c <= '9') || c == '_' || c == '-'))
                    return false;
            }
            return true;
        }

        /// <summary>解析 v3 必填的 campaign 段；每包恰好一个稳定 campaign_id。</summary>
        private static CampaignConfig ParseCampaign(Dictionary<string, object> root, string campaignId)
        {
            object campaignObj;
            if (!root.TryGetValue("campaign", out campaignObj) || campaignObj == null)
                throw new FormatException("manifest 缺少必填 campaign 对象；该包不是 v3 自定义战役");
            var dict = campaignObj as Dictionary<string, object>;
            if (dict == null)
                throw new FormatException("manifest.campaign 必须是对象");

            var campaign = new CampaignConfig { Id = campaignId };
            object newGameObj;
            if (!dict.TryGetValue("new_game", out newGameObj) || !(newGameObj is bool)
                || !(bool)newGameObj)
                throw new FormatException("manifest.campaign.new_game 在 v3 中必须为 true");
            campaign.NewGame = true;

            // 契约 §2：disable_official_events（可选布尔，缺省 false）：该 mod 战役期间返回
            // Free 的自动任务与位置点击不再触发官方故事，只允许 mod 自己的位置触发器命中。
            object disableObj;
            if (dict.TryGetValue("disable_official_events", out disableObj) && disableObj != null)
            {
                if (!(disableObj is bool))
                    throw new FormatException("manifest.campaign.disable_official_events 必须是布尔值");
                campaign.DisableOfficialEvents = (bool)disableObj;
            }

            object triggersObj;
            if (dict.TryGetValue("triggers", out triggersObj) && triggersObj != null)
            {
                var list = triggersObj as List<object>;
                if (list == null)
                    throw new FormatException("manifest.campaign.triggers 必须是数组");
                foreach (object item in list)
                {
                    var triggerDict = item as Dictionary<string, object>;
                    if (triggerDict == null)
                        throw new FormatException("manifest.campaign.triggers 元素必须是对象");
                    string type = GetString(triggerDict, "type", required: true);
                    if (type != "position")
                        throw new FormatException("不支持的 trigger type：" + type);
                    campaign.Triggers.Add(new CampaignTrigger
                    {
                        Position = GetString(triggerDict, "position", required: true),
                        Script = GetString(triggerDict, "script", required: true),
                        WhenFlagSet = GetString(triggerDict, "when_flag_set", required: false),
                        WhenFlagClear = GetString(triggerDict, "when_flag_clear", required: false),
                        WhenMonth = GetOptionalInt(triggerDict, "when_month", 1, 12),
                        WhenStage = GetOptionalInt(triggerDict, "when_stage", 1, 3),
                        WhenAffinity = ParseAffinity(triggerDict)
                    });
                }
            }
            return campaign;
        }

        /// <summary>
        /// 可选整数条件字段（契约 §2）：缺省返回 null；值必须是整数且在 [min,max] 内，
        /// 否则按 manifest 结构错误拒绝整包（与 when_flag_set 类型错误的行为一致）。
        /// </summary>
        private static int? GetOptionalInt(Dictionary<string, object> dict, string key, int min, int max)
        {
            object value;
            if (!dict.TryGetValue(key, out value) || value == null)
                return null;
            if (!(value is double) || (double)value % 1 != 0)
                throw new FormatException("manifest 字段 \"" + key + "\" 必须是整数");
            int n = (int)(double)value;
            if (n < min || n > max)
                throw new FormatException("manifest 字段 \"" + key + "\" 必须在 " + min + "~" + max + " 之间");
            return n;
        }

        /// <summary>
        /// 解析 when_affinity（契约 §2）：{ character: string, min: int }，缺省返回 null。
        /// character 用 RelationshipStatType 的 StringValue 契约 id（如 brother4），运行时解析。
        /// </summary>
        private static AffinityCondition ParseAffinity(Dictionary<string, object> dict)
        {
            object affinityObj;
            if (!dict.TryGetValue("when_affinity", out affinityObj) || affinityObj == null)
                return null;
            var affinity = affinityObj as Dictionary<string, object>;
            if (affinity == null)
                throw new FormatException("manifest 触发器 when_affinity 必须是对象");
            object minObj;
            double min;
            if (!affinity.TryGetValue("min", out minObj) || !(minObj is double)
                || double.IsNaN(min = (double)minObj) || double.IsInfinity(min)
                || min % 1 != 0 || min < int.MinValue || min > int.MaxValue)
                throw new FormatException("manifest 触发器 when_affinity.min 必须是 Int32 范围内的整数");
            return new AffinityCondition
            {
                Character = GetString(affinity, "character", required: true),
                Min = (int)min
            };
        }

        private static string GetString(Dictionary<string, object> dict, string key, bool required)
        {
            object value;
            if (!dict.TryGetValue(key, out value) || value == null)
            {
                if (required) throw new FormatException("manifest 缺少必填字段 \"" + key + "\"");
                return null;
            }
            var text = value as string;
            if (text == null)
                throw new FormatException("manifest 字段 \"" + key + "\" 必须是字符串");
            if (required && text.Length == 0)
                throw new FormatException("manifest 字段 \"" + key + "\" 不能为空");
            return text;
        }

        private static string GetOptionalCompatibilityString(
            Dictionary<string, object> dict, string key)
        {
            object value;
            if (!dict.TryGetValue(key, out value)) return null;
            string text = value as string;
            if (string.IsNullOrEmpty(text) || text.Length > 64)
                throw new FormatException("manifest 字段 \"" + key
                    + "\" 必须是 1~64 位版本字符串");
            return text;
        }

        /// <summary>解析 texts.json（契约 §1）：顶层对象，值必须全部是字符串。</summary>
        private static void ParseTexts(ModPackage package, string json)
        {
            ParseTextsInto(package.Texts, json, "texts.json");
        }

        private static void ParseTextsInto(Dictionary<string, string> target, string json, string source)
        {
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null)
                throw new FormatException(source + " 顶层必须是 JSON 对象");
            foreach (var pair in root)
            {
                var text = pair.Value as string;
                if (text == null)
                    throw new FormatException(source + " 键 " + pair.Key + " 的值必须是字符串");
                target[pair.Key] = text;
            }
        }

        private static bool IsStoryLocale(string locale)
        {
            return NormalizeStoryLocale(locale) != null;
        }

        private static string NormalizeStoryLocale(string locale)
        {
            if (locale == "chs" || locale == "cht" || locale == "ja" || locale == "ko") return locale;
            // Compatibility is input-only. New manifests and package paths always emit chs/cht.
            if (locale == "zh_CN" || locale == "zh-CN" || locale == "zh_Hans" || locale == "zh-Hans") return "chs";
            if (locale == "zh_TW" || locale == "zh-TW" || locale == "zh_Hant" || locale == "zh-Hant") return "cht";
            return null;
        }

        private static void ParseLocalization(ModPackage package, string json)
        {
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null) throw new FormatException("localization.json 顶层必须是对象");
            object schema;
            if (!root.TryGetValue("schema", out schema) || Convert.ToInt32(schema) != 1)
                throw new FormatException("localization.json schema 必须为 1");
            object defaultValue, fallbackValue;
            string defaultLocale = root.TryGetValue("default_locale", out defaultValue) ? defaultValue as string : null;
            string fallbackLocale = root.TryGetValue("fallback_locale", out fallbackValue) ? fallbackValue as string : defaultLocale;
            if (!IsStoryLocale(defaultLocale) || !IsStoryLocale(fallbackLocale))
                throw new FormatException("default_locale/fallback_locale 只支持 chs/cht/ja/ko");
            package.DefaultLocale = NormalizeStoryLocale(defaultLocale);
            package.FallbackLocale = NormalizeStoryLocale(fallbackLocale);
        }

        private static void LoadLocalizedScriptsAndTexts(ModPackage package, ZipArchive zip)
        {
            foreach (string locale in new[] { "chs", "cht", "ja", "ko" })
            {
                var scripts = new Dictionary<string, string>();
                string packageLocale = FindPackagedLocale(zip, locale);
                string prefix = "lua/" + packageLocale + "/";
                foreach (var entry in zip.Entries)
                {
                    if (!entry.FullName.StartsWith(prefix, StringComparison.Ordinal) ||
                        !entry.FullName.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)) continue;
                    string rest = entry.FullName.Substring(prefix.Length);
                    if (rest.Length <= 4 || rest.IndexOf('/') >= 0) continue;
                    string scriptId = rest.Substring(0, rest.Length - 4);
                    if (!package.LuaScripts.ContainsKey(scriptId))
                        throw new FormatException(entry.FullName + " 没有对应的默认 lua/" + scriptId + ".lua");
                    scripts[scriptId] = ReadEntryText(entry);
                }
                if (scripts.Count != package.LuaScripts.Count)
                    throw new FormatException(prefix + " 必须为每个默认脚本提供完整变体");
                package.LocalizedLuaScripts[locale] = scripts;
                var textsEntry = zip.GetEntry("texts/" + packageLocale + ".json");
                if (textsEntry == null)
                    throw new FormatException("缺少 texts/" + locale + ".json");
                var texts = new Dictionary<string, string>();
                ParseTextsInto(texts, ReadEntryText(textsEntry), textsEntry.FullName);
                if (texts.Count != package.Texts.Count)
                    throw new FormatException(textsEntry.FullName + " 的 key 必须与 texts.json 完全一致");
                foreach (string key in package.Texts.Keys)
                    if (!texts.ContainsKey(key))
                        throw new FormatException(textsEntry.FullName + " 缺少 key " + key);
                package.LocalizedTexts[locale] = texts;
            }
        }

        private static string FindPackagedLocale(ZipArchive zip, string locale)
        {
            if (zip.GetEntry("texts/" + locale + ".json") != null) return locale;
            if (locale == "chs" && zip.GetEntry("texts/zh_CN.json") != null) return "zh_CN";
            if (locale == "cht" && zip.GetEntry("texts/zh_TW.json") != null) return "zh_TW";
            return locale;
        }

        /// <summary>
        /// 读取 assets/user/&lt;type&gt;/&lt;id&gt;/content.json 与主文件。
        /// 路径不合法、metadata 损坏或文件缺失只警告跳过，不拖垮整包。
        /// </summary>
        private static void LoadUserContents(ModPackage package, ZipArchive zip, Action<string> logWarn)
        {
            var entries = new Dictionary<string, ZipArchiveEntry>(StringComparer.OrdinalIgnoreCase);
            foreach (var entry in zip.Entries)
            {
                string path = NormalizeAssetPath(entry.FullName);
                if (!path.StartsWith(ContentRef.PackageUserRoot + "/", StringComparison.Ordinal))
                    continue;
                if (path.EndsWith("/"))
                    continue;
                if (!ContentRef.IsSafePackageRelative(path))
                {
                    if (logWarn != null)
                        logWarn("mod " + package.Id + " 拒绝非法用户内容路径：" + path);
                    continue;
                }
                entries[path] = entry;
            }

            foreach (var pair in entries)
            {
                if (!pair.Key.EndsWith("/content.json", StringComparison.OrdinalIgnoreCase))
                    continue;
                try
                {
                    UserContent content = ParseUserContent(package, pair.Key, ReadEntryText(pair.Value), entries, logWarn);
                    if (content != null)
                    {
                        UserContent existing;
                        if (package.UserContents.TryGetValue(content.Id, out existing))
                        {
                            if (logWarn != null)
                                logWarn("mod " + package.Id + " 的用户内容 ID " + content.Id
                                    + " 重复（" + existing.Type + " / " + content.Type + "），已保留先加载项");
                            continue;
                        }
                        package.UserContents[content.Id] = content;
                    }
                }
                catch (Exception ex)
                {
                    if (logWarn != null)
                        logWarn("mod " + package.Id + " 的用户内容 " + pair.Key + " 解析失败，已忽略：" + ex.Message);
                }
            }
        }

        private static UserContent ParseUserContent(
            ModPackage package,
            string metaPath,
            string json,
            Dictionary<string, ZipArchiveEntry> entries,
            Action<string> logWarn)
        {
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null)
                throw new FormatException("content.json 顶层必须是对象");

            object schemaObj;
            bool hasExplicitSchema = root.TryGetValue("content_schema", out schemaObj);
            object legacySchemaObj = null;
            bool hasLegacySchema = root.TryGetValue("schema", out legacySchemaObj);
            if (!hasExplicitSchema)
                schemaObj = hasLegacySchema ? legacySchemaObj : null;
            if (!(schemaObj is double) || (double)schemaObj != ContentSchema)
                throw new FormatException("content.json content_schema 必须是 " + ContentSchema);
            if (hasExplicitSchema && hasLegacySchema
                && (!(legacySchemaObj is double) || (double)legacySchemaObj != (double)schemaObj))
                throw new FormatException("content.json content_schema 与旧字段 schema 不一致");

            string id = GetString(root, "id", required: true);
            string idError;
            if (!ContentRef.IsValidContentId(id, out idError))
                throw new FormatException(idError);

            string expectedDir = metaPath.Substring(0, metaPath.Length - "/content.json".Length);
            string expectedPrefix = ContentRef.PackageUserRoot + "/";
            if (!expectedDir.StartsWith(expectedPrefix, StringComparison.Ordinal))
                throw new FormatException("content.json 不在 assets/user/ 下");
            string rest = expectedDir.Substring(expectedPrefix.Length);
            int slash = rest.IndexOf('/');
            if (slash <= 0 || slash != rest.LastIndexOf('/'))
                throw new FormatException("用户内容目录结构必须是 assets/user/<type>/<id>/");
            string typeFromPath = rest.Substring(0, slash);
            string idFromPath = rest.Substring(slash + 1);
            if (!string.Equals(idFromPath, id, StringComparison.Ordinal))
                throw new FormatException("content.json 的 id 与目录名不一致");

            string type = GetString(root, "type", required: true);
            if (type != typeFromPath)
                throw new FormatException("content.json 的 type 与目录类型不一致");

            string name = GetString(root, "name", required: true);
            object filesObj;
            if (!root.TryGetValue("files", out filesObj))
                throw new FormatException("content.json 缺少 files");
            var files = filesObj as Dictionary<string, object>;
            if (files == null)
                throw new FormatException("content.json files 必须是对象");
            string mainFile = GetString(files, "main", required: true);
            if (mainFile.IndexOf('/') >= 0 || mainFile.IndexOf('\\') >= 0 || mainFile.IndexOf("..", StringComparison.Ordinal) >= 0)
                throw new FormatException("files.main 必须是同目录文件名");

            if (type == "audio")
                return ParseAudioContent(package, expectedDir, id, name, mainFile, root, entries);
            if (type == "character")
                return ParseCharacterContent(package, expectedDir, id, name, mainFile, root, entries);
            if (type == "image")
                return ParseImageContent(package, expectedDir, id, name, mainFile, entries);
            if (logWarn != null)
                logWarn("mod " + package.Id + " 的用户内容 " + id + " 类型 " + type + " 本版本不加载");
            return null;
        }

        private static UserContent ParseAudioContent(
            ModPackage package,
            string expectedDir,
            string id,
            string name,
            string mainFile,
            Dictionary<string, object> root,
            Dictionary<string, ZipArchiveEntry> entries)
        {
            string audioKind = GetString(root, "audio_kind", required: true);
            if (audioKind != "music" && audioKind != "sound" && audioKind != "env")
                throw new FormatException("audio_kind 必须是 music / sound / env");
            string ext = Path.GetExtension(mainFile);
            if (!ext.Equals(".ogg", StringComparison.OrdinalIgnoreCase)
                && !ext.Equals(".wav", StringComparison.OrdinalIgnoreCase))
                throw new FormatException("用户音频只支持 .ogg / .wav");

            string audioPath = expectedDir + "/" + mainFile;
            byte[] bytes = ReadZipBytes(package, entries, audioPath, MaxAudioBytes, "音频");
            return new UserContent
            {
                Id = id,
                Type = "audio",
                Name = name,
                AudioKind = audioKind,
                MainFile = mainFile,
                PackagePath = audioPath,
                Bytes = bytes
            };
        }

        private static UserContent ParseCharacterContent(
            ModPackage package,
            string expectedDir,
            string id,
            string name,
            string mainFile,
            Dictionary<string, object> root,
            Dictionary<string, ZipArchiveEntry> entries)
        {
            string ext = Path.GetExtension(mainFile);
            if (!IsImageExt(ext))
                throw new FormatException("角色主立绘必须是 .png / .jpg / .jpeg");

            var portraits = new Dictionary<string, string>(StringComparer.Ordinal);
            object portraitsObj;
            if (root.TryGetValue("portraits", out portraitsObj) && portraitsObj is Dictionary<string, object>)
            {
                foreach (var pair in (Dictionary<string, object>)portraitsObj)
                {
                    if (string.IsNullOrEmpty(pair.Key) || pair.Value == null)
                        continue;
                    string fname = Convert.ToString(pair.Value) ?? "";
                    if (fname.IndexOf('/') >= 0 || fname.IndexOf('\\') >= 0 || fname.IndexOf("..", StringComparison.Ordinal) >= 0)
                        throw new FormatException("portraits 文件名不能含路径：" + fname);
                    if (!IsImageExt(Path.GetExtension(fname)))
                        throw new FormatException("立绘必须是 .png / .jpg / .jpeg：" + fname);
                    portraits[pair.Key] = fname;
                }
            }
            if (!portraits.ContainsKey("normal"))
                portraits["normal"] = mainFile;

            var combatFrames = new Dictionary<string, string>(StringComparer.Ordinal);
            string[] combatNames = { "idle", "attack", "hurt", "defence" };
            for (int i = 0; i < combatNames.Length; i++)
            {
                string field = "combat_" + combatNames[i];
                string fname = GetString(root, field, required: false);
                if (string.IsNullOrEmpty(fname)) continue;
                if (fname.IndexOf('/') >= 0 || fname.IndexOf('\\') >= 0
                    || fname.IndexOf("..", StringComparison.Ordinal) >= 0
                    || !IsImageExt(Path.GetExtension(fname)))
                    throw new FormatException(field + " 必须是同一角色目录下的图片文件名");
                combatFrames[combatNames[i]] = fname;
            }

            var fileBytes = new Dictionary<string, byte[]>(StringComparer.OrdinalIgnoreCase);
            foreach (var pair in portraits)
            {
                string path = expectedDir + "/" + pair.Value;
                if (!fileBytes.ContainsKey(pair.Value))
                    fileBytes[pair.Value] = ReadZipBytes(package, entries, path, 8L * 1024 * 1024, "立绘");
            }
            foreach (var pair in combatFrames)
            {
                string path = expectedDir + "/" + pair.Value;
                if (!fileBytes.ContainsKey(pair.Value))
                    fileBytes[pair.Value] = ReadZipBytes(package, entries, path, 8L * 1024 * 1024, "决斗动画");
            }
            byte[] mainBytes;
            if (!fileBytes.TryGetValue(mainFile, out mainBytes))
            {
                mainBytes = ReadZipBytes(package, entries, expectedDir + "/" + mainFile, 8L * 1024 * 1024, "立绘");
                fileBytes[mainFile] = mainBytes;
            }

            string title = GetString(root, "title", required: false);
            if (string.IsNullOrEmpty(title) && root.ContainsKey("intro")
                && root["intro"] is Dictionary<string, object>)
            {
                object introTitle;
                if (((Dictionary<string, object>)root["intro"]).TryGetValue("title", out introTitle)
                    && introTitle != null)
                    title = Convert.ToString(introTitle);
            }
            string artFacing = GetString(root, "art_facing", required: false);
            if (string.IsNullOrEmpty(artFacing))
                artFacing = "left";
            else if (!artFacing.Equals("left", StringComparison.OrdinalIgnoreCase)
                && !artFacing.Equals("right", StringComparison.OrdinalIgnoreCase))
                artFacing = "left";
            else
                artFacing = artFacing.Equals("right", StringComparison.OrdinalIgnoreCase) ? "right" : "left";
            return new UserContent
            {
                Id = id,
                Type = "character",
                Name = name,
                Title = title,
                Scale = GetClampedInt(root, "scale", 100, 50, 130),
                ArtFacing = artFacing,
                MainFile = mainFile,
                PackagePath = expectedDir + "/" + mainFile,
                Bytes = mainBytes,
                Portraits = portraits,
                CombatFrames = combatFrames,
                Files = fileBytes
            };
        }

        private static UserContent ParseImageContent(
            ModPackage package,
            string expectedDir,
            string id,
            string name,
            string mainFile,
            Dictionary<string, ZipArchiveEntry> entries)
        {
            if (!IsImageExt(Path.GetExtension(mainFile)))
                throw new FormatException("用户图片只支持 .png / .jpg / .jpeg");
            string imagePath = expectedDir + "/" + mainFile;
            byte[] bytes = ReadZipBytes(package, entries, imagePath, ContentRef.MaxImageBytes, "图片");
            return new UserContent
            {
                Id = id,
                Type = "image",
                Name = name,
                MainFile = mainFile,
                PackagePath = imagePath,
                Bytes = bytes
            };
        }

        private static int GetClampedInt(
            Dictionary<string, object> dict,
            string key,
            int defaultValue,
            int lo,
            int hi)
        {
            object value;
            if (!dict.TryGetValue(key, out value) || value == null)
                return defaultValue;
            int number;
            if (value is double)
                number = (int)(double)value;
            else if (value is long)
                number = (int)(long)value;
            else if (value is int)
                number = (int)value;
            else if (value is string)
            {
                if (!int.TryParse((string)value, out number))
                    return defaultValue;
            }
            else
                return defaultValue;
            if (number < lo) return lo;
            if (number > hi) return hi;
            return number;
        }

        private static bool IsImageExt(string ext)
        {
            return ext.Equals(".png", StringComparison.OrdinalIgnoreCase)
                || ext.Equals(".jpg", StringComparison.OrdinalIgnoreCase)
                || ext.Equals(".jpeg", StringComparison.OrdinalIgnoreCase);
        }

        private static byte[] ReadZipBytes(
            ModPackage package,
            Dictionary<string, ZipArchiveEntry> entries,
            string path,
            long maxBytes,
            string label)
        {
            byte[] cached;
            if (package.Assets.TryGetValue(NormalizeAssetPath(path), out cached))
            {
                if (cached.LongLength > maxBytes)
                    throw new FormatException(label + "超过上限：" + path);
                if (cached.Length == 0)
                    throw new FormatException(label + "文件为空：" + path);
                return cached;
            }
            ZipArchiveEntry entry;
            if (!entries.TryGetValue(path, out entry))
                throw new FormatException("缺少" + label + "文件 " + path);
            if (entry.Length > maxBytes)
                throw new FormatException(label + "超过上限：" + path);
            using (var input = entry.Open())
            {
                byte[] bytes = ReadBoundedBytes(input, maxBytes, path);
                if (bytes.Length == 0)
                    throw new FormatException(label + "文件为空：" + path);
                return bytes;
            }
        }

        private static string ReadEntryText(ZipArchiveEntry entry)
        {
            if (entry.Length > MaxTextBytes)
                throw new FormatException("文本条目超过 4MB 上限：" + entry.FullName);
            using (var input = entry.Open())
            {
                byte[] bytes = ReadBoundedBytes(input, MaxTextBytes, entry.FullName);
                try
                {
                    string text = new UTF8Encoding(false, true).GetString(bytes);
                    return text.Length > 0 && text[0] == '\uFEFF' ? text.Substring(1) : text;
                }
                catch (DecoderFallbackException)
                {
                    throw new FormatException("文本条目不是合法 UTF-8：" + entry.FullName);
                }
            }
        }

        private static void ValidateArchiveLimits(ZipArchive zip)
        {
            if (zip.Entries.Count > MaxArchiveEntries)
                throw new FormatException("包内条目数超过 " + MaxArchiveEntries + " 上限");
            long total = 0;
            var exactPaths = new HashSet<string>(StringComparer.Ordinal);
            var foldedPaths = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            var filePaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var directoryPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var entry in zip.Entries)
            {
                string path = ValidateArchivePath(entry.FullName);
                if (!exactPaths.Add(path))
                    throw new FormatException("包内存在重复路径：" + path);
                string existing;
                if (foldedPaths.TryGetValue(path, out existing)
                    && !string.Equals(existing, path, StringComparison.Ordinal))
                    throw new FormatException("包内存在大小写冲突路径：" + existing + " / " + path);
                foldedPaths[path] = path;
                if (entry.Length < 0 || entry.Length > MaxEntryBytes)
                    throw new FormatException("包内条目超过 32MB 上限：" + entry.FullName);
                if (!path.EndsWith("/", StringComparison.Ordinal) && IsTextArchivePath(path)
                    && entry.Length > MaxTextBytes)
                    throw new FormatException("包内文本条目超过 4MB 上限：" + entry.FullName);
                if (total > MaxArchiveBytes - entry.Length)
                    throw new FormatException("包内总未压缩大小超过 128MB 上限");
                total += entry.Length;
                if (path.EndsWith("/", StringComparison.Ordinal))
                    directoryPaths.Add(path.Substring(0, path.Length - 1));
                else
                    filePaths.Add(path);
            }

            foreach (string path in exactPaths)
            {
                string body = path.EndsWith("/", StringComparison.Ordinal)
                    ? path.Substring(0, path.Length - 1) : path;
                string[] parts = body.Split('/');
                for (int i = 1; i < parts.Length; i++)
                {
                    string prefix = string.Join("/", parts, 0, i);
                    if (filePaths.Contains(prefix))
                        throw new FormatException("包内路径同时被用作文件和目录：" + prefix);
                }
            }
            foreach (string file in filePaths)
                if (directoryPaths.Contains(file))
                    throw new FormatException("包内路径同时被用作文件和目录：" + file);
        }

        private static bool IsTextArchivePath(string path)
        {
            return path.EndsWith(".json", StringComparison.OrdinalIgnoreCase)
                || path.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)
                || path.EndsWith(".sha256", StringComparison.OrdinalIgnoreCase)
                || path.EndsWith(".txt", StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>
        /// ZIP 条目必须使用唯一、规范的相对 POSIX 路径。Runtime 从不解压条目，但仍与
        /// Editor 使用同一规则拒绝路径穿越、Windows drive/UNC、反斜杠和歧义路径。
        /// </summary>
        private static string ValidateArchivePath(string path)
        {
            if (string.IsNullOrEmpty(path) || path.IndexOf('\0') >= 0)
                throw new FormatException("包内包含空路径或 NUL 字符");
            if (path.IndexOf('\\') >= 0)
                throw new FormatException("包内路径必须使用正斜杠：" + path);
            if (path[0] == '/' || (path.Length >= 2 && IsAsciiLetter(path[0]) && path[1] == ':'))
                throw new FormatException("包内包含绝对路径：" + path);

            bool directory = path.EndsWith("/", StringComparison.Ordinal);
            string body = directory ? path.Substring(0, path.Length - 1) : path;
            if (body.Length == 0)
                throw new FormatException("包内包含根目录条目");
            string[] parts = body.Split('/');
            foreach (string part in parts)
            {
                if (part.Length == 0 || part == "." || part == "..")
                    throw new FormatException("包内包含不安全或非规范路径：" + path);
                if (part.IndexOf(':') >= 0)
                    throw new FormatException("包内路径段禁止冒号（Windows ADS 风险）：" + path);
                if (part.EndsWith(".", StringComparison.Ordinal)
                    || part.EndsWith(" ", StringComparison.Ordinal))
                    throw new FormatException("包内路径段禁止尾随点或空格：" + path);
                if (IsWindowsDeviceName(part))
                    throw new FormatException("包内路径使用 Windows 保留设备名：" + path);
            }
            return directory ? body + "/" : body;
        }

        private static bool IsWindowsDeviceName(string segment)
        {
            string stem = segment;
            int dot = stem.IndexOf('.');
            if (dot >= 0) stem = stem.Substring(0, dot);
            stem = stem.ToUpperInvariant();
            if (stem == "CON" || stem == "PRN" || stem == "AUX" || stem == "NUL")
                return true;
            if (stem.Length == 4 && (stem.StartsWith("COM", StringComparison.Ordinal)
                || stem.StartsWith("LPT", StringComparison.Ordinal)))
                return stem[3] >= '1' && stem[3] <= '9';
            return false;
        }

        private static bool IsAsciiLetter(char value)
        {
            return (value >= 'A' && value <= 'Z') || (value >= 'a' && value <= 'z');
        }

        /// <summary>
        /// 验证 v3 打包器生成的压缩无关逻辑内容哈希。记录必填；格式错误、缺失或
        /// 任一条目被替换都拒绝整包。v1/v2 在 manifest 版本门禁处已直接拒绝。
        /// </summary>
        private static void ValidatePackageContentHash(ZipArchive zip, bool required)
        {
            ZipArchiveEntry recordEntry = zip.GetEntry(PackageContentHashEntry);
            if (recordEntry == null)
            {
                if (required)
                    throw new FormatException("package_format=3 缺少 " + PackageContentHashEntry);
                return;
            }

            Dictionary<string, string> record = ParseHashRecord(ReadEntryText(recordEntry));
            string algorithm;
            string declared;
            if (!record.TryGetValue("algorithm", out algorithm)
                || algorithm != PackageContentHashAlgorithm
                || !record.TryGetValue("sha256", out declared)
                || !IsUpperHexSha256(declared))
                throw new FormatException(PackageContentHashEntry + " 记录格式无效");

            string actual;
            using (var digest = SHA256.Create())
            {
                var entries = new List<ZipArchiveEntry>();
                foreach (ZipArchiveEntry entry in zip.Entries)
                    if (entry.Name.Length != 0 && entry.FullName != PackageContentHashEntry)
                        entries.Add(entry);
                entries.Sort(delegate(ZipArchiveEntry left, ZipArchiveEntry right)
                {
                    return string.CompareOrdinal(left.FullName, right.FullName);
                });
                foreach (ZipArchiveEntry entry in entries)
                {
                    byte[] name = Encoding.UTF8.GetBytes(entry.FullName);
                    AddBigEndian(digest, (uint)name.Length, 4);
                    digest.TransformBlock(name, 0, name.Length, name, 0);
                    AddBigEndian(digest, (ulong)entry.Length, 8);
                    using (Stream input = entry.Open())
                    {
                        byte[] buffer = new byte[81920];
                        int read;
                        long actualLength = 0;
                        while ((read = input.Read(buffer, 0, buffer.Length)) > 0)
                        {
                            if (actualLength > entry.Length - read)
                                throw new FormatException("条目读取长度与 ZIP 元数据不一致：" + entry.FullName);
                            actualLength += read;
                            digest.TransformBlock(buffer, 0, read, buffer, 0);
                        }
                        if (actualLength != entry.Length)
                            throw new FormatException("条目读取长度与 ZIP 元数据不一致：" + entry.FullName);
                    }
                }
                digest.TransformFinalBlock(new byte[0], 0, 0);
                actual = ToUpperHex(digest.Hash);
            }
            if (!string.Equals(actual, declared, StringComparison.Ordinal))
                throw new FormatException(PackageContentHashEntry + " 与包内逻辑内容不一致");
        }

        /// <summary>
        /// 验证 Story 原始字节与由打包器编译出的每份 Lua（含本地化变体）的绑定记录。
        /// 旧包可缺少该记录；新打包器产物一旦含记录，缺行、重复行或内容替换均拒绝。
        /// </summary>
        private static void ValidateStoryLuaIntegrity(ZipArchive zip, bool required)
        {
            ZipArchiveEntry recordEntry = zip.GetEntry(StoryLuaHashEntry);
            if (recordEntry == null)
            {
                if (required)
                    throw new FormatException("package_format=3 缺少 " + StoryLuaHashEntry);
                return;
            }
            string[] lines = ReadEntryText(recordEntry).Replace("\r\n", "\n").Split('\n');
            if (lines.Length == 0 || lines[0] != "algorithm=" + StoryLuaHashAlgorithm)
                throw new FormatException(StoryLuaHashEntry + " 算法标识无效");

            var expectedLua = new HashSet<string>(StringComparer.Ordinal);
            foreach (ZipArchiveEntry entry in zip.Entries)
                if (entry.Name.Length != 0 && IsRuntimeLuaPath(entry.FullName))
                    expectedLua.Add(entry.FullName);
            if (expectedLua.Count == 0)
                throw new FormatException("包内没有可绑定的 Lua 脚本");

            var seenLua = new HashSet<string>(StringComparer.Ordinal);
            for (int i = 1; i < lines.Length; i++)
            {
                if (lines[i].Length == 0) continue;
                string[] fields = lines[i].Split('\t');
                if (fields.Length != 4 || !IsStoryPath(fields[0]) || !IsUpperHexSha256(fields[1])
                    || !IsRuntimeLuaPath(fields[2]) || !IsUpperHexSha256(fields[3]))
                    throw new FormatException(StoryLuaHashEntry + " 第 " + (i + 1) + " 行格式无效");
                if (!seenLua.Add(fields[2]))
                    throw new FormatException(StoryLuaHashEntry + " 重复声明 " + fields[2]);
                ZipArchiveEntry story = zip.GetEntry(fields[0]);
                ZipArchiveEntry lua = zip.GetEntry(fields[2]);
                if (story == null || lua == null)
                    throw new FormatException(StoryLuaHashEntry + " 引用不存在条目：" + fields[0] + " / " + fields[2]);
                string storyId = fields[0].Substring("story/".Length,
                    fields[0].Length - "story/".Length - ".json".Length);
                string luaId = Path.GetFileNameWithoutExtension(fields[2]);
                if (!string.Equals(storyId, luaId, StringComparison.Ordinal))
                    throw new FormatException(StoryLuaHashEntry + " 的 Story/Lua id 不一致：" + fields[0] + " / " + fields[2]);
                if (!string.Equals(HashEntry(story), fields[1], StringComparison.Ordinal)
                    || !string.Equals(HashEntry(lua), fields[3], StringComparison.Ordinal))
                    throw new FormatException("Story/Lua 一致性校验失败：" + fields[2]);
            }
            if (!expectedLua.SetEquals(seenLua))
                throw new FormatException(StoryLuaHashEntry + " 未完整覆盖全部 Lua 脚本");
        }

        private static bool IsStoryPath(string path)
        {
            if (!path.StartsWith("story/", StringComparison.Ordinal) || !path.EndsWith(".json", StringComparison.Ordinal))
                return false;
            string rest = path.Substring("story/".Length);
            return rest.Length > ".json".Length && rest.IndexOf('/') < 0;
        }

        private static bool IsRuntimeLuaPath(string path)
        {
            if (!path.StartsWith("lua/", StringComparison.Ordinal) || !path.EndsWith(".lua", StringComparison.Ordinal))
                return false;
            string rest = path.Substring("lua/".Length);
            string[] parts = rest.Split('/');
            return (parts.Length == 1 || parts.Length == 2)
                && parts[parts.Length - 1].Length > ".lua".Length;
        }

        private static string HashEntry(ZipArchiveEntry entry)
        {
            using (SHA256 sha = SHA256.Create())
            using (Stream input = entry.Open())
            {
                byte[] bytes = ReadBoundedBytes(input, MaxEntryBytes, entry.FullName);
                if (bytes.LongLength != entry.Length)
                    throw new FormatException("条目读取长度与 ZIP 元数据不一致：" + entry.FullName);
                return ToUpperHex(sha.ComputeHash(bytes));
            }
        }

        private static Dictionary<string, string> ParseHashRecord(string text)
        {
            var result = new Dictionary<string, string>(StringComparer.Ordinal);
            string[] lines = text.Replace("\r\n", "\n").Split('\n');
            foreach (string line in lines)
            {
                if (line.Length == 0) continue;
                int separator = line.IndexOf('=');
                if (separator <= 0 || separator == line.Length - 1)
                    throw new FormatException("哈希记录行格式无效");
                string key = line.Substring(0, separator);
                if (result.ContainsKey(key))
                    throw new FormatException("哈希记录字段重复：" + key);
                result[key] = line.Substring(separator + 1);
            }
            return result;
        }

        private static bool IsUpperHexSha256(string value)
        {
            if (value == null || value.Length != 64) return false;
            for (int i = 0; i < value.Length; i++)
                if (!((value[i] >= '0' && value[i] <= '9') || (value[i] >= 'A' && value[i] <= 'F')))
                    return false;
            return true;
        }

        private static void AddBigEndian(HashAlgorithm digest, ulong value, int bytes)
        {
            byte[] buffer = new byte[bytes];
            for (int i = bytes - 1; i >= 0; i--)
            {
                buffer[i] = (byte)(value & 0xFF);
                value >>= 8;
            }
            digest.TransformBlock(buffer, 0, buffer.Length, buffer, 0);
        }

        private static byte[] ReadBoundedBytes(Stream input, long maxBytes, string path)
        {
            using (var ms = new MemoryStream())
            {
                var buffer = new byte[81920];
                long total = 0;
                while (true)
                {
                    int read = input.Read(buffer, 0, buffer.Length);
                    if (read == 0) break;
                    if (total > maxBytes - read)
                        throw new FormatException("条目读取超过大小上限：" + path);
                    ms.Write(buffer, 0, read);
                    total += read;
                }
                return ms.ToArray();
            }
        }
    }
}
