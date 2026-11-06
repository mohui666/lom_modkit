using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal static class LegacyExtensionsTests
    {
        internal static void Run()
        {
            var source = (Dictionary<string, object>)MiniJson.Parse(@"{
                ""schema"":1,
                ""equivalents"":{""Condition"":{""test"":""return ext.GetPlayerStat('money') >= 5""},
                    ""Switch"":{""test"":""return 2""},""Position"":{""test"":""return 'Free'""}},
                ""tables"":[{""type"":""Book"",""name"":""legacy_book"",""fields"":{""_currentMartialLevel"":3}},
                    {""type"":""ItemDataCollection"",""name"":""Books"",""fields"":{""_add"":[{""so.type"":""Book"",""so.name"":""legacy_book""}]}}]
            }");
            var config = LegacyExtensionsConfig.Parse(source);
            if (config.Equivalents.Count != 3 || config.Equivalents["Condition"]["test"] != "return ext.GetPlayerStat('money') >= 5"
                || config.Tables.Count != 2 || config.Tables[0].Name != "legacy_book" || !config.Tables[1].Fields.ContainsKey("_add"))
                throw new Exception("Legacy extension source and reference fields were not preserved");
            var passive = new ModPackage { LegacyExtensions = config };
            const string emptyScenario = "{\"schema\":1,\"events\":[],\"tasks\":[]}";
            if (ScenarioConfig.Parse(emptyScenario, "addon", LegacyExtensionsConfig.HasOriginalContent(passive)).Events.Count != 0)
                throw new Exception("Passive legacy changes must not invent scenario events");
            bool rejected = false;
            try { ScenarioConfig.Parse(emptyScenario, "addon"); }
            catch (FormatException) { rejected = true; }
            if (!rejected || LegacyExtensionsConfig.HasOriginalContent(new ModPackage { LegacyExtensions = new LegacyExtensionsConfig() }))
                throw new Exception("Ordinary and empty legacy projects must keep the required event contract");
            Reject(@"{""schema"":2}");
            Reject(@"{""schema"":1,""equivalents"":{""Unknown"":{}}}");
            Reject(@"{""schema"":1,""tables"":[{""type"":""Book"",""name"":""same"",""fields"":{}},{""type"":""Book"",""name"":""same"",""fields"":{}}]}");
            Console.WriteLine("PASS legacy extensions: exact Lua sources, three checkpoint kinds, SO reference fields and malformed metadata (Unity execution=false)");
        }
        private static void Reject(string json)
        {
            try { LegacyExtensionsConfig.Parse((Dictionary<string, object>)MiniJson.Parse(json)); }
            catch (FormatException) { return; }
            throw new Exception("Invalid legacy configuration was accepted");
        }
    }
}
