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
            TestSameSideSpawnerAndSliderLevel();
            TestCombatVitalityDefaultsToConfiguredMaximum();
            TestOfficialAutoLoadRedirectsOnTitle();
            TestCampaignIdsThatLookLikeSlotSuffixes();
            TestCombatPlayerOverrideKeysStayPrefixed();
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
                Assert(loaded.Count == 0, "v1/v2 包不得被 Runtime 静默加载");
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
            Assert(ModSaveSlotPolicy.IsolatedManualSlot(campaignId, 1) == main
                    && ModSaveSlotPolicy.IsolatedManualSlot(campaignId, 2)
                    == "mod_campaign_campaign-01_s002"
                    && CampaignIdentity.OwnsSlot(
                        campaignId, ModSaveSlotPolicy.IsolatedManualSlot(campaignId, 20)),
                "右侧 001～020 必须映射到战役隔离手动槽，001 沿用主槽");
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
            Assert(ModSaveSlotPolicy.IsIsolatedAutoSlotForCampaign(
                    campaignId, "mod_campaign_campaign-01_auto_battle")
                    && !ModSaveSlotPolicy.IsIsolatedAutoSlotForCampaign(
                        campaignId, "mod_campaign_campaign-01_s002"),
                "只有三类自动槽应使用来源手动槽绑定；002~020 是独立手动槽");
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
                    && !BattleCompositionPolicy.IsVerifiedAssetIdentity("special4", "Special4")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special4", "樊嘯天_敵方")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special811", "Special811_Die")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special401", "Enemy_Special401_Animator")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special401", "丐幫_王二壯")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special401", "Special_401_毛二壯")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special102", "南宮_南宮深")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special103", "南宮_南宮淺")
                    && BattleCompositionPolicy.IsVerifiedAssetIdentity("special811", "唐門_唐衫"),
                "Battle 官方人物必须按已核对的 Animator/动画资源身份匹配");
            string animatorAddress;
            Assert(BattleCompositionPolicy.TryOfficialBattleAnimatorAddress("special4", out animatorAddress)
                    && animatorAddress.IndexOf("Enemy_Special004_Animator", StringComparison.Ordinal) >= 0,
                "catalog 中的 Boss Animator 必须能作为 special4 的生成底板");
            Assert(!BattleCompositionPolicy.IsVerifiedAssetIdentity("special3", "Brother3_Animator")
                    && !BattleCompositionPolicy.IsVerifiedAssetIdentity("special3", "prefix_special003_animator"),
                "Battle 官方人物不得使用任意位置子串造成相似 ID 串错");
            Assert(!BattleCompositionPolicy.HasNpcPrefabAsset("brother4")
                    && !BattleCompositionPolicy.HasNpcPrefabAsset("girl4")
                    && BattleCompositionPolicy.HasNpcPrefabAsset("special4")
                    && BattleCompositionPolicy.HasNpcPrefabAsset("special811"),
                "玩家战场技能 Animator 与可生成 NpcSpawnPreset 必须严格区分");
            Assert(BattleCompositionPolicy.HasBattleLevel("500")
                    && BattleCompositionPolicy.HasBattleLevel("001")
                    && !BattleCompositionPolicy.HasBattleLevel("400"),
                "只有原版 BattleLevel.NameKey 存在的阵营才能附加兵种");
            List<string> factions = BattleCompositionPolicy.ParseFactions("500,001");
            Assert(factions.Count == 2 && factions[0] == "500" && factions[1] == "001",
                "附加兵种应按声明顺序混入，不得整营替换");
            List<BattleCompositionPolicy.FactionGroup> groups =
                BattleCompositionPolicy.ParseFactionGroups("500:3,002:5");
            Assert(groups.Count == 2 && groups[0].People == 3 && groups[1].People == 5
                    && BattleCompositionPolicy.TotalPeople(groups, 1) == 9,
                "各方总人数必须等于各阵营 people 与具名角色之和");
            List<BattleCompositionPolicy.FactionGroup> legacy =
                BattleCompositionPolicy.ResolveSideGroups("500,001", 1, 8);
            Assert(legacy.Count == 2 && legacy[0].People == 6 && legacy[1].People == 1
                    && BattleCompositionPolicy.TotalPeople(legacy, 1) == 8,
                "旧脚本的手动总人数应补给第一个阵营");
            bool badFaction = false;
            try { BattleCompositionPolicy.ParseFactions("400"); }
            catch (InvalidOperationException) { badFaction = true; }
            Assert(badFaction, "没有 BattleLevel 的阵营不能当作附加兵种");

            // Combat 不再从包含中文描述/数字补零的资源路径猜人物身份；
            // Runtime 直接使用 CombatLevel.EnemyStat.Name 的原版对象关系。
        }

        private static void TestSameSideSpawnerAndSliderLevel()
        {
            Assert(BattleCompositionPolicy.SameSideSpawnerField("friend") == "_friendSpawnerPrefab"
                    && BattleCompositionPolicy.SameSideSpawnerField("enemy") == "_enemySpawnerPrefab"
                    && BattleCompositionPolicy.IsSameSideSpawnerField("friend", "_friendSpawnerPrefab")
                    && !BattleCompositionPolicy.IsSameSideSpawnerField("friend", "_enemySpawnerPrefab"),
                "具名角色必须先查本方生成器，不能直接拿敌方 Boss preset");
            Assert(CombatStatDisplayPolicy.OfficialLevelIndex(100, 50, 5) == 4
                    && CombatStatDisplayPolicy.OfficialLevelIndex(50, 50, 5) == 4
                    && CombatStatDisplayPolicy.OfficialLevelIndex(30, 50, 5) == 3
                    && CombatStatDisplayPolicy.OfficialLevelIndex(0, 50, 5) == 0,
                "决斗详情评语应按官方档位，作者填 100 时落在最高评语而不是越界数字");
        }

        private static void TestCombatVitalityDefaultsToConfiguredMaximum()
        {
            int maximum;
            int current;
            CombatVitalPolicy.Resolve(100, 100, true, 333, false, 0,
                out maximum, out current);
            Assert(maximum == 333 && current == 333,
                "只填写 Combat 对手最大血量时，初始血量必须从该基准满血开始");
            CombatVitalPolicy.Resolve(333, 333, true, 333, true, 120,
                out maximum, out current);
            Assert(maximum == 333 && current == 120,
                "显式填写 Combat 对手初始血量时必须覆盖满血默认值");
            CombatVitalPolicy.Resolve(100, 100, true, 0, false, 0,
                out maximum, out current);
            Assert(maximum == 0 && current == 0,
                "最大气力为 0 时不得被旧初始气力反向抬回非零值");
            CombatVitalPolicy.Resolve(100, 100, true, 333, true, 999,
                out maximum, out current);
            Assert(maximum == 333 && current == 333,
                "初始血量不得超过作者设置的最大血量");
            Assert(CombatVitalPolicy.ResolveInitialHealthAfterModifiers(333, false, 400) == 400,
                "未填写初始血量时，基础血量加原版增益后必须满最终血量");
            Assert(CombatVitalPolicy.ResolveInitialHealthAfterModifiers(120, true, 400) == 120,
                "显式初始血量必须保留，不能被增益结算重置为满血");
            Assert(CombatVitalPolicy.ResolveInitialHealthAfterModifiers(500, true, 400) == 400,
                "显式初始血量仍必须夹到最终血量上限");
        }

        private static void TestCombatPlayerOverrideKeysStayPrefixed()
        {
            string official;
            Assert(CombatPlayerOverridePolicy.Key("strength") == "player_strength"
                    && CombatPlayerOverridePolicy.Key("player_health") == "player_health"
                    && CombatPlayerOverridePolicy.TryOfficialGameStatType("player_strength", out official)
                    && official == "體力"
                    && CombatPlayerOverridePolicy.TryOfficialGameStatType("stamina_power", out official)
                    && official == "內力"
                    && !CombatPlayerOverridePolicy.TryOfficialGameStatType("max_health", out official),
                "赵活覆盖必须使用 player_ 前缀，并映射到官方 GameStatType");
            var seen = new HashSet<string>(StringComparer.Ordinal);
            seen.Add("player_strength");
            seen.Add("player_talents");
            Assert(CombatPlayerOverridePolicy.HasAny(seen.Contains)
                    && !CombatPlayerOverridePolicy.TouchesVitality(seen.Contains),
                "未填写血量字段时不得 Reset 赵活官方计算血量");
        }

        private static void TestOfficialAutoLoadRedirectsOnTitle()
        {
            const string campaignId = "campaign-01";
            string isolated = ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                "auto_battle", "001", null, campaignId);
            Assert(isolated == "mod_campaign_campaign-01_auto_battle",
                "标题页点战役自动档必须重定向到隔离槽，不能读原版 auto_battle");
            Assert(ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                    "auto_free", "001", null, campaignId)
                    == "mod_campaign_campaign-01_auto_free",
                "自由自动档同样必须走隔离槽");
            Assert(ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                    "auto_battle", "001", null, null) == "auto_battle",
                "没有当前战役时不得改写原版自动槽");
            Assert(ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                    "mod_campaign_campaign-01_auto", "001", campaignId, campaignId)
                    == "mod_campaign_campaign-01_auto",
                "已经是隔离槽名时不得再拼接");
            Assert(ModSaveSlotPolicy.IsolatedAutoSlot(
                    "mod_campaign_campaign-01_s002", "auto")
                    == "mod_campaign_campaign-01_auto",
                "002～020 上手动槽的自动档仍写入战役根隔离槽");
        }

        private static void TestCampaignIdsThatLookLikeSlotSuffixes()
        {
            const string campaignId = "chapter_s002";
            string root = CampaignIdentity.SaveSlot(campaignId);
            string slot = ModSaveSlotPolicy.IsolatedManualSlot(campaignId, 2);
            Assert(CampaignIdentity.OwnsSlot(campaignId, root)
                    && CampaignIdentity.OwnsSlot(campaignId, slot)
                    && ModSaveSlotPolicy.IsolatedAutoSlotForCampaign(
                        campaignId, "auto_battle") == root + "_auto_battle",
                "campaign_id 自身带槽后缀时，主槽/手动槽/自动槽仍必须属于同一战役");
            Assert(ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                    "auto", root, null, "chapter_s002") == root + "_auto",
                "标题页选中的战役必须优先于残留 CurrentSlot 推导自动槽");
            Assert(ModSaveSlotPolicy.RedirectOfficialAutoSlot(
                    "auto_battle", "001", "stale-campaign", null) == "auto_battle",
                "原版 CurrentSlot 不得因残留 MOD 运行态被重定向到旧战役自动槽");
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
