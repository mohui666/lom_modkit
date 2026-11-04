using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class CustomGameplayTests
    {
        internal static void Run(string root)
        {
            var martial = Parse("{\"levels\":[{\"points\":40,\"requirements\":[{\"stat\":\"strength\",\"min\":20}],\"bonuses\":[{\"stat\":\"dexterity\",\"delta\":2}]}]}", "martial");
            Require(martial.Category == "book" && martial.Levels[0].Points == 40
                && martial.Levels[0].Requirements[0].Value == 20 && martial.Levels[0].Bonuses[0].Stat == "dexterity", "功法级别数据丢失");
            var support = Parse("{\"effects\":[{\"type\":\"heal\",\"target\":\"self\",\"value\":25}]}", "support");
            Require(support.Trigger == "round_start" && support.Chance == 100 && support.Uses == 1, "援助默认值不一致");
            foreach (string invalid in new[] {
                "{\"levels\":[{\"points\":0}]}",
                "{\"levels\":[{\"points\":1.5}]}",
                "{\"levels\":[{\"points\":20,\"bonuses\":[{\"stat\":\"strength\",\"delta\":1},{\"stat\":\"strength\",\"delta\":2}]}]}" })
                Reject<FormatException>(() => Parse(invalid, "martial"));
            Reject<FormatException>(() => Parse("{\"cost\":-1}", "item"));
            Reject<FormatException>(() => Parse("{\"effects\":[{\"type\":\"heal\",\"target\":\"all\",\"value\":25}]}", "support"));

            var fieldMedicine = Parse("{\"category\":\"consumable\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":10}]}", "item");
            Require(fieldMedicine.UseIn == "field" && fieldMedicine.ConsumableEffects[0].Value == 10, "养成消耗品契约不一致");
            var combatMedicine = Parse("{\"category\":\"consumable\",\"use_in\":\"combat\",\"effects\":[{\"type\":\"heal\",\"value\":25}]}", "item");
            Require(combatMedicine.UseIn == "combat" && combatMedicine.ConsumableEffects[0].Type == "heal", "决斗消耗品契约不一致");
            foreach (string invalid in new[] {
                "{\"category\":\"consumable\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":0}]}",
                "{\"category\":\"consumable\",\"use_in\":\"combat\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":10}]}",
                "{\"category\":\"consumable\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":10,\"target\":\"enemy\"}]}" })
                Reject<FormatException>(() => Parse(invalid, "item"));
            var skill = Parse("{\"power\":20,\"animation\":{\"frames\":[\"swing.png\",\"hit.png\"],\"fps\":12,\"hit_frame\":1}}", "skill").Skill;
            Require(skill.Action == "attack" && skill.Animation.Frames.Count == 2 && skill.Animation.HitFrame == 1, "招式及动画契约不一致");
            Reject<FormatException>(() => Parse("{\"animation\":{\"frames\":[\"../hit.png\"]}}", "skill"));
            Reject<FormatException>(() => Parse("{\"animation\":{\"frames\":[\"hit.png\"],\"hit_frame\":1}}", "skill"));

            string stateRoot = Path.Combine(root, "addon-state");
            ModPackage first = Package("addon_one", 'A'), second = Package("addon_two", 'B');
            string manual = ModSaveSlotPolicy.AddonManualSlot(1), other = ModSaveSlotPolicy.AddonManualSlot(2);
            string automatic = ModSaveSlotPolicy.AddonAutoSlot("auto_battle");
            var left = new PersistentStateStore(stateRoot);
            var right = new PersistentStateStore(stateRoot);
            left.Set(first, manual, "score", 5); right.Set(second, manual, "score", 9);
            left.Flush(); right.Flush();
            Require(new PersistentStateStore(stateRoot).Get(first, manual, "score") == 5
                && new PersistentStateStore(stateRoot).Get(second, manual, "score") == 9, "多个附加 MOD 同槽变量串包");
            left.Set(first, manual, "score", 17);
            left.SaveToSlot(first, other);
            left.SaveToSlot(first, automatic);
            Require(new PersistentStateStore(stateRoot).Get(first, manual, "score") == 5, "另存/自动存档不得修改旧手动档的变量");
            Require(new PersistentStateStore(stateRoot).Get(first, other, "score") == 17
                && new PersistentStateStore(stateRoot).Get(first, automatic, "score") == 17
                && left.Get(first, manual, "score") == 17, "目标栏位必须保存实际快照并保留运行态");
            Reject<InvalidOperationException>(() => left.Get(first, "001", "score"));
            Reject<InvalidOperationException>(() => left.SaveToSlot(first, "mod_addon_other"));

            GameplayCheckpointStore.Initialize(stateRoot);
            GameplaySession.Reset();
            CustomImageRuntime.ActiveBackgroundReference = "";
            GameplaySession.Prepare(second, "combat", "main", "fight", "win", "lose");
            GameplaySession.Configure("combat", "player_supports=user:demo.helper");
            Reject<InvalidOperationException>(() => GameplaySession.CaptureCheckpoint(first));
            GameplayCheckpointStore.Save(second, automatic, manual);
            GameplaySession.Reset();
            ModPackage owner; GameplayCheckpoint checkpoint;
            Require(GameplayCheckpointStore.TryLoadAddon(automatic, new[] { first, second }, out owner, out checkpoint)
                && owner.Id == second.Id && checkpoint.SourceSlot == manual
                && checkpoint.Config["player_supports"] == "user:demo.helper", "附加 MOD 战斗存档的实际所有者未恢复");
            Require(!GameplayCheckpointStore.TryLoadAddon(automatic, new[] { Package("addon_two", 'C') }, out owner, out checkpoint),
                "换包后不能冒认旧决斗 checkpoint");
            Require(!GameplayCheckpointStore.TryLoadAddon("001", new[] { first, second }, out owner, out checkpoint),
                "原版档不能被当作附加 MOD checkpoint");
            Console.WriteLine("PASS custom gameplay: definition validation, addon state isolation, save-as/autosave snapshots and checkpoint ownership (offline)");
        }
        private static CustomGameplayDefinition Parse(string json, string type)
        {
            return CustomGameplayDefinition.Parse((Dictionary<string, object>)MiniJson.Parse(json), type);
        }
        private static ModPackage Package(string id, char fingerprint)
        {
            return new ModPackage { Id = id, CampaignId = id, PlayMode = "addon", PackageFingerprint = new string(fingerprint, 64) };
        }
        private static void Require(bool value, string message)
        {
            if (!value) throw new Exception(message);
        }
        private static void Reject<T>(Action action) where T : Exception
        {
            try { action(); } catch (T) { return; }
            throw new Exception("应拒绝的自定义玩法数据被接受：" + typeof(T).Name);
        }
    }
}
