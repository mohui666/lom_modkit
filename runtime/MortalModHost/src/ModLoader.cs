using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// mod 包扫描与解析（纯静态、无 BepInEx/Unity 依赖，便于离线单测）。
    /// 行为契约见 docs/mod_format.md §6：扫描 mods/*.lommod，解出 manifest.json、lua/*.lua、可选 texts.json（契约 §1）
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
            var root = MiniJson.Parse(json) as Dictionary<string, object>;
            if (root == null)
                throw new FormatException("texts.json 顶层必须是 JSON 对象");
            foreach (var pair in root)
            {
                var text = pair.Value as string;
                if (text == null)
                    throw new FormatException("texts.json 键 " + pair.Key + " 的值必须是字符串");
                package.Texts[pair.Key] = text;
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
                        package.UserContents[content.Id] = content;
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
            if (type != "audio")
            {
                if (logWarn != null)
                    logWarn("mod " + package.Id + " 的用户内容 " + id + " 类型 " + type + " 本版本不加载");
                return null;
            }

            string name = GetString(root, "name", required: true);
            string audioKind = GetString(root, "audio_kind", required: true);
            if (audioKind != "music" && audioKind != "sound" && audioKind != "env")
                throw new FormatException("audio_kind 必须是 music / sound / env");

            object filesObj;
            if (!root.TryGetValue("files", out filesObj))
                throw new FormatException("content.json 缺少 files");
            var files = filesObj as Dictionary<string, object>;
            if (files == null)
                throw new FormatException("content.json files 必须是对象");
            string mainFile = GetString(files, "main", required: true);
            if (mainFile.IndexOf('/') >= 0 || mainFile.IndexOf('\\') >= 0 || mainFile.IndexOf("..", StringComparison.Ordinal) >= 0)
                throw new FormatException("files.main 必须是同目录文件名");
            string ext = Path.GetExtension(mainFile);
            if (!ext.Equals(".ogg", StringComparison.OrdinalIgnoreCase)
                && !ext.Equals(".wav", StringComparison.OrdinalIgnoreCase))
                throw new FormatException("用户音频只支持 .ogg / .wav");

            string audioPath = expectedDir + "/" + mainFile;
            ZipArchiveEntry audioEntry;
            if (!entries.TryGetValue(audioPath, out audioEntry))
                throw new FormatException("缺少音频文件 " + audioPath);
            if (audioEntry.Length > MaxAudioBytes)
                throw new FormatException("音频超过 20MB：" + audioPath);

            byte[] bytes;
            using (var input = audioEntry.Open())
            using (var ms = new MemoryStream())
            {
                input.CopyTo(ms);
                bytes = ms.ToArray();
            }
            if (bytes.Length == 0)
                throw new FormatException("音频文件为空：" + audioPath);

            return new UserContent
            {
                Id = id,
                Type = type,
                Name = name,
                AudioKind = audioKind,
                MainFile = mainFile,
                PackagePath = audioPath,
                Bytes = bytes
            };
        }

        private static string ReadEntryText(ZipArchiveEntry entry)
        {
            using (var reader = new StreamReader(entry.Open(), Encoding.UTF8))
                return reader.ReadToEnd();
        }
    }
}
