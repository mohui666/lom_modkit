using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// mod 包扫描与解析（纯静态、无 BepInEx/Unity 依赖，便于离线单测）。
    /// 行为契约见 docs/zh_CN/mod_format.md §6：扫描 mods/*.lommod，解出 manifest.json、lua/*.lua、可选 texts.json（契约 §1）
    /// 与 assets/ 下图片（契约 §3.1）。单个包损坏只警告跳过，绝不抛出让插件崩溃。
    /// </summary>
    internal static class ModLoader
    {
        /// <summary>结局卡背景图单张上限（字节，契约 §3.1，与编译器 pack 校验一致）。</summary>
        private const long MaxEndingImageBytes = 8L * 1024 * 1024;

        /// <summary>用户音频单条上限（与 compiler/lomc/content.py 一致）。</summary>
        private const long MaxAudioBytes = ContentRef.MaxAudioBytes;

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
            if (!Directory.Exists(modsDir))
            {
                Directory.CreateDirectory(modsDir);
                if (logInfo != null) logInfo("mods 目录不存在，已创建：" + modsDir);
                return result;
            }

            string[] files = Directory.GetFiles(modsDir, "*.lommod");
            Array.Sort(files, StringComparer.OrdinalIgnoreCase); // 按文件名排序，加载顺序稳定
            foreach (string file in files)
            {
                try
                {
                    result.Add(LoadPackage(file, logWarn));
                }
                catch (Exception ex)
                {
                    // 坏 zip / 缺 manifest / 缺 entry lua / JSON 非法等都走这里
                    if (logWarn != null)
                        logWarn("跳过损坏的 mod 包 " + Path.GetFileName(file) + "：" + ex.Message);
                }
            }
            return result;
        }

        /// <summary>
        /// 解析单个 .lommod（zip 全程只走内存流，不解到磁盘）。校验失败抛异常，由调用方兜底。
        /// </summary>
        private static ModPackage LoadPackage(string path, Action<string> logWarn)
        {
            using (var stream = File.OpenRead(path))
            using (var zip = new ZipArchive(stream, ZipArchiveMode.Read))
            {
                var manifestEntry = zip.GetEntry("manifest.json");
                if (manifestEntry == null)
                    throw new FormatException("包内缺少 manifest.json");

                var package = ParseManifest(ReadEntryText(manifestEntry));
                package.PackagePath = path;

                // 收集 lua/<id>.lua（仅 lua/ 直接子项，契约 §1）
                foreach (var entry in zip.Entries)
                {
                    if (!entry.FullName.StartsWith("lua/", StringComparison.Ordinal)) continue;
                    string rest = entry.FullName.Substring(4);
                    if (rest.Length == 0 || rest.IndexOf('/') >= 0) continue;       // 跳过子目录
                    if (!rest.EndsWith(".lua", StringComparison.OrdinalIgnoreCase)) continue;
                    string scriptId = rest.Substring(0, rest.Length - 4);
                    if (scriptId.Length == 0) continue;
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
                        package.DefaultLocale = "zh_CN";
                        package.FallbackLocale = "zh_CN";
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
                        using (var ms = new MemoryStream())
                        {
                            input.CopyTo(ms);
                            package.Assets[NormalizeAssetPath(entry.FullName)] = ms.ToArray();
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

        /// <summary>解析 manifest.json，校验契约 §2 的必填字段。</summary>
        private static ModPackage ParseManifest(string json)
        {
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null)
                throw new FormatException("manifest.json 顶层必须是 JSON 对象");

            // format 固定为 1，不符说明是未知格式版本，跳过比误加载安全
            object formatObj;
            double format;
            if (!root.TryGetValue("format", out formatObj) || !(formatObj is double) || (format = (double)formatObj) != 1.0)
                throw new FormatException("manifest.format 不是 1（本插件只支持格式 v1）");

            var package = new ModPackage
            {
                Id = GetString(root, "id", required: true),
                Name = GetString(root, "name", required: false) ?? "",
                Version = GetString(root, "version", required: false) ?? "",
                Author = GetString(root, "author", required: false) ?? "",
                Description = GetString(root, "description", required: false) ?? "",
                Entry = GetString(root, "entry", required: true),
                Campaign = ParseCampaign(root)
            };
            return package;
        }

        /// <summary>解析可选的 campaign 段（契约 §2）；无该段返回 null，结构非法抛 FormatException。</summary>
        private static CampaignConfig ParseCampaign(Dictionary<string, object> root)
        {
            object campaignObj;
            if (!root.TryGetValue("campaign", out campaignObj) || campaignObj == null)
                return null;
            var dict = campaignObj as Dictionary<string, object>;
            if (dict == null)
                throw new FormatException("manifest.campaign 必须是对象");

            var campaign = new CampaignConfig();
            object newGameObj;
            if (dict.TryGetValue("new_game", out newGameObj) && newGameObj != null)
            {
                if (!(newGameObj is bool))
                    throw new FormatException("manifest.campaign.new_game 必须是布尔值");
                campaign.NewGame = (bool)newGameObj;
            }

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
            if (!affinity.TryGetValue("min", out minObj) || !(minObj is double) || (double)minObj % 1 != 0 || (double)minObj < 0)
                throw new FormatException("manifest 触发器 when_affinity.min 必须是非负整数");
            return new AffinityCondition
            {
                Character = GetString(affinity, "character", required: true),
                Min = (int)(double)minObj
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
            return locale == "zh_CN" || locale == "zh_TW" || locale == "ja" || locale == "ko";
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
                throw new FormatException("default_locale/fallback_locale 只支持 zh_CN/zh_TW/ja/ko");
            package.DefaultLocale = defaultLocale;
            package.FallbackLocale = fallbackLocale;
        }

        private static void LoadLocalizedScriptsAndTexts(ModPackage package, ZipArchive zip)
        {
            foreach (string locale in new[] { "zh_CN", "zh_TW", "ja", "ko" })
            {
                var scripts = new Dictionary<string, string>();
                string prefix = "lua/" + locale + "/";
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
                var textsEntry = zip.GetEntry("texts/" + locale + ".json");
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
            if (!root.TryGetValue("schema", out schemaObj) || !(schemaObj is double) || (double)schemaObj != 1.0)
                throw new FormatException("content.json schema 必须是 1");

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
                return ParseImageContent(expectedDir, id, name, mainFile, entries);
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
            byte[] bytes = ReadZipBytes(entries, audioPath, MaxAudioBytes, "音频");
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

            var fileBytes = new Dictionary<string, byte[]>(StringComparer.OrdinalIgnoreCase);
            foreach (var pair in portraits)
            {
                string path = expectedDir + "/" + pair.Value;
                if (!fileBytes.ContainsKey(pair.Value))
                    fileBytes[pair.Value] = ReadZipBytes(entries, path, 8L * 1024 * 1024, "立绘");
            }
            byte[] mainBytes;
            if (!fileBytes.TryGetValue(mainFile, out mainBytes))
            {
                mainBytes = ReadZipBytes(entries, expectedDir + "/" + mainFile, 8L * 1024 * 1024, "立绘");
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
                Files = fileBytes
            };
        }

        private static UserContent ParseImageContent(
            string expectedDir,
            string id,
            string name,
            string mainFile,
            Dictionary<string, ZipArchiveEntry> entries)
        {
            if (!IsImageExt(Path.GetExtension(mainFile)))
                throw new FormatException("用户图片只支持 .png / .jpg / .jpeg");
            string imagePath = expectedDir + "/" + mainFile;
            byte[] bytes = ReadZipBytes(entries, imagePath, ContentRef.MaxImageBytes, "图片");
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
            Dictionary<string, ZipArchiveEntry> entries,
            string path,
            long maxBytes,
            string label)
        {
            ZipArchiveEntry entry;
            if (!entries.TryGetValue(path, out entry))
                throw new FormatException("缺少" + label + "文件 " + path);
            if (entry.Length > maxBytes)
                throw new FormatException(label + "超过上限：" + path);
            using (var input = entry.Open())
            using (var ms = new MemoryStream())
            {
                input.CopyTo(ms);
                byte[] bytes = ms.ToArray();
                if (bytes.Length == 0)
                    throw new FormatException(label + "文件为空：" + path);
                return bytes;
            }
        }

        private static string ReadEntryText(ZipArchiveEntry entry)
        {
            using (var reader = new StreamReader(entry.Open(), Encoding.UTF8))
                return reader.ReadToEnd();
        }
    }
}
