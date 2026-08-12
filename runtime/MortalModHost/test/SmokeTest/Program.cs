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

                var warnings = new List<string>();
                var infos = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, infos.Add, warnings.Add);

                // 只应加载出 demo_mod 一个好包
                Assert(mods.Count == 1, "应加载 1 个好包，实际 " + mods.Count);
                var mod = mods[0];
                Assert(mod.Id == "demo_mod", "id 解析错误：" + mod.Id);
                Assert(mod.Name == "示例 Mod", "name 解析错误（含中文）：" + mod.Name);
                Assert(mod.Version == "1.0.0", "version 解析错误：" + mod.Version);
                Assert(mod.Author == "somebody", "author 解析错误：" + mod.Author);
                Assert(mod.Entry == "main", "entry 解析错误：" + mod.Entry);
                Assert(mod.LuaScripts.Count == 2, "应有 2 个 lua 脚本，实际 " + mod.LuaScripts.Count);
                Assert(mod.LuaScripts.ContainsKey("main") && mod.LuaScripts.ContainsKey("extra"), "脚本 id 清单错误");
                Assert(mod.LuaScripts["main"].Contains("node_n1"), "lua 内容未读入内存");
                Assert(mod.GetRegisteredScriptName("main") == "MOD_demo_mod_main", "注册名前缀错误：" + mod.GetRegisteredScriptName("main"));
                Assert(warnings.Count == 3, "应有 3 条坏包警告，实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

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
                Assert(ModRegistry.Count == 2, "注册表应含 2 个脚本，实际 " + ModRegistry.Count);
                Assert(warnings.Count == 4, "注册名冲突应新增 1 条警告，实际共 " + warnings.Count + "：" + string.Join(" | ", warnings));

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

                Console.WriteLine("--- 扫描信息 ---");
                infos.ForEach(Console.WriteLine);
                Console.WriteLine("--- 坏包警告（预期 3 条） ---");
                warnings.ForEach(Console.WriteLine);
                Console.WriteLine("PASS: 1 个好包解析正确，3 个坏包均警告跳过，注册表查找/冲突处理正确，热键迁移改写正确，campaign 解析/触发器条件正确。");
                return 0;
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        /// <summary>契约 §2 campaign 段：解析、触发器 script 校验、条件判定、位置名映射。独立临时目录不影响上面的断言。</summary>
        private static void TestCampaign()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_campaign_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                // 好包：new_game + 两个触发器（一个带 when_flag_set，一个带 when_flag_clear）
                WriteZip(Path.Combine(modsDir, "campaign_ok.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"camp\",\"name\":\"战役\",\"version\":\"1.0\",\"entry\":\"main\",\"campaign\":{\"new_game\":true,\"triggers\":[{\"type\":\"position\",\"position\":\"Center\",\"script\":\"train\",\"when_flag_set\":\"F_A\"},{\"type\":\"position\",\"position\":\"Door\",\"script\":\"gate\",\"when_flag_clear\":\"F_B\"}]}}"),
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
                // campaign.new_game 非布尔 → 整包拒绝 + 1 警告
                WriteZip(Path.Combine(modsDir, "campaign_badbool.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"badbool\",\"entry\":\"main\",\"campaign\":{\"new_game\":\"yes\"}}"),
                    ("lua/main.lua", "say(\"m\")"));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 2, "应加载 2 个包，实际 " + mods.Count + "：" + string.Join(" | ", warnings));
                Assert(warnings.Count == 3, "应有 3 条警告，实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

                var ok = mods[0].Id == "badtrig" ? mods[1] : mods[0];
                Assert(ok.Id == "camp", "campaign_ok 应加载，实际 " + ok.Id);
                Assert(ok.Campaign != null && ok.Campaign.NewGame, "new_game 解析错误");
                Assert(ok.Campaign.Triggers.Count == 2, "应有 2 个触发器，实际 " + ok.Campaign.Triggers.Count);
                var t0 = ok.Campaign.Triggers[0];
                Assert(t0.Position == "Center" && t0.Script == "train" && t0.WhenFlagSet == "F_A" && t0.WhenFlagClear == null,
                    "触发器 0 字段错误：" + t0.Position + "/" + t0.Script + "/" + t0.WhenFlagSet + "/" + t0.WhenFlagClear);
                var t1 = ok.Campaign.Triggers[1];
                Assert(t1.Position == "Door" && t1.Script == "gate" && t1.WhenFlagClear == "F_B" && t1.WhenFlagSet == null,
                    "触发器 1 字段错误");
                Assert(ok.GetRegisteredScriptName(t0.Script) == "MOD_camp_train", "触发器脚本注册名错误");

                var badTrig = mods[0].Id == "badtrig" ? mods[0] : mods[1];
                Assert(badTrig.Campaign != null && badTrig.Campaign.Triggers.Count == 0, "坏触发器应被丢弃");
                Assert(!badTrig.Campaign.NewGame, "未写 new_game 应为 false");

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

                Console.WriteLine("--- campaign 警告（预期 3 条） ---");
                warnings.ForEach(Console.WriteLine);
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        /// <summary>按契约 §1/§2 造一个完整好包（含 story/ 源文件，运行时应忽略它）。</summary>
        private static void WriteGoodPackage(string path)
        {
            WriteZip(path,
                ("manifest.json", "{\"format\":1,\"id\":\"demo_mod\",\"name\":\"示例 Mod\",\"version\":\"1.0.0\",\"author\":\"somebody\",\"description\":\"一句话简介\",\"entry\":\"main\"}"),
                ("story/main.json", "{\"id\":\"main\"}"),
                ("story/extra.json", "{\"id\":\"extra\"}"),
                ("lua/main.lua", "local function node_n1() say(\"你好\") end\nreturn node_n1()"),
                ("lua/extra.lua", "say(\"extra\")"));
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

        private static void Assert(bool condition, string message)
        {
            if (!condition) throw new Exception("断言失败：" + message);
        }
    }
}
