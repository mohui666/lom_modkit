using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class VanillaStoryPatchTests
    {
        private static void Check(bool value, string message)
        { if (!value) throw new InvalidOperationException(message); }

        internal static void Run()
        {
            string source = "-- say(\"same\")\r\nif ready then say(\"same\") else say(\"same\") end\r\nadvance()";
            string changes = "[{\"find\":\"say(\\\"same\\\")\",\"occurrence\":3,\"replace\":\"extra(); say(\\\"same\\\")\"}]";
            string changed = VanillaStoryPatch.Apply(source, changes);
            Check(changed == source.Replace("else say", "else extra(); say"), "Only the selected original occurrence may change");
            bool failed = false;
            try { VanillaStoryPatch.Apply(source, changes.Replace("same", "missing")); }
            catch (InvalidOperationException) { failed = true; }
            Check(failed && source.EndsWith("advance()"), "A missing anchor must fail before any original commands run");
            failed = false;
            try { VanillaStoryPatch.Apply(source, changes.Substring(0,changes.Length-1)+","+changes.Substring(1)); }
            catch (FormatException) { failed = true; }
            Check(failed, "Overlapping changes must be rejected");

            var first = new ModPackage {Id="patch_a",CampaignId="patch_a",Entry="main",PlayMode="addon"};
            var second = new ModPackage {Id="patch_b",CampaignId="patch_b",Entry="main",PlayMode="addon"};
            first.LuaScripts["main"] = "first"; second.LuaScripts["main"] = "second";
            first.VanillaOverrides["CH_DEMO"] = "main"; second.VanillaOverrides["ch_demo"] = "main";
            var warnings = new List<string>();
            ModRegistry.Rebuild(new[] {first,second},warnings.Add);
            string registered;
            ModPlayModeKind originalMode = ModPlayModeSession.Selected;
            ModPlayModeSession.Select(ModPlayModeKind.Original);
            Check(!ModRegistry.TryGetVanillaOverride("CH_DEMO",ModPlayModeSession.Allows,out registered), "Original mode must not apply patches");
            ModPlayModeSession.Select(ModPlayModeKind.Addon);
            Check(ModRegistry.TryGetVanillaOverride("ch_demo",ModPlayModeSession.Allows,out registered)
                && registered==first.GetRegisteredScriptName("main"), "Enabled patches must respect deterministic chapter ownership and resource casing");
            Check(ModRegistry.TryGetVanillaOverride("CH_DEMO",package=>ReferenceEquals(package,second),out registered)
                && registered==second.GetRegisteredScriptName("main"), "Inactive packages must not intercept the selected campaign's chapter");
            Check(warnings.Count==1 && warnings[0].Contains("多个补丁"), "Conflicting chapter ownership must be visible");
            ModPlayModeSession.Select(originalMode);
            ModRegistry.Rebuild(new ModPackage[0]);
            string root = Path.Combine(Path.GetTempPath(),"lom-bookmark-test-"+Guid.NewGuid().ToString("N"));
            try
            {
                string firstMark = VanillaBookmarkStore.Write(root,"CH_DEMO","D_FIRST","原对白","game",1,4,1);
                string secondMark = VanillaBookmarkStore.Write(root,"CH_DEMO","D_SECOND","另一句","game",1,4,2);
                Check(firstMark!=secondMark && File.Exists(firstMark) && File.Exists(secondMark),"Explicit bookmarks must not overwrite previous records");
                var record = MiniJson.Parse(File.ReadAllText(secondMark)) as Dictionary<string,object>;
                Check((string)record["script"]=="CH_DEMO" && (string)record["text_key"]=="D_SECOND", "Bookmarks must retain both chapter and dialogue identities");
                VanillaBookmarkStore.WriteTimeline(root,new Dictionary<string,object> {{"schema",1},{"missions",new List<object>()}});
                Check(File.Exists(Path.Combine(root,"timeline.json")) && Directory.GetFiles(root,"*.tmp").Length==0,"Only complete bookmark and timeline files may become visible");
            }
            finally {if(Directory.Exists(root)) Directory.Delete(root,true);}
            try {
                SystemMessageScope.Run<int>(()=>{
                    Check(SystemMessageScope.Suppressed,"Quiet changes must suppress presentations");
                    SystemMessageScope.Run<int>(()=>{Check(SystemMessageScope.Suppressed,"Nested quiet changes must retain suppression");return 0;});
                    Check(SystemMessageScope.Suppressed,"Nested changes must not release the outer scope");
                    throw new InvalidOperationException("expected");
                });
            } catch(InvalidOperationException) {}
            Check(!SystemMessageScope.Suppressed,"An interrupted mutation must restore normal system messages");
            Console.WriteLine("PASS vanilla story edits, original progression, mode isolation, chapter conflicts and temporary message suppression");
        }
    }
}
