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
            TestVerifiedGameplayAssetIdentity();
            TestSceneTransitionReadiness();
        }

        private static void TestSceneTransitionReadiness()
        {
            Assert(SceneTransitionPolicy.IsReady(false, false, null)
                    && SceneTransitionPolicy.IsReady(false, false, ""),
                "原版目标场景与读取遮罩均完成后才应允许 MOD 切场景");
            Assert(!SceneTransitionPolicy.IsReady(true, false, "")
                    && !SceneTransitionPolicy.IsReady(false, true, ""),
                "SceneController 正在 Prepare/Loading 时必须拒绝并发切场景");
            Assert(!SceneTransitionPolicy.IsReady(false, false, "Loading1"),
                "目标场景已激活但 Loading1 尚未卸载时也必须等待，不能重复载入读取场景");
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
            var empty = new CampaignMenuFlow(new ModPackage[0], "removed-campaign");
            Assert(empty.SelectedPackage == null && empty.RecentPackage == null
                    && !empty.Select("not-loaded"),
                "没有 MOD 时战役面板状态机仍须可建立空状态，不能把打开面板等同于选择或启动");

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

        private static void TestVerifiedGameplayAssetIdentity()
        {
            Assert(BattleCompositionPolicy.IsVerifiedAssetIdentity("brother4", "Brother4_Animator")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("girl4", "Girl_004_Animator")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special3", "special003_attack_01")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special4", "Enemy_Special004_Attack1")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special811", "Special811_Die"),
                "Battle 官方人物必须按已核对的 Animator/动画资源身份匹配");
            Assert(!BattleCompositionPolicy.IsVerifiedAssetIdentity("special3", "Brother3_Animator")
                    && !BattleCompositionPolicy.IsVerifiedAssetIdentity("special3", "prefix_special003_animator"),
                "Battle 官方人物不得使用任意位置子串造成相似 ID 串错");
            Assert(!BattleCompositionPolicy.HasNpcPrefabAsset("brother4")
                    && !BattleCompositionPolicy.HasNpcPrefabAsset("girl4")
                    && BattleCompositionPolicy.HasNpcPrefabAsset("special4")
                    && BattleCompositionPolicy.HasNpcPrefabAsset("special811"),
                "玩家战场技能 Animator 与可生成 NpcSpawnPreset 必须严格区分");

            // Combat 不再从包含中文描述/数字补零的资源路径猜人物身份；
            // Runtime 直接使用 CombatLevel.EnemyStat.Name 的原版对象关系。
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
