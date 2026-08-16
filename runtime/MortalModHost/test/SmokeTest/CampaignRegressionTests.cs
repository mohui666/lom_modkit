using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;

namespace MortalModHost
{
    internal static class CampaignRegressionTests
    {
        internal static void Run()
        {
            TestLegacyPackageRejected();
            TestIdentityAndSaveNamespace();
            TestSelectionDoesNotStartCampaign();
            TestRecentCampaignOnlyUsesLoadedPackages();
            TestBattleNamedCharactersAreIncludedInTotal();
            TestCombatAnimationFallback();
        }

        private static void TestLegacyPackageRejected()
        {
            string root = Path.Combine(
                Path.GetTempPath(), "lom_campaign_legacy_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                string packagePath = Path.Combine(root, "legacy-v2.lommod");
                using (var stream = new FileStream(packagePath, FileMode.CreateNew))
                using (var zip = new ZipArchive(stream, ZipArchiveMode.Create))
                {
                    Write(zip, "manifest.json",
                        "{\"format\":2,\"package_format\":2,\"story_schema\":1,"
                        + "\"content_schema\":1,\"id\":\"legacy\",\"entry\":\"main\"}");
                    Write(zip, "lua/main.lua", "return\n");
                }
                var warnings = new List<string>();
                List<ModPackage> loaded = ModLoader.ScanMods(root, null, warnings.Add);
                Assert(loaded.Count == 0, "v1/v2 包不得被 1.0.1 Runtime 静默加载");
                Assert(warnings.Count == 1
                        && warnings[0].IndexOf("package_format", StringComparison.OrdinalIgnoreCase) >= 0,
                    "旧包拒绝必须给出明确的 package_format 重导出提示");
            }
            finally
            {
                Directory.Delete(root, true);
            }
        }

        private static void TestIdentityAndSaveNamespace()
        {
            Assert(CampaignIdentity.IsValid("campaign-01")
                    && !CampaignIdentity.IsValid("Campaign-01")
                    && !CampaignIdentity.IsValid("../campaign"),
                "campaign_id 必须严格匹配小写安全标识符");
            const string campaignId = "campaign-01";
            string main = CampaignIdentity.SaveSlot(campaignId);
            Assert(main == "mod_campaign_campaign-01"
                    && CampaignIdentity.OwnsSlot(campaignId, main),
                "v3 主存档槽必须由稳定 campaign_id 唯一生成");
            Assert(!CampaignIdentity.OwnsSlot(campaignId, "mod_campaign-01")
                    && !ModSaveSlotPolicy.IsModSlot("mod_campaign-01")
                    && ModSaveSlotPolicy.IsModSlot(main),
                "v3 不得命中旧版 mod_<manifest.id> 槽");

            string[] slots =
            {
                main,
                ModSaveSlotPolicy.IsolatedAutoSlot(main, "auto"),
                ModSaveSlotPolicy.IsolatedAutoSlot(main, "auto_free"),
                ModSaveSlotPolicy.IsolatedAutoSlot(main, "auto_battle"),
            };
            string[] expected =
            {
                "mod_campaign_campaign-01",
                "mod_campaign_campaign-01_auto",
                "mod_campaign_campaign-01_auto_free",
                "mod_campaign_campaign-01_auto_battle",
            };
            Assert(string.Join("|", slots) == string.Join("|", expected),
                "主槽与三类自动槽枚举不完整或命名不稳定");
            Assert(new HashSet<string>(slots, StringComparer.Ordinal).Count == 4,
                "主槽与三类自动槽必须互不重叠");
        }

        private static void TestSelectionDoesNotStartCampaign()
        {
            ModPackage alpha = Package("mod-alpha", "campaign-alpha");
            ModPackage beta = Package("mod-beta", "campaign-beta");
            ModCampaignState.Clear();
            var flow = new CampaignMenuFlow(new[] { alpha, beta }, null);

            Assert(flow.Select(alpha.CampaignId), "已加载 campaign 应可切换到存档页");
            Assert(object.ReferenceEquals(flow.SelectedPackage, alpha)
                    && object.ReferenceEquals(flow.RecentPackage, alpha),
                "选择 campaign 必须只更新当前页和最近记录");
            Assert(!ModCampaignState.Active,
                "选择 campaign 不能直接启动战役或激活运行态");

            flow.Back();
            Assert(flow.SelectedPackage == null && object.ReferenceEquals(flow.RecentPackage, alpha),
                "返回列表只清除当前页，不应丢失最近选择");
            Assert(!flow.Select("not-loaded") && flow.SelectedPackage == null,
                "未加载 campaign 不得成为当前选择");
        }

        private static void TestRecentCampaignOnlyUsesLoadedPackages()
        {
            ModPackage alpha = Package("mod-alpha", "campaign-alpha");
            var missing = new CampaignMenuFlow(new[] { alpha }, "removed-campaign");
            Assert(missing.RecentCampaignId == null && missing.RecentPackage == null,
                "配置中的最近 ID 若未加载，必须忽略而不是回退到其他包");

            var loaded = new CampaignMenuFlow(new[] { alpha }, alpha.CampaignId);
            Assert(loaded.SelectedPackage == null
                    && object.ReferenceEquals(loaded.RecentPackage, alpha),
                "最近 ID 只可恢复已加载包，且恢复记录不能等价于启动/选择");
        }

        private static void TestBattleNamedCharactersAreIncludedInTotal()
        {
            List<string> characters = BattleCompositionPolicy.ParseCharacters(
                "brother1,girl4", 2);
            Assert(characters.Count == 2,
                "战役总人数应包含具名官方人物，不得把具名人物额外加算");

            bool overTotal = false;
            try { BattleCompositionPolicy.ParseCharacters("brother1,girl4", 1); }
            catch (InvalidOperationException) { overTotal = true; }
            Assert(overTotal, "具名官方人物数量超过总人数必须拒绝");

            bool duplicate = false;
            try { BattleCompositionPolicy.ParseCharacters("brother1,brother1", 2); }
            catch (InvalidOperationException) { duplicate = true; }
            Assert(duplicate, "同一方不得重复计入同一官方人物");

            bool unofficial = false;
            try { BattleCompositionPolicy.ParseCharacters("user:author.hero", 1); }
            catch (InvalidOperationException) { unofficial = true; }
            Assert(unofficial, "Battle 具名人物清单只能接受已验证的官方人物");
        }

        private static void TestCombatAnimationFallback()
        {
            var frames = new Dictionary<string, string>(StringComparer.Ordinal)
            {
                { "idle", "idle.png" },
                { "attack", "attack.png" },
            };
            Dictionary<string, string> resolved =
                CombatCharacterPolicy.ResolveFrames(frames, "normal.png");
            Assert(resolved["idle"] == "idle.png"
                    && resolved["attack"] == "attack.png"
                    && resolved["hurt"] == "idle.png"
                    && resolved["defence"] == "idle.png",
                "缺少的 Combat 动画状态必须逐项回退到 idle");

            resolved = CombatCharacterPolicy.ResolveFrames(
                new Dictionary<string, string>(), "normal.png");
            Assert(resolved["idle"] == "normal.png"
                    && resolved["attack"] == "normal.png",
                "缺少 idle 时必须回退人物 normal 立绘");
        }

        private static ModPackage Package(string modId, string campaignId)
        {
            return new ModPackage
            {
                Id = modId,
                CampaignId = campaignId,
                Entry = "main",
                Campaign = new CampaignConfig { Id = campaignId, NewGame = true },
            };
        }

        private static void Write(ZipArchive zip, string name, string value)
        {
            ZipArchiveEntry entry = zip.CreateEntry(name);
            using (var writer = new StreamWriter(entry.Open())) writer.Write(value);
        }

        private static void Assert(bool condition, string message)
        {
            if (!condition) throw new Exception("Campaign regression: " + message);
        }
    }
}
