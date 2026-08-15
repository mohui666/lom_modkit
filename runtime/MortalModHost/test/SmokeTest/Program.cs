using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// ModLoader/MiniJson/ModRegistry/HotkeyMigration 离线冒烟测试：构造 1 个好包 + 3 个坏包，验证解析、容错与热键迁移。
    /// 断言失败抛异常（退出码非 0），全部通过打印 PASS。
    /// </summary>
    internal static class Program
    {
        private static int Main()
        {
            // 临时目录里造测试包，跑完即删
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_smoketest_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                WriteGoodPackage(Path.Combine(modsDir, "demo_mod.lommod"));
                File.WriteAllText(Path.Combine(modsDir, "badzip.lommod"), "这不是 zip");            // 坏 zip
                WriteZip(Path.Combine(modsDir, "nomanifest.lommod"),                             // 缺 manifest
                    ("lua/main.lua", "say(\"hi\")"));
                WriteZip(Path.Combine(modsDir, "noentry.lommod"),                                // 缺 entry lua
                    ("manifest.json", "{\"format\":1,\"id\":\"noentry\",\"name\":\"缺入口\",\"version\":\"0.1\",\"entry\":\"main\"}"),
                    ("lua/other.lua", "say(\"other\")"));
                // texts.json 非法 → 包仍加载，文本忽略 + 1 警告（契约 §A 可选文件容错）
                WriteZip(Path.Combine(modsDir, "badtexts.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badtexts\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"m\")"),
                    ("texts.json", "{\"MOD_badtexts_main_n1\": 123}"));

                var warnings = new List<string>();
                var infos = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, infos.Add, warnings.Add);

                // 应加载出 2 个好包（badtexts 的 texts.json 容错 + demo_mod）
                Assert(mods.Count == 2, "应加载 2 个好包，实际 " + mods.Count);
                var mod = mods.First(m => m.Id == "demo_mod");
                Assert(mod.Id == "demo_mod", "id 解析错误：" + mod.Id);
                Assert(mod.Name == "示例 Mod", "name 解析错误（含中文）：" + mod.Name);
                Assert(mod.Version == "1.0.0", "version 解析错误：" + mod.Version);
                Assert(mod.Author == "somebody", "author 解析错误：" + mod.Author);
                Assert(mod.Entry == "main", "entry 解析错误：" + mod.Entry);
                Assert(mod.LuaScripts.Count == 2, "应有 2 个 lua 脚本，实际 " + mod.LuaScripts.Count);
                Assert(mod.LuaScripts.ContainsKey("main") && mod.LuaScripts.ContainsKey("extra"), "脚本 id 清单错误");
                Assert(mod.LuaScripts["main"].Contains("node_n1"), "lua 内容未读入内存");
                Assert(mod.GetRegisteredScriptName("main") == "MOD_demo_mod_main", "注册名前缀错误：" + mod.GetRegisteredScriptName("main"));
                // 契约 §A：texts.json 解析（含中文文本）
                Assert(mod.Texts.Count == 2, "texts.json 应含 2 条文本，实际 " + mod.Texts.Count);
                Assert(mod.Texts.ContainsKey("MOD_demo_mod_main_n1") && mod.Texts["MOD_demo_mod_main_n1"] == "你好", "texts.json 条目 n1 解析错误");
                Assert(mod.Texts.ContainsKey("MOD_demo_mod_main_n2") && mod.Texts["MOD_demo_mod_main_n2"] == "第二句\"带引号\"", "texts.json 条目 n2 解析错误");
                var badTexts = mods.First(m => m.Id == "badtexts");
                Assert(badTexts.Texts.Count == 0, "坏 texts.json 应被忽略，实际 " + badTexts.Texts.Count);
                // 契约 §3.1：assets/ 下图片读入内存（键=包内正斜杠路径、字节逐位对比）；
                // 非图片条目不读；超 8MB 图片警告跳过（本包含 1 张有效 png + 1 张 9MB png + 1 个 ogg）
                Assert(mod.Assets.Count == 1, "assets 应只读入 1 张图片，实际 " + mod.Assets.Count);
                byte[] endingPng;
                byte[] expectedPng = Convert.FromBase64String(PngBase64);
                Assert(mod.Assets.TryGetValue("assets/ending.png", out endingPng)
                    && endingPng.Length == expectedPng.Length
                    && endingPng.SequenceEqual(expectedPng),
                    "assets/ending.png 未读入或字节不一致（长度 " + (endingPng == null ? -1 : endingPng.Length) + "）");
                Assert(!mod.Assets.ContainsKey("assets/bgm.ogg"), "非图片条目不应读入内存");
                Assert(!mod.Assets.ContainsKey("assets/huge.png"), "超 8MB 图片应被跳过");
                Assert(warnings.Count == 5, "应有 5 条坏包警告（含 texts.json 容错 + 超限图片跳过），实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

                // ModRegistry：注册名命中/未命中/冲突时保留先加载者
                var dup = new ModPackage { Id = "demo_mod", Entry = "main" }; // 与好包同 id，制造注册名冲突
                dup.LuaScripts["main"] = "say(\"dup\")";
                ModRegistry.Rebuild(new ModPackage[] { mod, dup }, warnings.Add);
                string lua;
                Assert(ModRegistry.TryGetLuaByRegisteredName("MOD_demo_mod_main", out lua), "注册名 MOD_demo_mod_main 应命中");
                Assert(lua.Contains("node_n1"), "冲突时应保留先加载包的内容，实际：" + lua);
                Assert(ModRegistry.TryGetLuaByRegisteredName("MOD_demo_mod_extra", out lua), "注册名 MOD_demo_mod_extra 应命中");
                Assert(!ModRegistry.TryGetLuaByRegisteredName("MOD_demo_mod_nope", out lua), "不存在的脚本不应命中");
                Assert(!ModRegistry.TryGetLuaByRegisteredName("act1", out lua), "官方脚本名不应命中");
                Assert(!ModRegistry.TryGetLuaByRegisteredName(null, out lua), "null 不应命中");
                // 契约 §3.1：按注册名查所属 mod 包（结局卡背景图按当前演出 mod 的 assets 解析）
                ModPackage byName;
                Assert(ModRegistry.TryGetPackageByRegisteredName("MOD_demo_mod_main", out byName) && byName == mod,
                    "注册名 MOD_demo_mod_main 应查到 demo_mod 包");
                Assert(!ModRegistry.TryGetPackageByRegisteredName("MOD_demo_mod_nope", out byName), "不存在的脚本不应查到包");
                Assert(!ModRegistry.TryGetPackageByRegisteredName(null, out byName), "null 不应查到包");
                Assert(ModRegistry.Count == 2, "注册表应含 2 个脚本，实际 " + ModRegistry.Count);
                Assert(warnings.Count == 6, "注册名冲突应新增 1 条警告，实际共 " + warnings.Count + "：" + string.Join(" | ", warnings));

                // HotkeyMigration：cfg 里旧默认 F9 → F8，其余内容一律不动
                string migrated;
                Assert(HotkeyMigration.TryRewriteLegacyHotkey("[General]\nMenuHotkey = F9\n", out migrated)
                    && migrated == "[General]\nMenuHotkey = F8\n", "基本改写不符：" + migrated);
                Assert(HotkeyMigration.TryRewriteLegacyHotkey("MenuHotkey = F9\r\n", out migrated)
                    && migrated == "MenuHotkey = F8\r\n", "CRLF 改写不符：" + migrated);
                Assert(HotkeyMigration.TryRewriteLegacyHotkey("MenuHotkey = F9 \n", out migrated)
                    && migrated == "MenuHotkey = F8 \n", "行尾空格改写不符：" + migrated);
                Assert(!HotkeyMigration.TryRewriteLegacyHotkey("MenuHotkey = F10\n", out migrated), "用户改过的值不应动");
                Assert(!HotkeyMigration.TryRewriteLegacyHotkey("# MenuHotkey = F9\n", out migrated), "注释行不应动");
                Assert(!HotkeyMigration.TryRewriteLegacyHotkey("MenuHotkey = LeftControl + F9\n", out migrated), "带修饰键不应动");
                Assert(!HotkeyMigration.TryRewriteLegacyHotkey("", out migrated), "空文本不应动");

                TestCampaign();
                TestPreviewRequest(modsDir);
                TestUserContent();

                Console.WriteLine("--- 扫描信息 ---");
                infos.ForEach(Console.WriteLine);
                Console.WriteLine("--- 坏包/容错警告（预期 5 条，另 1 条注册冲突） ---");
                warnings.ForEach(Console.WriteLine);
                Console.WriteLine("PASS: 2 个好包解析正确（texts.json 含中文/转义文本 + 坏 texts.json 容错 + assets/ 图片读入与超限跳过），4 个坏包/坏文件警告跳过，注册表查找/包查找/冲突处理正确，热键迁移改写正确，campaign 解析/触发器条件（含 when_month/when_stage/when_affinity/disable_official_events）正确。");
                return 0;
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        private static void TestPreviewRequest(string tempDir)
        {
            string path = Path.Combine(tempDir, "preview-request.json");
            File.WriteAllText(path,
                "{\"format\":1,\"mod_id\":\"lom_modkit_preview\",\"script_id\":\"main\",\"node_id\":\"n3\"}");
            PreviewRequest request;
            string error;
            Assert(PreviewRequest.TryRead(path, out request, out error), "合法试玩请求应解析成功：" + error);
            Assert(request.ModId == "lom_modkit_preview" && request.ScriptId == "main" && request.NodeId == "n3",
                "试玩请求字段解析错误");
            File.WriteAllText(path,
                "{\"format\":1,\"mod_id\":\"bad id\",\"script_id\":\"main\",\"node_id\":\"n3\"}");
            Assert(!PreviewRequest.TryRead(path, out request, out error) && error.Contains("mod_id"),
                "含空格的试玩 id 应被拒绝");
        }

        /// <summary>契约 §2 campaign 段：解析、触发器 script 校验、条件判定、位置名映射。独立临时目录不影响上面的断言。</summary>
        private static void TestCampaign()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_campaign_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                // 好包：new_game + disable_official_events + 两个触发器（一个带 when_flag_set + 时间/好感条件，一个带 when_flag_clear）
                WriteZip(Path.Combine(modsDir, "campaign_ok.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"camp\",\"name\":\"战役\",\"version\":\"1.0\",\"entry\":\"main\",\"campaign\":{\"new_game\":true,\"disable_official_events\":true,\"triggers\":[{\"type\":\"position\",\"position\":\"Center\",\"script\":\"train\",\"when_flag_set\":\"F_A\",\"when_month\":4,\"when_stage\":1,\"when_affinity\":{\"character\":\"brother4\",\"min\":3}},{\"type\":\"position\",\"position\":\"Door\",\"script\":\"gate\",\"when_flag_clear\":\"F_B\"}]}}"),
                    ("lua/main.lua", "say(\"m\")"),
                    ("lua/train.lua", "say(\"t\")"),
                    ("lua/gate.lua", "say(\"g\")"));
                // 触发器 script 指向不存在的脚本 → 该触发器丢弃 + 1 警告，包仍加载
                WriteZip(Path.Combine(modsDir, "campaign_badtrigger.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badtrig\",\"entry\":\"main\",\"campaign\":{\"triggers\":[{\"type\":\"position\",\"position\":\"Mall\",\"script\":\"ghost\"}]}}"),
                    ("lua/main.lua", "say(\"m\")"));
                // 未知 trigger type → 整包拒绝 + 1 警告
                WriteZip(Path.Combine(modsDir, "campaign_badtype.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badtype\",\"entry\":\"main\",\"campaign\":{\"triggers\":[{\"type\":\"time\",\"position\":\"Mall\",\"script\":\"main\"}]}}"),
                    ("lua/main.lua", "say(\"m\")"));
                // when_month 越界（13）→ 整包拒绝 + 1 警告（契约 §2.1 校验）
                WriteZip(Path.Combine(modsDir, "campaign_badmonth.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badmonth\",\"entry\":\"main\",\"campaign\":{\"triggers\":[{\"type\":\"position\",\"position\":\"Mall\",\"script\":\"main\",\"when_month\":13}]}}"),
                    ("lua/main.lua", "say(\"m\")"));
                // campaign.new_game 非布尔 → 整包拒绝 + 1 警告
                WriteZip(Path.Combine(modsDir, "campaign_badbool.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badbool\",\"entry\":\"main\",\"campaign\":{\"new_game\":\"yes\"}}"),
                    ("lua/main.lua", "say(\"m\")"));
                // campaign.disable_official_events 非布尔 → 整包拒绝 + 1 警告（契约 §2）
                WriteZip(Path.Combine(modsDir, "campaign_baddisable.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"baddisable\",\"entry\":\"main\",\"campaign\":{\"disable_official_events\":\"yes\"}}"),
                    ("lua/main.lua", "say(\"m\")"));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 2, "应加载 2 个包，实际 " + mods.Count + "：" + string.Join(" | ", warnings));
                Assert(warnings.Count == 5, "应有 5 条警告，实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

                var ok = mods[0].Id == "badtrig" ? mods[1] : mods[0];
                Assert(ok.Id == "camp", "campaign_ok 应加载，实际 " + ok.Id);
                Assert(ok.Campaign != null && ok.Campaign.NewGame, "new_game 解析错误");
                Assert(ok.Campaign.DisableOfficialEvents, "disable_official_events 解析错误（应为 true）");
                Assert(ok.Campaign.Triggers.Count == 2, "应有 2 个触发器，实际 " + ok.Campaign.Triggers.Count);
                var t0 = ok.Campaign.Triggers[0];
                Assert(t0.Position == "Center" && t0.Script == "train" && t0.WhenFlagSet == "F_A" && t0.WhenFlagClear == null,
                    "触发器 0 字段错误：" + t0.Position + "/" + t0.Script + "/" + t0.WhenFlagSet + "/" + t0.WhenFlagClear);
                Assert(t0.WhenMonth == 4, "触发器 0 when_month 应为 4，实际 " + (t0.WhenMonth == null ? "null" : t0.WhenMonth.Value.ToString()));
                Assert(t0.WhenStage == 1, "触发器 0 when_stage 应为 1");
                Assert(t0.WhenAffinity != null && t0.WhenAffinity.Character == "brother4" && t0.WhenAffinity.Min == 3,
                    "触发器 0 when_affinity 解析错误");
                // 无条件触发器的 when_month/when_stage/when_affinity 应为 null（未写字段不解析）
                var t1 = ok.Campaign.Triggers[1];
                Assert(t1.Position == "Door" && t1.Script == "gate" && t1.WhenFlagClear == "F_B" && t1.WhenFlagSet == null,
                    "触发器 1 字段错误");
                Assert(t1.WhenMonth == null && t1.WhenStage == null && t1.WhenAffinity == null,
                    "触发器 1 不应有时间/好感条件");
                Assert(ok.GetRegisteredScriptName(t0.Script) == "MOD_camp_train", "触发器脚本注册名错误");

                var badTrig = mods[0].Id == "badtrig" ? mods[0] : mods[1];
                Assert(badTrig.Campaign != null && badTrig.Campaign.Triggers.Count == 0, "坏触发器应被丢弃");
                Assert(!badTrig.Campaign.NewGame, "未写 new_game 应为 false");
                Assert(!badTrig.Campaign.DisableOfficialEvents, "未写 disable_official_events 应为 false");

                // 无 campaign 段的包 → Campaign 为 null（沿用首个目录的 demo_mod 验证过，这里直接测解析容错）
                var bare = new CampaignTrigger { Position = "Center", Script = "s" };
                var keys = new List<string> { "F_A" };
                // IsConditionMet：无条件恒真；when_flag_set 要求命中；when_flag_clear 要求不命中
                Assert(bare.IsConditionMet(null) && bare.IsConditionMet(keys), "无条件触发器应恒真");
                Assert(t0.IsConditionMet(keys), "when_flag_set 已设置应生效");
                Assert(!t0.IsConditionMet(new List<string>()), "when_flag_set 未设置不应生效");
                Assert(!t0.IsConditionMet(null), "when_flag_set 遇 null 列表不应生效");
                Assert(!t1.IsConditionMet(new List<string> { "F_B" }), "when_flag_clear 已设置不应生效");
                Assert(t1.IsConditionMet(keys) && t1.IsConditionMet(null), "when_flag_clear 未设置应生效");

                // PositionNameMap：中文枚举名 → 契约 id
                Assert(PositionNameMap.ToContractId("校場") == "Center", "校場 映射错误");
                Assert(PositionNameMap.ToContractId("正心堂") == "Mall", "正心堂 映射错误");
                Assert(PositionNameMap.ToContractId("神秘房子") == "Secret", "神秘房子 映射错误");
                Assert(PositionNameMap.ToContractId("無") == null, "無 不应有映射");
                Assert(PositionNameMap.ToContractId("不存在的") == null && PositionNameMap.ToContractId(null) == null, "未知名应返回 null");

                Console.WriteLine("--- campaign 警告（预期 5 条） ---");
                warnings.ForEach(Console.WriteLine);
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        /// <summary>1x1 透明 PNG（契约 §3.1 结局卡背景图冒烟：读入字节逐位对比）。</summary>
        private const string PngBase64 =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

        /// <summary>按契约 §1/§2/§A/§3.1 造一个完整好包（含 story/ 源文件，运行时应忽略它）。</summary>
        private static void WriteGoodPackage(string path)
        {
            WriteZip(path,
                ("manifest.json", "{\"format\":1,\"id\":\"demo_mod\",\"name\":\"示例 Mod\",\"version\":\"1.0.0\",\"author\":\"somebody\",\"description\":\"一句话简介\",\"entry\":\"main\"}"),
                ("story/main.json", "{\"id\":\"main\"}"),
                ("story/extra.json", "{\"id\":\"extra\"}"),
                ("lua/main.lua", "local function node_n1() say(\"你好\") end\nreturn node_n1()"),
                ("lua/extra.lua", "say(\"extra\")"),
                ("texts.json", "{\"MOD_demo_mod_main_n1\": \"你好\", \"MOD_demo_mod_main_n2\": \"第二句\\\"带引号\\\"\"}"));
            // 二进制条目：1 张有效 png、1 张 9MB 超限 png（zip 压缩后很小，但未压缩长度>8MB）、1 个非图片
            AppendBinaryEntries(path,
                ("assets/ending.png", Convert.FromBase64String(PngBase64)),
                ("assets/huge.png", new byte[9 * 1024 * 1024]),
                ("assets/bgm.ogg", new byte[] { 1, 2, 3, 4 }));
        }

        /// <summary>往已存在的 zip 追加二进制条目（Assets 图片测试用）。</summary>
        private static void AppendBinaryEntries(string path, params (string name, byte[] data)[] entries)
        {
            using (var stream = File.Open(path, FileMode.Open))
            using (var zip = new ZipArchive(stream, ZipArchiveMode.Update))
            {
                foreach (var (name, data) in entries)
                {
                    var entry = zip.CreateEntry(name);
                    using (var writer = entry.Open())
                        writer.Write(data, 0, data.Length);
                }
            }
        }

        private static void WriteZip(string path, params (string name, string content)[] entries)
        {
            using (var stream = File.Create(path))
            using (var zip = new ZipArchive(stream, ZipArchiveMode.Create))
            {
                foreach (var (name, content) in entries)
                {
                    var entry = zip.CreateEntry(name);
                    using (var writer = new StreamWriter(entry.Open(), new UTF8Encoding(false)))
                        writer.Write(content);
                }
            }
        }

        /// <summary>用户内容协议与包内解析：合法加载、非法路径拒绝、Mod 间隔离。</summary>
        private static void TestUserContent()
        {
            string error;
            ContentRef parsed;
            Assert(ContentRef.IsUserRef("user:mohui.boss_theme"), "user: 前缀应识别");
            Assert(!ContentRef.IsUserRef("普通_001"), "官方 ID 不应识别为 user:");
            Assert(ContentRef.TryParse("user:mohui.boss_theme", out parsed, out error)
                && parsed.ContentId == "mohui.boss_theme"
                && parsed.Namespace == "mohui"
                && parsed.LocalId == "boss_theme",
                "合法引用解析失败：" + error);
            Assert(!ContentRef.TryParse("user:../evil", out parsed, out error) && error != null,
                "路径穿越 ID 应被拒绝");
            Assert(!ContentRef.TryParse("user:MOHUI.Boss", out parsed, out error),
                "大写 ID 应被拒绝");
            Assert(!ContentRef.TryParse("user:mohui/boss", out parsed, out error),
                "含斜杠 ID 应被拒绝");
            Assert(!ContentRef.IsSafePackageRelative("assets/user/audio/../evil/content.json"),
                "包内穿越路径应拒绝");
            Assert(!ContentRef.IsSafePackageRelative("assets/bgm.ogg"),
                "非 assets/user 路径应拒绝");
            Assert(ContentRef.IsSafePackageRelative("assets/user/audio/mohui.boss_theme/boss_theme.ogg"),
                "合法包内音频路径应接受");
            Assert(ContentRef.IsSafePackageRelative("assets/user/character/mohui.luoxue/normal.png"),
                "合法包内角色立绘路径应接受");

            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_usercontent_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                string contentJson =
                    "{\"schema\":1,\"id\":\"mohui.boss_theme\",\"type\":\"audio\",\"name\":\"决战曲\",\"audio_kind\":\"music\",\"files\":{\"main\":\"boss_theme.ogg\"}}";
                string contentJsonB =
                    "{\"schema\":1,\"id\":\"mohui.boss_theme\",\"type\":\"audio\",\"name\":\"另一首\",\"audio_kind\":\"music\",\"files\":{\"main\":\"boss_theme.ogg\"}}";
                WriteZip(Path.Combine(modsDir, "mod_a.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"mod_a\",\"name\":\"A\",\"version\":\"1\",\"author\":\"t\",\"description\":\"t\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"a\")"),
                    ("assets/user/audio/mohui.boss_theme/content.json", contentJson));
                WriteZip(Path.Combine(modsDir, "mod_b.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"mod_b\",\"name\":\"B\",\"version\":\"1\",\"author\":\"t\",\"description\":\"t\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"b\")"),
                    ("assets/user/audio/mohui.boss_theme/content.json", contentJsonB));
                AppendBinaryEntries(Path.Combine(modsDir, "mod_a.lommod"),
                    ("assets/user/audio/mohui.boss_theme/boss_theme.ogg", new byte[] { 1, 2, 3, 4 }));
                AppendBinaryEntries(Path.Combine(modsDir, "mod_b.lommod"),
                    ("assets/user/audio/mohui.boss_theme/boss_theme.ogg", new byte[] { 9, 9, 9, 9 }),
                    ("assets/user/audio/../evil/content.json", Encoding.UTF8.GetBytes("{\"schema\":1}")),
                    ("assets/user/audio/Bad.Id/content.json", Encoding.UTF8.GetBytes("{\"schema\":1,\"id\":\"Bad.Id\"}")));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 2, "应加载 2 个含用户音频的包，实际 " + mods.Count + "：" + string.Join(" | ", warnings));
                var a = mods[0].Id == "mod_a" ? mods[0] : mods[1];
                var b = mods[0].Id == "mod_b" ? mods[0] : mods[1];
                UserContent ca;
                UserContent cb;
                Assert(a.TryGetUserContent("mohui.boss_theme", out ca) && ca.Bytes != null && ca.Bytes[0] == 1,
                    "mod_a 应解析到自己的音频字节");
                Assert(b.TryGetUserContent("mohui.boss_theme", out cb) && cb.Bytes != null && cb.Bytes[0] == 9,
                    "mod_b 应解析到自己的音频字节（不得串用 mod_a）");
                Assert(ca.Name == "决战曲" && ca.AudioKind == "music", "mod_a metadata 解析错误");
                Assert(!a.Assets.ContainsKey("assets/user/audio/mohui.boss_theme/boss_theme.ogg"),
                    "用户音频不应进入图片 Assets 表");
                Assert(warnings.Exists(w => w.Contains("非法用户内容路径") || w.Contains("Bad.Id") || w.Contains("解析失败")),
                    "非法路径/非法 ID 应产生警告，实际：" + string.Join(" | ", warnings));
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }

            string charDir = Path.Combine(Path.GetTempPath(), "lommod_userchar_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(charDir);
            try
            {
                string charJson =
                    "{\"schema\":1,\"id\":\"mohui.luoxue\",\"type\":\"character\",\"name\":\"洛雪\",\"files\":{\"main\":\"normal.png\"},\"portraits\":{\"normal\":\"normal.png\",\"happy\":\"happy.png\"}}";
                byte[] png = new byte[] {
                    0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A,0x00,0x00,0x00,0x0D,0x49,0x48,0x44,0x52,
                    0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01,0x08,0x02,0x00,0x00,0x00,0x90,0x77,0x53,
                    0xDE,0x00,0x00,0x00,0x0C,0x49,0x44,0x41,0x54,0x78,0x9C,0x63,0xF8,0xCF,0xC0,
                    0x00,0x00,0x00,0x03,0x00,0x01,0x00,0x05,0xFE,0xD4,0xEF,0x00,0x00,0x00,0x00,
                    0x49,0x45,0x4E,0x44,0xAE,0x42,0x60,0x82
                };
                WriteZip(Path.Combine(charDir, "mod_c.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"mod_c\",\"name\":\"C\",\"version\":\"1\",\"author\":\"t\",\"description\":\"t\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"c\")"),
                    ("assets/user/character/mohui.luoxue/content.json", charJson));
                AppendBinaryEntries(Path.Combine(charDir, "mod_c.lommod"),
                    ("assets/user/character/mohui.luoxue/normal.png", png),
                    ("assets/user/character/mohui.luoxue/happy.png", png));
                var charMods = ModLoader.ScanMods(charDir, _ => { }, _ => { });
                Assert(charMods.Count == 1, "应加载含自定义角色的包");
                UserContent ch;
                Assert(charMods[0].TryGetUserContent("mohui.luoxue", out ch)
                    && ch.Type == "character"
                    && ch.Portraits != null
                    && ch.Portraits.ContainsKey("happy")
                    && ch.Files != null
                    && ch.Files.ContainsKey("happy.png"),
                    "自定义角色应解析 portraits 与立绘字节");
            }
            finally
            {
                Directory.Delete(charDir, recursive: true);
            }
        }

        private static void Assert(bool condition, string message)
        {
            if (!condition) throw new Exception("断言失败：" + message);
        }
    }
}
