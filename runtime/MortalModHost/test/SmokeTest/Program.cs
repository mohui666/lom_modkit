using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Security.Cryptography;
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
                string demoPackagePath = Path.Combine(modsDir, "demo_mod.lommod");
                WriteGoodPackage(demoPackagePath);
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
                WriteZip(Path.Combine(modsDir, "future-format.lommod"),
                    ("manifest.json", "{\"format\":1,\"package_format\":2,\"id\":\"future\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"m\")"));
                WriteZip(Path.Combine(modsDir, "future-story.lommod"),
                    ("manifest.json", "{\"format\":1,\"package_format\":1,\"story_schema\":2,\"id\":\"futurestory\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"m\")"));

                var warnings = new List<string>();
                var infos = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, infos.Add, warnings.Add);

                // 应加载出 2 个好包（badtexts 的 texts.json 容错 + demo_mod）
                Assert(mods.Count == 2, "应加载 2 个好包，实际 " + mods.Count);
                var mod = mods.First(m => m.Id == "demo_mod");
                Assert(mod.Id == "demo_mod", "id 解析错误：" + mod.Id);
                Assert(mod.PackageFingerprint == ComputeFileSha256(demoPackagePath),
                    "包指纹必须等于最终 .lommod 原始字节的 SHA-256");
                Assert(mod.PackageFingerprint.Length == 64 && mod.PackageFingerprint.All(IsUpperHex),
                    "包指纹必须是 64 位大写十六进制");
                Assert(mod.Name == "示例 Mod", "name 解析错误（含中文）：" + mod.Name);
                Assert(mod.Version == "1.0.0", "version 解析错误：" + mod.Version);
                Assert(mod.Author == "somebody", "author 解析错误：" + mod.Author);
                Assert(mod.MinHostVersion == "0.5.0" && mod.TestedHostVersion == "0.6.0"
                    && mod.TestedGameVersion == "1.2.3",
                    "兼容性 metadata 解析错误");
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
                Assert(warnings.Count == 7, "应有 7 条坏包警告（含版本拒绝、texts.json 容错 + 超限图片跳过），实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

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
                Assert(warnings.Count == 8, "注册名冲突应新增 1 条警告，实际共 " + warnings.Count + "：" + string.Join(" | ", warnings));

                // 分隔符碰撞也必须按整包拒绝：a_b/c 与 a/b_c 都会生成 MOD_a_b_c。
                var left = new ModPackage { Id = "a_b", Entry = "c" };
                left.LuaScripts["c"] = "say(\"left\")";
                var right = new ModPackage { Id = "a", Entry = "b_c" };
                right.LuaScripts["b_c"] = "say(\"right\")";
                var collisionWarnings = new List<string>();
                ModRegistry.Rebuild(new ModPackage[] { left, right }, collisionWarnings.Add);
                Assert(ModRegistry.Count == 1 && ModRegistry.IsPackageFullyRegistered(left)
                    && !ModRegistry.IsPackageFullyRegistered(right),
                    "跨 id/script 的注册名碰撞必须原子拒绝后加载整包");
                Assert(collisionWarnings.Count == 1 && collisionWarnings[0].Contains("整包忽略"),
                    "原子拒绝注册名碰撞应给出明确警告");

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
                TestLocalization();
                TestRuntimeCompatibility();
                TestRuntimeTrace();
                TestStructuredRuntimeError();
                TestArchiveLimits();
                TestManifestIdentifiers();
                TestPackageFingerprint();
                TestPreviewRequest(modsDir);
                TestUserContent();
                TestDisclosurePolicy();
                TestProvenanceWatermarkProtocol();
                TestProvenanceWatermarkCodec();
                TestGameplaySession();

                Console.WriteLine("--- 扫描信息 ---");
                infos.ForEach(Console.WriteLine);
                Console.WriteLine("--- 坏包/容错警告（预期 7 条，另 1 条注册冲突） ---");
                warnings.ForEach(Console.WriteLine);
                Console.WriteLine("PASS: 2 个好包解析正确（显式 Schema + 运行环境兼容 metadata + texts/assets 容错），6 个坏包/坏文件与未来版本安全跳过，Host/游戏版本硬拒载与软警告、注册表冲突、热键迁移、campaign、非官方剧情披露与结构化 Runtime 错误均正确。");
                return 0;
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        private static void TestRuntimeTrace()
        {
            RuntimeTrace.Reset();
            var preview = new ModPackage
            {
                Id = "lom_modkit_preview", Name = "F5", Entry = "main",
                PackagePath = Path.Combine(Path.GetTempPath(), "__lom_modkit_preview.lommod")
            };
            RuntimeTrace.BeginScript(preview, "MOD_lom_modkit_preview_main");
            Assert(RuntimeTrace.IsDevelopmentPackage(preview), "固定包名和 id 应识别为开发试玩包");
            Assert(!RuntimeTrace.IsDevelopmentPackage(new ModPackage
                { Id = "lom_modkit_preview", PackagePath = "renamed.lommod" }),
                "仅伪造开发 id、但包名不匹配时不得启用开发能力");
            Assert(RuntimeDebugControl.Active && !RuntimeDebugControl.Paused, "F5 调试控制应启用且默认继续运行");
            RuntimeDebugControl.PauseBeforeNextNode();
            Assert(RuntimeDebugControl.PausePending && RuntimeDebugControl.BeforeNode() && RuntimeDebugControl.Paused,
                "Pause 必须在下一节点体执行前生效");
            RuntimeDebugControl.Step();
            Assert(!RuntimeDebugControl.Paused && RuntimeDebugControl.PausePending && RuntimeDebugControl.BeforeNode(),
                "Step 应放行当前节点并在再下一节点前暂停");
            RuntimeDebugControl.Continue();
            Assert(!RuntimeDebugControl.Paused && !RuntimeDebugControl.PausePending, "Continue 应清除暂停请求");
            RuntimeTrace.NodeEnter("c1", "choice");
            RuntimeTrace.Choice("c1", 1, "b1");
            RuntimeTrace.NodeEnter("b1", "branch");
            RuntimeTrace.Condition("b1", "true", "e1");
            RuntimeTrace.NodeEnter("e1", "end");
            RuntimeTrace.RuntimeError("synthetic");
            RuntimeTrace.ReplaceVariables(new Dictionary<string, string> { { "chapter", "2" } });
            RuntimeTrace.ReplaceFlags(new Dictionary<string, string> { { "READY", "true" } });
            var events = RuntimeTrace.Snapshot();
            Assert(RuntimeTrace.Active, "固定 F5 试玩包应启用 trace");
            Assert(events.Exists(item => item.EventType == "mod_enter"), "缺 mod_enter");
            Assert(events.Exists(item => item.EventType == "story_enter" && item.StoryId == "main"), "缺 story_enter");
            Assert(events.Exists(item => item.EventType == "node_enter" && item.NodeId == "c1"), "缺 node_enter");
            Assert(events.Exists(item => item.EventType == "choice"), "缺 choice");
            Assert(events.Exists(item => item.EventType == "condition_result"), "缺 condition_result");
            Assert(events.Exists(item => item.EventType == "goto" && item.Detail == "b1"), "缺推断 goto");
            Assert(events.Exists(item => item.EventType == "end"), "缺 end");
            Assert(events.Exists(item => item.EventType == "runtime_error"), "缺 runtime_error");
            Assert(RuntimeTrace.VariablesSnapshot()["chapter"] == "2", "变量快照错误");
            Assert(RuntimeTrace.FlagsSnapshot()["READY"] == "true", "Flag 快照错误");
            RuntimeTrace.PrepareHotReload("selected_2");
            events = RuntimeTrace.Snapshot();
            Assert(RuntimeTrace.Active && !RuntimeDebugControl.Paused && !RuntimeDebugControl.PausePending,
                "热重载边界必须保持 F5 trace 并解除旧暂停状态");
            Assert(RuntimeTrace.CurrentNode == "", "热重载必须清空旧节点");
            Assert(RuntimeTrace.VariablesSnapshot().Count == 0 && RuntimeTrace.FlagsSnapshot().Count == 0,
                "热重载必须清空旧 Lua 变量/Flag 快照");
            Assert(events.Exists(item => item.EventType == "hot_reload" && item.Detail == "restart=selected_2"),
                "缺少带目标节点的 hot_reload 事件");
            RuntimeTrace.BeginScript(preview, "MOD_lom_modkit_preview_main");
            Assert(RuntimeTrace.Snapshot().Exists(item => item.EventType == "hot_reload"),
                "热重载后的新脚本必须保留有界历史分隔事件");
            for (int i = 0; i < RuntimeTrace.Capacity + 20; i++)
                RuntimeTrace.Record("node_enter", "n" + i, "stress");
            Assert(RuntimeTrace.Snapshot().Count == RuntimeTrace.Capacity, "trace ring buffer 必须有界");

            int retained = RuntimeTrace.Snapshot().Count;
            RuntimeTrace.BeginScript(new ModPackage { Id = "ordinary", Entry = "main", PackagePath = "ordinary.lommod" }, "MOD_ordinary_main");
            RuntimeTrace.Record("node_enter", "should_not_record", "");
            Assert(!RuntimeTrace.Active && !RuntimeDebugControl.Active && RuntimeTrace.Snapshot().Count == retained,
                "普通 Mod 默认不得记录 trace 或改变执行路径");
            RuntimeTrace.Reset();
        }

        private static void TestRuntimeCompatibility()
        {
            var legacy = new ModPackage { Id = "legacy" };
            CompatibilityResult result = RuntimeCompatibility.Evaluate(legacy, "0.6.0", "1.2.3");
            Assert(result.IsCompatible && result.Warnings.Count == 0,
                "旧 manifest 无兼容字段时必须正常工作");

            var supported = new ModPackage
            {
                Id = "supported",
                MinHostVersion = "0.5.0",
                TestedHostVersion = "0.6.0",
                GameVersion = "1.2.3",
                TestedGameVersion = "1.2.3"
            };
            result = RuntimeCompatibility.Evaluate(supported, "0.6.0", "1.2.3");
            Assert(result.IsCompatible && result.Warnings.Count == 0,
                "匹配的 Host/游戏版本声明应通过");

            supported.MinHostVersion = "0.7.0";
            result = RuntimeCompatibility.Evaluate(supported, "0.6.0", "1.2.3");
            Assert(!result.IsCompatible && result.Error.Contains("Host >= 0.7.0"),
                "Host 低于硬门槛必须给明确错误");

            supported.MinHostVersion = "0.5.0";
            supported.TestedHostVersion = "0.5.9";
            supported.GameVersion = null;
            supported.TestedGameVersion = "1.2.2";
            result = RuntimeCompatibility.Evaluate(supported, "0.6.0", "1.2.3");
            Assert(result.IsCompatible && result.Warnings.Count == 2,
                "超出作者测试的 Host/游戏版本应警告但继续加载");

            supported.TestedHostVersion = "0.6.0";
            supported.GameVersion = "1.2.2";
            supported.TestedGameVersion = null;
            result = RuntimeCompatibility.Evaluate(supported, "0.6.0", "1.2.3");
            Assert(!result.IsCompatible && result.Error.Contains("需要游戏版本 1.2.2"),
                "game_version 精确硬门槛不匹配时必须拒绝");

            supported.GameVersion = null;
            supported.MinHostVersion = "not-semver";
            result = RuntimeCompatibility.Evaluate(supported, "0.6.0", "1.2.3");
            Assert(!result.IsCompatible && result.Error.Contains("min_host_version"),
                "手工包的非法兼容字段必须在运行时明确拒绝");
        }

        private sealed class ThrowingToStringException : Exception
        {
            public override string ToString()
            {
                throw new InvalidOperationException("secondary formatter failure");
            }
        }

        private static void TestStructuredRuntimeError()
        {
            RuntimeTrace.Reset();
            RuntimeErrorReporter.ResetForTests();
            var package = new ModPackage
            {
                Id = "ordinary_mod",
                Name = "Ordinary \"Mod\"\nName",
                Version = "2.3.4",
                Entry = "main",
                PackagePath = "ordinary.lommod"
            };
            RuntimeTrace.BeginScript(package, "MOD_ordinary_mod_chapter_2");
            RuntimeTrace.NodeEnter("say_7", "say");
            RuntimeTrace.Choice("choice_1", 2, "say_7");
            Assert(!RuntimeTrace.Active, "普通 Mod 不得启用完整开发 trace");
            Assert(RuntimeTrace.DiagnosticSnapshot(16).Count >= 3,
                "普通 Mod 必须保留有界故障 breadcrumb");

            var logs = new List<string>();
            StructuredRuntimeError report = RuntimeErrorReporter.Report(
                "lua_runtime", "演出失败", new InvalidOperationException("boom\nline"),
                package, "MOD_ordinary_mod_chapter_2", logs.Add);
            Assert(report.ModId == "ordinary_mod" && report.ModName == package.Name,
                "结构化错误缺 mod id/name");
            Assert(report.Version == "2.3.4" && report.Story == "chapter_2"
                && report.Node == "say_7", "结构化错误缺 version/story/node");
            Assert(report.Category == "lua_runtime" && report.Error.Contains("boom"),
                "结构化错误缺 category/error");
            Assert(report.RecentTrace.Length >= 3 && report.RecentTrace.Length <= 16,
                "结构化错误 recent trace 必须存在且有界");
            Assert(logs.Count == 1 && logs[0].StartsWith("[mod-runtime-error] ", StringComparison.Ordinal),
                "结构化错误必须写单条可识别日志");
            var json = (Dictionary<string, object>)MiniJson.Parse(report.ToJson());
            foreach (string key in new[] { "mod_id", "mod_name", "version", "story", "node", "category", "error", "recent_trace" })
                Assert(json.ContainsKey(key), "结构化错误 JSON 缺字段 " + key);
            Assert((string)json["mod_id"] == "ordinary_mod"
                && (string)json["story"] == "chapter_2"
                && (string)json["node"] == "say_7"
                && (string)json["category"] == "lua_runtime",
                "结构化错误 JSON 字段错误");
            Assert(((List<object>)json["recent_trace"]).Count == report.RecentTrace.Length,
                "结构化错误 JSON recent_trace 错误");
            Assert(object.ReferenceEquals(RuntimeErrorReporter.LastSnapshot(), report),
                "最后一条结构化错误必须可供诊断包读取");

            for (int i = 0; i < RuntimeTrace.DiagnosticCapacity + 9; i++)
                RuntimeTrace.Record("node_enter", "bounded_" + i, "say");
            Assert(RuntimeTrace.DiagnosticSnapshot(100).Count == RuntimeTrace.DiagnosticCapacity,
                "普通 Mod diagnostic breadcrumb 必须有界");

            // Exception.ToString 与日志 sink 同时故障时，报告器仍必须返回且不抛。
            StructuredRuntimeError fallback = RuntimeErrorReporter.Report(
                "lua_runtime", new string('x', 10000), new ThrowingToStringException(),
                package, "MOD_ordinary_mod_chapter_2",
                delegate(string ignored) { throw new InvalidOperationException("logger failed"); });
            Assert(fallback != null && fallback.Error.Length <= 8193
                && MiniJson.Parse(fallback.ToJson()) is Dictionary<string, object>,
                "错误格式化/日志二次失败不得让报告器崩溃");

            // 未注册脚本不能错误继承上一 Mod 的身份、节点或 breadcrumb。
            StructuredRuntimeError missing = RuntimeErrorReporter.Report(
                "script_lookup", "missing", null, null, "MOD_deleted_main");
            Assert(missing.ModId == "" && missing.ModName == "" && missing.Version == ""
                && missing.Story == "MOD_deleted_main" && missing.Node == ""
                && missing.RecentTrace.Length == 0,
                "未注册脚本结构化错误不得串用上一 Mod 上下文");
            StructuredRuntimeError mismatched = RuntimeErrorReporter.Report(
                "script_setup", "broken setup", null, null, "MOD_another_main");
            Assert(mismatched.ModId == "" && mismatched.Node == ""
                && mismatched.RecentTrace.Length == 0,
                "无法核对注册名时不得串用上一 Mod 上下文");
            RuntimeTrace.Reset();
        }

        /// <summary>Story 本地化包：请求语言 → fallback → default → legacy，且注册表无需重扫。</summary>
        private static void TestLocalization()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_localization_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                WriteZip(Path.Combine(modsDir, "localized.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"localized\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"legacy\")"),
                    ("texts.json", "{\"MOD_localized_main_s1\":\"legacy\"}"),
                    ("localization.json", "{\"schema\":1,\"default_locale\":\"zh_CN\",\"fallback_locale\":\"zh_TW\",\"locales\":[\"zh_CN\",\"zh_TW\",\"ja\",\"ko\"]}"),
                    ("lua/zh_CN/main.lua", "say(\"简中\")"),
                    ("lua/zh_TW/main.lua", "say(\"繁中\")"),
                    ("lua/ja/main.lua", "say(\"日本語\")"),
                    ("lua/ko/main.lua", "say(\"한국어\")"),
                    ("texts/zh_CN.json", "{\"MOD_localized_main_s1\":\"简中\"}"),
                    ("texts/zh_TW.json", "{\"MOD_localized_main_s1\":\"繁中\"}"),
                    ("texts/ja.json", "{\"MOD_localized_main_s1\":\"日本語\"}"),
                    ("texts/ko.json", "{\"MOD_localized_main_s1\":\"한국어\"}"));
                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 1 && warnings.Count == 0, "本地化包应无警告加载：" + string.Join(" | ", warnings));
                var mod = mods[0];
                Assert(mod.DefaultLocale == "zh_CN" && mod.FallbackLocale == "zh_TW", "locale 元数据解析错误");
                Assert(mod.GetLuaScript("main", "ja").Contains("日本語"), "ja Lua 选择错误");
                Assert(mod.GetLuaScript("main", "fr").Contains("繁中"), "未知语言应选择 fallback Lua");
                Assert(mod.GetTexts("ko")["MOD_localized_main_s1"] == "한국어", "ko texts 选择错误");
                ModRegistry.Rebuild(mods);
                string lua;
                I18n.CurrentStoryLocale = "ja";
                Assert(ModRegistry.TryGetLuaByRegisteredName("MOD_localized_main", out lua) && lua.Contains("日本語"), "注册表首次语言选择错误");
                I18n.CurrentStoryLocale = "ko";
                Assert(ModRegistry.TryGetLuaByRegisteredName("MOD_localized_main", out lua) && lua.Contains("한국어"), "切换语言后不重扫也应选择新脚本");
                I18n.CurrentStoryLocale = "zh_CN";

                WriteZip(Path.Combine(modsDir, "incomplete.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"incomplete\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"legacy-safe\")"),
                    ("texts.json", "{}"),
                    ("localization.json", "{\"schema\":1,\"default_locale\":\"zh_CN\",\"fallback_locale\":\"zh_CN\"}"),
                    ("lua/zh_CN/main.lua", "say(\"partial\")"),
                    ("texts/zh_CN.json", "{}"));
                warnings.Clear();
                mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                var incomplete = mods.First(item => item.Id == "incomplete");
                Assert(incomplete.LocalizedLuaScripts.Count == 0 && incomplete.GetLuaScript("main", "ja").Contains("legacy-safe"),
                    "不完整 locale 资源必须整组回退 legacy");
                Assert(warnings.Exists(item => item.Contains("incomplete") && item.Contains("已回退默认语言")),
                    "不完整 locale 资源应明确告警");
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
                    ("manifest.json", "{\"format\":1,\"id\":\"camp\",\"name\":\"战役\",\"version\":\"1.0\",\"entry\":\"main\",\"campaign\":{\"new_game\":true,\"disable_official_events\":true,\"triggers\":[{\"type\":\"position\",\"position\":\"Center\",\"script\":\"train\",\"when_flag_set\":\"F_A\",\"when_month\":4,\"when_stage\":1,\"when_affinity\":{\"character\":\"brother4\",\"min\":-3}},{\"type\":\"position\",\"position\":\"Door\",\"script\":\"gate\",\"when_flag_clear\":\"F_B\"}]}}"),
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
                WriteZip(Path.Combine(modsDir, "campaign_fractionalaffinity.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"fractional\",\"entry\":\"main\",\"campaign\":{\"triggers\":[{\"type\":\"position\",\"position\":\"Mall\",\"script\":\"main\",\"when_affinity\":{\"character\":\"brother4\",\"min\":1.5}}]}}"),
                    ("lua/main.lua", "say(\"m\")"));
                WriteZip(Path.Combine(modsDir, "campaign_overflowaffinity.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"overflow\",\"entry\":\"main\",\"campaign\":{\"triggers\":[{\"type\":\"position\",\"position\":\"Mall\",\"script\":\"main\",\"when_affinity\":{\"character\":\"brother4\",\"min\":2147483648}}]}}"),
                    ("lua/main.lua", "say(\"m\")"));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 2, "应加载 2 个包，实际 " + mods.Count + "：" + string.Join(" | ", warnings));
                Assert(warnings.Count == 7, "应有 7 条警告，实际 " + warnings.Count + "：" + string.Join(" | ", warnings));

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
                Assert(t0.WhenAffinity != null && t0.WhenAffinity.Character == "brother4" && t0.WhenAffinity.Min == -3,
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

                Console.WriteLine("--- campaign 警告（预期 7 条） ---");
                warnings.ForEach(Console.WriteLine);
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        private static void TestArchiveLimits()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_limits_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                string tooMany = Path.Combine(modsDir, "too_many.lommod");
                using (var stream = File.Create(tooMany))
                using (var zip = new ZipArchive(stream, ZipArchiveMode.Create))
                {
                    for (int i = 0; i < 2049; i++)
                        zip.CreateEntry("empty/" + i);
                }

                string hugeText = Path.Combine(modsDir, "huge_text.lommod");
                using (var stream = File.Create(hugeText))
                using (var zip = new ZipArchive(stream, ZipArchiveMode.Create))
                {
                    var manifest = zip.CreateEntry("manifest.json");
                    using (var writer = new StreamWriter(manifest.Open(), new UTF8Encoding(false)))
                        writer.Write("{\"format\":1,\"id\":\"huge\",\"entry\":\"main\"}");
                    var lua = zip.CreateEntry("lua/main.lua", CompressionLevel.Optimal);
                    using (var output = lua.Open())
                    {
                        byte[] block = new byte[8192];
                        for (int i = 0; i < 513; i++) output.Write(block, 0, block.Length);
                    }
                }

                string deepJson = Path.Combine(modsDir, "deep_json.lommod");
                string nested = new string('[', 129) + "0" + new string(']', 129);
                WriteZip(deepJson,
                    ("manifest.json", "{\"format\":1,\"id\":\"deep\",\"entry\":\"main\",\"extra\":" + nested + "}"),
                    ("lua/main.lua", "say(\"m\")"));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 0, "超限包不应加载");
                Assert(warnings.Count == 3, "三种包结构上限应各产生一条警告：" + string.Join(" | ", warnings));
                Assert(warnings.Any(x => x.Contains("条目数超过")), "应报告条目数上限");
                Assert(warnings.Any(x => x.Contains("文本条目超过")), "应报告文本条目上限");
                Assert(warnings.Any(x => x.Contains("JSON 嵌套层数超过")), "应报告 JSON 嵌套上限");
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        private static void TestManifestIdentifiers()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_ids_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                WriteZip(Path.Combine(modsDir, "bad_id.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"Official/../fake\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"x\")"));
                WriteZip(Path.Combine(modsDir, "bad_entry.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"safe_id\",\"entry\":\"../main\"}"),
                    ("lua/main.lua", "say(\"x\")"));
                WriteZip(Path.Combine(modsDir, "bad_script.lommod"),
                    ("manifest.json", "{\"format\":1,\"id\":\"safe_id2\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"x\")"),
                    ("lua/bad name.lua", "say(\"y\")"));

                var warnings = new List<string>();
                var mods = ModLoader.ScanMods(modsDir, _ => { }, warnings.Add);
                Assert(mods.Count == 0, "运行时必须拒绝绕过编译器的非法 id/entry/script id");
                Assert(warnings.Count == 3, "三个非法标识包应各报告一次：" + string.Join(" | ", warnings));
            }
            finally
            {
                Directory.Delete(modsDir, recursive: true);
            }
        }

        private static void TestPackageFingerprint()
        {
            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_sha_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                string first = Path.Combine(modsDir, "a.lommod");
                WriteZip(first,
                    ("manifest.json", "{\"format\":1,\"id\":\"sha_a\",\"name\":\"A\",\"entry\":\"main\",\"official\":true,\"verified\":true,\"sha256\":\"FAKE\"}"),
                    ("lua/main.lua", "say(\"one\")"));
                string copied = Path.Combine(modsDir, "b.lommod");
                File.Copy(first, copied);
                string changed = Path.Combine(modsDir, "c.lommod");
                WriteZip(changed,
                    ("manifest.json", "{\"format\":1,\"id\":\"sha_c\",\"name\":\"C\",\"entry\":\"main\"}"),
                    ("lua/main.lua", "say(\"two\")"));

                var mods = ModLoader.ScanMods(modsDir, _ => { }, _ => { });
                Assert(mods.Count == 3, "指纹测试包应全部加载");
                ModPackage a = mods.First(x => x.Id == "sha_a");
                ModPackage b = mods.First(x => x.PackagePath.EndsWith("b.lommod", StringComparison.OrdinalIgnoreCase));
                ModPackage c = mods.First(x => x.Id == "sha_c");
                Assert(a.PackageFingerprint == b.PackageFingerprint, "逐字节复制或改文件名不能改变内容指纹");
                Assert(a.PackageFingerprint != c.PackageFingerprint, "包内容改变后指纹必须改变");
                Assert(a.PackageFingerprint == ComputeFileSha256(first), "Host 指纹应与独立 SHA-256 计算一致");
                Assert(a.PackageFingerprint != "FAKE", "manifest 自报 official/verified/sha256 不得覆盖 Host 指纹");
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
                ("manifest.json", "{\"format\":1,\"package_format\":1,\"story_schema\":1,\"content_schema\":1,\"id\":\"demo_mod\",\"name\":\"示例 Mod\",\"version\":\"1.0.0\",\"author\":\"somebody\",\"description\":\"一句话简介\",\"entry\":\"main\",\"min_host_version\":\"0.5.0\",\"tested_host_version\":\"0.6.0\",\"tested_game_version\":\"1.2.3\"}"),
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
            Assert(ContentRef.IsSafePackageRelative("assets/user/image/mohui.moon_bg/moon.jpg"),
                "合法包内统一图片路径应接受");

            string modsDir = Path.Combine(Path.GetTempPath(), "lommod_usercontent_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(modsDir);
            try
            {
                string contentJson =
                    "{\"schema\":1,\"content_schema\":1,\"id\":\"mohui.boss_theme\",\"type\":\"audio\",\"name\":\"决战曲\",\"audio_kind\":\"music\",\"files\":{\"main\":\"boss_theme.ogg\"},\"character\":\"user:mohui.luoxue\"}";
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
                    "{\"schema\":1,\"content_schema\":1,\"id\":\"mohui.luoxue\",\"type\":\"character\",\"name\":\"洛雪\",\"scale\":80,\"art_facing\":\"right\",\"files\":{\"main\":\"normal.png\"},\"portraits\":{\"normal\":\"normal.png\",\"happy\":\"happy.png\"}}";
                string imageJson =
                    "{\"schema\":1,\"content_schema\":1,\"id\":\"mohui.moon_bg\",\"type\":\"image\",\"name\":\"月夜\",\"files\":{\"main\":\"moon.jpg\"}}";
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
                    ("assets/user/character/mohui.luoxue/content.json", charJson),
                    ("assets/user/image/mohui.moon_bg/content.json", imageJson));
                AppendBinaryEntries(Path.Combine(charDir, "mod_c.lommod"),
                    ("assets/user/character/mohui.luoxue/normal.png", png),
                    ("assets/user/character/mohui.luoxue/happy.png", png),
                    ("assets/user/image/mohui.moon_bg/moon.jpg", png));
                var charMods = ModLoader.ScanMods(charDir, _ => { }, _ => { });
                Assert(charMods.Count == 1, "应加载含自定义角色的包");
                UserContent ch;
                Assert(charMods[0].TryGetUserContent("mohui.luoxue", out ch)
                    && ch.Type == "character"
                    && ch.Portraits != null
                    && ch.Portraits.ContainsKey("happy")
                    && ch.Files != null
                    && ch.Files.ContainsKey("happy.png")
                    && ch.Scale == 80
                    && ch.ArtFacing == "right",
                    "自定义角色应解析 portraits、立绘字节、体型与原图朝向");
                UserContent image;
                Assert(charMods[0].TryGetUserContent("mohui.moon_bg", out image)
                    && image.Type == "image"
                    && image.Bytes != null
                    && image.Bytes.Length == png.Length
                    && image.PackagePath == "assets/user/image/mohui.moon_bg/moon.jpg",
                    "统一图片应从当前 ModPackage 读取 metadata、路径与字节");
                Assert(object.ReferenceEquals(
                        ch.Files["normal.png"],
                        charMods[0].Assets["assets/user/character/mohui.luoxue/normal.png"])
                    && object.ReferenceEquals(
                        ch.Files["happy.png"],
                        charMods[0].Assets["assets/user/character/mohui.luoxue/happy.png"]),
                    "用户角色图片在 Assets 与 Files 间应复用同一字节数组，不能重复占用内存");
            }
            finally
            {
                Directory.Delete(charDir, recursive: true);
            }
        }

        /// <summary>
        /// 可见披露只在官方枢纽（Title / Free）关掉；Loading 必须保持，否则进死亡/结局卡时标已经没了。
        /// </summary>
        private static void TestDisclosurePolicy()
        {
            Assert(ModDisclosurePolicy.ShouldKeepOnScene("Story"), "Story 应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene("GameOver"), "GameOver 应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene("End"), "End 应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene("Loading"), "Loading 应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene("Battle"), "Battle 应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene(null), "未知场景应保持披露");
            Assert(ModDisclosurePolicy.ShouldKeepOnScene(""), "空场景名应保持披露");
            Assert(!ModDisclosurePolicy.ShouldKeepOnScene("Title"), "Title 应关闭披露");
            Assert(!ModDisclosurePolicy.ShouldKeepOnScene("Free"), "Free 应关闭披露");
            Assert(ModDisclosurePolicy.ShouldDeferHostDisable(true), "披露活动时必须延迟总开关禁用");
            Assert(!ModDisclosurePolicy.ShouldDeferHostDisable(false), "无披露时应立即允许禁用");
            Assert(ModDisclosurePolicy.LabelKey == "disclosure.label", "文案 key 应变");

            var previewIdentity = new ModPackage
            {
                Id = "lom_modkit_preview",
                PackagePath = Path.Combine(Path.GetTempPath(), "__lom_modkit_preview.lommod")
            };
            Assert(ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    true, true, "lom_modkit_preview", previewIdentity),
                "只有活动披露与 F5 trace 同时命中时才可更新固定试玩包身份");
            Assert(!ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    false, true, "lom_modkit_preview", previewIdentity)
                && !ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    true, false, "lom_modkit_preview", previewIdentity)
                && !ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    true, true, "other", previewIdentity),
                "缺少活动披露、F5 trace 或同一开发 id 时必须拒绝身份替换");
            previewIdentity.PackagePath = "renamed.lommod";
            Assert(!ModDisclosurePolicy.CanReplaceDevelopmentPreview(
                    true, true, "lom_modkit_preview", previewIdentity),
                "改名包不得借 F5 热重载切换活动披露身份");

            const string fingerprint = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
            Assert(ModDisclosurePolicy.IsValidPackageFingerprint(fingerprint), "64 位十六进制 SHA-256 应合法");
            Assert(!ModDisclosurePolicy.IsValidPackageFingerprint("abcdef")
                && !ModDisclosurePolicy.IsValidPackageFingerprint(new string('Z', 64)),
                "短指纹和非十六进制指纹必须拒绝");
            Assert(ModDisclosurePolicy.ShortFingerprint(fingerprint) == "ABCDEF0123456789",
                "界面必须显示 SHA-256 前 16 位，不能退回过短指纹");

            string dirty = "  ＜color=red＞官\r\n\t方\0\u202E\u2066\u200B  名＜/color＞  ";
            string clean = ModDisclosurePolicy.SanitizeDisplayText(dirty, 40);
            Assert(!clean.Contains('<') && !clean.Contains('>') && !clean.Any(char.IsControl)
                && !clean.Any(c => CharUnicodeInfo.GetUnicodeCategory(c) == UnicodeCategory.Format)
                && clean.IndexOf('\r') < 0 && clean.IndexOf('\n') < 0,
                "显示身份必须清除富文本尖括号、换行、控制与双向/零宽格式字符：" + clean);

            string emoji = ModDisclosurePolicy.SanitizeDisplayText("A😀B", 3);
            Assert(emoji == "A😀B", "合法 emoji 代理对必须完整保留：" + emoji);
            new UTF8Encoding(false, true).GetBytes(emoji); // 孤立代理会抛异常
            string supplementaryFormat = char.ConvertFromUtf32(0xE0001); // LANGUAGE TAG，Unicode Cf
            Assert(ModDisclosurePolicy.SanitizeDisplayText("A" + supplementaryFormat + "B", 3) == "AB",
                "辅助平面 Format 字符也必须被移除");
            string truncated = ModDisclosurePolicy.SanitizeDisplayText(new string('名', 40), 5);
            Assert(truncated == "名名名名名…", "超长显示字段应按文本元素截断并加省略号：" + truncated);

            var hostile = new ModPackage
            {
                Id = "safe_id",
                Name = "\r\n\u202E＜b＞官方续作＜/b＞",
                Author = "作者\n\u2066伪造",
                PackageFingerprint = fingerprint
            };
            Assert(ModDisclosurePolicy.SafePackageName(hostile) == "b官方续作/b",
                "名称只可作为清洗后的次要身份显示");
            Assert(ModDisclosurePolicy.SafePackageAuthor(hostile) == "作者 伪造",
                "作者自报字段必须单行并移除方向控制字符");
            hostile.Name = " \r\n ";
            Assert(ModDisclosurePolicy.SafePackageName(hostile) == "safe_id",
                "空名称必须回退到已验证的 mod id");
        }

        private static void TestProvenanceWatermarkProtocol()
        {
            const string goldenPacket = "4C4F4D5701010000720435D441F942141A10BE8AA833C8741C08EE6D";
            const string goldenHash = "720435D441F942141A10BE8AA833C874";
            byte[] encoded = ProvenanceWatermarkProtocol.Encode("demo_mod", 1);
            Assert(BitConverter.ToString(encoded).Replace("-", "") == goldenPacket,
                "水印协议 C#/Python 黄金 payload 必须一致");
            Assert(BitConverter.ToString(ProvenanceWatermarkProtocol.HashModId("demo_mod"))
                    .Replace("-", "") == goldenHash,
                "水印协议 C#/Python mod_id hash 必须一致");
            ProvenanceWatermarkProtocol.Packet packet;
            string error;
            Assert(ProvenanceWatermarkProtocol.TryDecode(encoded, out packet, out error)
                    && packet.Protocol == 1 && packet.Algorithm == 1
                    && packet.ModIdHashHex == goldenHash && packet.ChecksumValid,
                "合法水印 payload 应通过结构与 CRC-32 校验：" + error);
            byte[] bits = ProvenanceWatermarkProtocol.ToBits(encoded);
            Assert(bits.Length == 224 && bits[0] == 0 && bits[1] == 1,
                "水印 bit 序必须固定为逐字节 MSB-first");
            Assert(ProvenanceWatermarkProtocol.FromBits(bits).SequenceEqual(encoded),
                "水印 payload 与 bit 序列必须无损往返");

            byte[] corrupted = (byte[])encoded.Clone();
            corrupted[12] ^= 1;
            Assert(ProvenanceWatermarkProtocol.TryParse(corrupted, out packet, out error)
                    && packet != null && !packet.ChecksumValid,
                "结构解析应保留 CRC 失败状态供离线检测器报告");
            Assert(!ProvenanceWatermarkProtocol.TryDecode(corrupted, out packet, out error)
                    && error.Contains("CRC-32"),
                "严格解码必须拒绝 CRC 损坏 payload");
            corrupted = (byte[])encoded.Clone();
            corrupted[0] = 0;
            Assert(!ProvenanceWatermarkProtocol.TryParse(corrupted, out packet, out error)
                    && error.Contains("magic"),
                "错误 magic 必须拒绝");
            bool invalidIdRejected = false;
            try { ProvenanceWatermarkProtocol.Encode("Official.Mod", 1); }
            catch (ArgumentException) { invalidIdRejected = true; }
            Assert(invalidIdRejected, "非法 mod id 不得进入水印身份哈希");
        }

        private static void TestProvenanceWatermarkCodec()
        {
            byte[] packet = ProvenanceWatermarkProtocol.Encode("demo_mod", 1);
            byte[] encoded = ProvenanceWatermarkCodec.HammingEncode(packet);
            for (int offset = 0; offset < encoded.Length; offset += 7)
                encoded[offset + (offset / 7) % 7] ^= 1;
            int corrections;
            byte[] decoded = ProvenanceWatermarkCodec.HammingDecode(encoded, out corrections);
            Assert(decoded.SequenceEqual(packet) && corrections == 56,
                "Hamming(7,4) 必须修正每个 codeword 的单 bit 错误");

            int[] cells;
            sbyte[] polarity;
            ProvenanceWatermarkCodec.CarrierLayout(out cells, out polarity);
            Assert(cells.Length == 392 && cells.Distinct().Count() == 392
                    && cells.Take(8).SequenceEqual(new[] { 388, 301, 111, 85, 164, 305, 22, 72 }),
                "C#/Python keyed PRNG 载波排列必须一致");
            sbyte[] signs = ProvenanceWatermarkCodec.CarrierSigns(packet);
            byte[] recovered = ProvenanceWatermarkCodec.RecoverEccBits(signs);
            decoded = ProvenanceWatermarkCodec.HammingDecode(recovered, out corrections);
            Assert(decoded.SequenceEqual(packet) && corrections == 0,
                "空间载波映射必须无损往返 ECC payload");

            byte[] tile = ProvenanceWatermarkCodec.BuildTileRgba(packet);
            string tileHash;
            using (var sha = SHA256.Create())
                tileHash = string.Concat(sha.ComputeHash(tile).Select(b => b.ToString("X2")));
            Assert(tile.Length == 448 * 224 * 4
                    && tileHash == "D075861FB031C39D390AD27C45C4FF3B858E7804CC0BC8510E3B75D5AA68831C",
                "C#/Python 中频 RGBA tile 黄金向量必须一致");
        }

        private static void TestGameplaySession()
        {
            var package = new ModPackage
            {
                Id = "demo_mod", Entry = "main",
                PackageFingerprint = new string('A', 64)
            };
            GameplaySession.Reset();
            Assert(!GameplaySession.HasPending && GameplaySession.LastResult == "",
                "Gameplay 初始态必须为空");
            GameplaySession.Prepare(package, "combat", "main", "fight1", "win1", "lose1");
            Assert(GameplaySession.PendingCombat && GameplaySession.ShouldForceCombatReturn,
                "combat 准备后必须进入有所有者的待决态");
            Assert(GameplaySession.ConsumeResume(package, "main") == "",
                "原版战斗尚未回报结果时不得提前续接");
            Assert(GameplaySession.RecordResult("combat", "win"), "应接受原版 Combat win");
            Assert(GameplaySession.LastKind == "combat" && GameplaySession.LastResult == "win",
                "必须保留最后一次真实结果");
            Assert(GameplaySession.ConsumeResume(package, "main") == "win1"
                    && !GameplaySession.HasPending,
                "win 结果必须一次性映射到作者目标并清除待决态");
            Assert(GameplaySession.ConsumeResume(package, "main") == "",
                "同一结果不得被重复消费");

            GameplaySession.Prepare(package, "combat", "main", "fight2", "win2", "lose2");
            Assert(GameplaySession.RecordResult("combat", "lose")
                    && GameplaySession.ConsumeResume(package, "main") == "lose2",
                "lose 结果必须映射到失败目标");

            GameplaySession.Prepare(package, "combat", "main", "fight3", "win3", "lose3");
            Assert(GameplaySession.RecordResult("combat", "win"), "第二次 win 应可记录");
            bool rejected = false;
            try
            {
                GameplaySession.ConsumeResume(new ModPackage
                {
                    Id = package.Id, Entry = "main", PackageFingerprint = new string('B', 64)
                }, "main");
            }
            catch (InvalidOperationException) { rejected = true; }
            Assert(rejected && GameplaySession.HasPending,
                "同 id 不同完整 SHA-256 的包不得消费旧包战斗结果");
            GameplaySession.Reset();
        }

        private static bool IsUpperHex(char c)
        {
            return (c >= '0' && c <= '9') || (c >= 'A' && c <= 'F');
        }

        private static string ComputeFileSha256(string path)
        {
            using (var stream = File.OpenRead(path))
            using (var sha = SHA256.Create())
                return string.Concat(sha.ComputeHash(stream).Select(b => b.ToString("X2")));
        }

        private static void Assert(bool condition, string message)
        {
            if (!condition) throw new Exception("断言失败：" + message);
        }
    }
}
