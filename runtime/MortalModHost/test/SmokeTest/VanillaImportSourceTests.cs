using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json.Nodes;

namespace MortalModHost
{
    internal static class VanillaImportSourceTests
    {
        private static void Check(bool condition, string message)
        {
            if (!condition) throw new Exception(message);
        }

        internal static void Run()
        {
            JsonNode fixture = JsonNode.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory,"vanilla-import.json")));
            string original = fixture["source"].GetValue<string>();
            var baseline = VanillaImportSource.Parse(fixture["baseline"].ToJsonString());
            baseline.VerifyOriginal(original);
            Check(baseline.Begin() && baseline.Node("original_2") && baseline.ContinueOriginal(),
                "Untouched native statements must retain original provenance");
            var edited = VanillaImportSource.Parse(fixture["story"].ToJsonString());
            edited.VerifyOriginal(original);
            Check(edited.Begin() && !edited.Node("original_2") && edited.ContinueOriginal(),
                "Text-only edits must disclose the edited line and then return to original provenance");
            Check(!edited.Node("inserted") && edited.Node("original_6"),
                "Inserted inherited dialogue must disclose only its own content");

            JsonNode persistent = fixture["baseline"].DeepClone();
            persistent["nodes"].AsArray().Insert(1,JsonNode.Parse("{\"id\":\"music\",\"type\":\"music\",\"op\":\"stop\"}"));
            var source = VanillaImportSource.Parse(persistent.ToJsonString());
            source.VerifyOriginal(original);
            Check(source.Begin() && !source.Node("music") && !source.Node("original_2") && !source.ContinueOriginal(),
                "Ongoing presentation or gameplay effects must keep the chapter disclosed");
            Check(source.Begin() && source.Node("original_2"), "A new run must reset transient playback state");

            JsonNode deleted = fixture["baseline"].DeepClone();
            deleted["nodes"].AsArray().RemoveAt(1);
            var changedFlow = VanillaImportSource.Parse(deleted.ToJsonString());
            changedFlow.VerifyOriginal(original);
            Check(!changedFlow.Begin() && !changedFlow.Node("original_6"), "Deleted native steps alter the chapter's flow");
            var stale = VanillaImportSource.Parse(fixture["baseline"].ToJsonString());
            bool rejected = false;
            try { stale.VerifyOriginal(original + "-- changed"); }
            catch (InvalidOperationException) { rejected = true; }
            Check(rejected && !stale.OriginalVerified, "A changed installed chapter must require reimport");

            JsonNode structured = fixture["baseline"].DeepClone();
            structured["vanilla_import"]["schema"] = 2;
            structured["vanilla_import"]["origins"][1]["node"]["goto"] = "original_3";
            structured["nodes"][1]["goto"] = "extra";
            structured["nodes"].AsArray().Insert(2, JsonNode.Parse("{\"id\":\"extra\",\"type\":\"say\",\"text\":\"added\",\"inherit_dialog\":true,\"goto\":\"original_3\"}"));
            var detour = VanillaImportSource.Parse(structured.ToJsonString());
            detour.VerifyOriginal(original);
            Check(detour.Begin() && detour.Node("original_2") && !detour.Node("extra") && detour.Node("original_6"),
                "Structured transient insertions must rejoin native attribution");
            structured["nodes"][1]["goto"] = "original_6";
            var redirected = VanillaImportSource.Parse(structured.ToJsonString());
            redirected.VerifyOriginal(original);
            Check(redirected.Begin() && !redirected.Node("original_2") && !redirected.ContinueOriginal(),
                "Redirecting to a different native successor must retain MOD attribution");
            structured["vanilla_import"]["external"] = true;
            var externalSource = VanillaImportSource.Parse(structured.ToJsonString());
            Check(externalSource.OriginalVerified && !externalSource.Begin() && !externalSource.Node("original_6"),
                "Legacy external scripts must always be classified as player content");

            // Load/classify separately from the one-time comparison and cached path.
            JsonNode large = fixture["baseline"].DeepClone();
            large["vanilla_import"]["origins"][0]["code"] = new string('x',1024*1024);
            string json = large.ToJsonString();
            var timer = Stopwatch.StartNew();
            var largeSource = VanillaImportSource.Parse(json);
            timer.Stop();
            double loadMs = timer.Elapsed.TotalMilliseconds;
            string installed = new string(largeSource.OriginalSource.ToCharArray());
            timer.Restart();
            largeSource.VerifyOriginal(installed);
            timer.Stop();
            double compareMs = timer.Elapsed.TotalMilliseconds;
            timer.Restart();
            for (int i=0;i<100000;++i)
            {
                largeSource.Begin(); largeSource.Node("original_2"); largeSource.ContinueOriginal();
            }
            timer.Stop();
            Console.WriteLine("PASS original-source classification and cached playback. Offline 1 MiB source load {0:F3} ms, first comparison {1:F3} ms, 100000 cached runs {2:F3} ms",
                loadMs,compareMs,timer.Elapsed.TotalMilliseconds);
        }
    }
}
