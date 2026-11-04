using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class CustomConsumableTests
    {
        internal static void Run()
        {
            var field = Parse("{\"category\":\"consumable\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":4}]}");
            Check(field.UseIn == "field" && field.ConsumableEffects.Count == 1 && field.ConsumableEffects[0].Value == 4,
                "消耗品默认场景或属性效果丢失");
            var combat = Parse("{\"category\":\"consumable\",\"use_in\":\"combat\",\"effects\":[{\"type\":\"heal\",\"value\":25},{\"type\":\"stamina\",\"value\":10}]}");
            Check(combat.UseIn == "combat" && combat.ConsumableEffects.Count == 2, "决斗恢复效果丢失");
            foreach (string json in new[] {
                "{\"category\":\"consumable\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":0}]}",
                "{\"category\":\"consumable\",\"effects\":[{\"type\":\"heal\",\"value\":25}]}",
                "{\"category\":\"consumable\",\"use_in\":\"combat\",\"effects\":[{\"type\":\"stat\",\"stat\":\"mental\",\"value\":4}]}",
                "{\"category\":\"consumable\",\"use_in\":\"combat\",\"effects\":[{\"type\":\"heal\",\"target\":\"enemy\",\"value\":25}]}" })
                Reject<FormatException>(() => Parse(json));

            var records = new List<string> { "unrelated official or MOD flag" };
            CustomConsumableState.Write(records, "first", "demo.pill", 3);
            CustomConsumableState.Write(records, "second", "demo.pill", 8);
            Check(CustomConsumableState.Read(records, "first", "demo.pill") == 3
                && CustomConsumableState.Read(records, "second", "demo.pill") == 8, "同内容ID跨包串数量");
            CustomConsumableState.Write(records, "first", "demo.pill", 0);
            Check(CustomConsumableState.Read(records, "first", "demo.pill") == 0
                && CustomConsumableState.Read(records, "second", "demo.pill") == 8 && records.Contains("unrelated official or MOD flag"),
                "用尽药物不应修改其他包或官方标记");
            CustomConsumableState.Write(records, "first", "demo.pill", int.MaxValue);
            Check(CustomConsumableState.Read(records, "first", "demo.pill") == int.MaxValue, "数量必须保留Int32精度");
            Reject<InvalidDataException>(() => CustomConsumableState.Read(new[] {
                "LOMMOD_CONSUMABLE_V1:first:demo.pill:1", "LOMMOD_CONSUMABLE_V1:first:demo.pill:2" }, "first", "demo.pill"));
            foreach (string count in new[] { "-1", "1.5", "2147483648", "broken" })
                Reject<InvalidDataException>(() => CustomConsumableState.Read(new[] { "LOMMOD_CONSUMABLE_V1:first:demo.pill:" + count }, "first", "demo.pill"));
            Console.WriteLine("PASS consumables: effect/context validation and native StorySave count records (offline)");
        }
        private static CustomGameplayDefinition Parse(string json)
        {
            return CustomGameplayDefinition.Parse((Dictionary<string, object>)MiniJson.Parse(json), "item");
        }
        private static void Check(bool ok, string message) { if (!ok) throw new Exception(message); }
        private static void Reject<T>(Action action) where T : Exception
        {
            try { action(); } catch (T) { return; }
            throw new Exception("应拒绝的数据被接受：" + typeof(T).Name);
        }
    }
}
