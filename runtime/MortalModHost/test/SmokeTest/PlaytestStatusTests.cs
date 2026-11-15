using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class PlaytestStatusTests
    {
        internal static void Run()
        {
            string root = Path.Combine(Path.GetTempPath(), "lom-playtest-status-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                string requestPath = Path.Combine(root, "preview-request.json");
                string body = "{\"format\":1,\"mod_id\":\"lom_modkit_preview\",\"script_id\":\"main\",\"node_id\":\"target\"";
                PreviewRequest request; string error;
                File.WriteAllText(requestPath, body + "}");
                Check(PreviewRequest.TryRead(requestPath, out request, out error) && request.RequestId == null, "old F5 must remain valid");
                PlaytestStatusRuntime.Observe(request, root);
                Check(!Directory.Exists(Path.Combine(root, "playtest-status")), "old F5 must not create tracking files");
                foreach (string id in new[] { "../outside", "ABCDEF0123456789abcdef0123456789", "" })
                {
                    File.WriteAllText(requestPath, body + ",\"request_id\":\"" + id + "\"}");
                    Check(!PreviewRequest.TryRead(requestPath, out request, out error), "unsafe tracking id accepted");
                }
                const string first = "0123456789abcdef0123456789abcdef";
                File.WriteAllText(requestPath, body + ",\"request_id\":\"" + first + "\"}");
                Check(PreviewRequest.TryRead(requestPath, out request, out error), error);
                PlaytestStatusRuntime.Observe(request, root);
                Check(Read(root, first)["state"].Equals("game_started"), "Host receipt must not report chapter/node entry");
                PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "main");
                PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "main", "target");
                PlaytestStatusRuntime.RuntimeFailed("unrelated old playback error", "old", "broken");
                Check(Read(root, first)["state"].Equals("game_started"), "old playback acknowledged or failed a request before dispatch");
                PlaytestStatusRuntime.Dispatch();
                PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "other");
                PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "other", "target");
                Check(Read(root, first)["state"].Equals("game_started"), "wrong chapter acknowledged");
                PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "main");
                Check(Read(root, first)["state"].Equals("chapter_entered"), "actual chapter not acknowledged");
                PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "main", "wrong");
                Check(Read(root, first)["state"].Equals("chapter_entered"), "wrong node acknowledged");
                PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "main", "target");
                var entered = Read(root, first);
                Check(entered["state"].Equals("node_entered") && entered["evidence"].Equals("node_trace")
                    && entered["visual_checked"].Equals(false), "node evidence or visual inspection claim incorrect");
                string saved = File.ReadAllText(StatusPath(root, first));
                PlaytestStatusRuntime.Observe(new PreviewRequest(), root);
                PlaytestStatusRuntime.Fail("unrelated later F5 error", "other", "wrong");
                Check(File.ReadAllText(StatusPath(root, first)) == saved, "ordinary F5 leaked into old request status");
                const string second = "11111111111111111111111111111111";
                PlaytestStatusRuntime.Observe(new PreviewRequest { RequestId = second, ScriptId = "main", NodeId = "target" }, root);
                PlaytestStatusRuntime.Fail("example Lua failure", "main", "broken");
                PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "main", "target");
                var failure = Read(root, second);
                Check(failure["state"].Equals("failed") && failure["current_node"].Equals("broken"), "failure location lost or false recovery");
                const string third = "22222222222222222222222222222222";
                PlaytestStatusRuntime.Observe(new PreviewRequest { RequestId = third, ScriptId = "main", NodeId = "target" }, root);
                PlaytestStatusRuntime.Shutdown();
                Check(Read(root, third)["state"].Equals("failed"), "exit before target not reported");
                TestEventRequest(root, requestPath);
                TestEventContext(root);
            }
            finally { PlaytestStatusRuntime.Shutdown(); Directory.Delete(root, true); }
            Console.WriteLine("PASS tracked playtest receipt, exact chapter/node/event evidence, predecessor context and old F5 isolation (offline)");
        }

        private static void TestEventRequest(string root, string requestPath)
        {
            string eventBody = "{\"format\":1,\"mod_id\":\"lom_modkit_preview\",\"script_id\":\"setup\",\"node_id\":\"notice\","
                + "\"fresh_campaign\":true,\"event_id\":\"wanted\",\"target\":\"original_target\",\"from\":\"actual_from\"";
            string tracked = ",\"request_id\":\"33333333333333333333333333333333\",\"event_script\":\"event_story\",\"event_node\":\"open\"";
            PreviewRequest request; string error;
            File.WriteAllText(requestPath, eventBody + "}");
            Check(PreviewRequest.TryRead(requestPath, out request, out error) && request.SourceScript == null, "old boundary event request changed");
            string versionTwo = eventBody.Replace("\"format\":1", "\"format\":2");
            File.WriteAllText(requestPath, versionTwo + tracked + ",\"source_script\":\"original_before\"}");
            Check(PreviewRequest.TryRead(requestPath, out request, out error) && request.SourceScript == "original_before"
                && request.From == "actual_from" && request.ScriptId == "setup" && request.EventScript == "event_story", "event target and actual original source were conflated");
            var config = new ScenarioConfig();
            config.Events.Add(new ScenarioEvent { Id = "wanted", Script = "event_story" });
            var loaded = new HashSet<string> { "original_before", "original_target" };
            Check(request.ValidateEvent(config, loaded.Contains).Id == "wanted", "valid event context rejected");
            loaded.Remove("original_before");
            Throws(() => request.ValidateEvent(config, loaded.Contains), "missing original source accepted");
            loaded.Add("original_before"); request.EventScript = "another_story";
            Throws(() => request.ValidateEvent(config, loaded.Contains), "event_script did not match selected event");
            foreach (string invalid in new[] {
                eventBody + tracked + "}",
                eventBody + ",\"source_script\":\"original_before\"}",
                eventBody + ",\"event_script\":\"event_story\"}",
                eventBody + ",\"event_node\":\"open\"}",
                versionTwo + "}",
                versionTwo + ",\"request_id\":\"33333333333333333333333333333333\"}",
                versionTwo + ",\"event_script\":\"event_story\",\"event_node\":\"open\"}",
                versionTwo.Replace("\"fresh_campaign\":true", "\"fresh_campaign\":false") + tracked + "}",
                versionTwo.Replace(",\"target\":\"original_target\"", "").Replace(",\"from\":\"actual_from\"", "") + tracked + ",\"source_script\":\"original_before\"}",
                versionTwo + tracked + ",\"source_script\":\"MOD_fake\"}",
                versionTwo + tracked + ",\"source_script\":\"../outside\"}" })
            {
                File.WriteAllText(requestPath, invalid);
                Check(!PreviewRequest.TryRead(requestPath, out request, out error), "invalid event preview context accepted");
            }
        }

        private static PreviewRequest BeginEvent(string root, string id, string source = "original_before")
        {
            var request = new PreviewRequest { RequestId = id, ModId = "lom_modkit_preview", FreshCampaign = true,
                ScriptId = "setup", NodeId = "notice", EventId = "wanted", EventScript = "event_story", EventNode = "open",
                SourceScript = source, Target = "original_target" };
            PlaytestStatusRuntime.Observe(request, root);
            PlaytestStatusRuntime.Dispatch();
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "setup");
            PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "setup", "notice");
            Check(Read(root, id)["state"].Equals("game_started") && Read(root, id)["event_entered"].Equals(false), "setup was reported as target event success");
            return request;
        }

        private static void TestEventContext(string root)
        {
            const string miss = "44444444444444444444444444444444";
            BeginEvent(root, miss);
            PlaytestStatusRuntime.OriginalScriptStarted("original_before");
            Check(Read(root, miss)["state"].Equals("predecessor_entered") && Read(root, miss)["predecessor_entered"].Equals(true), "real predecessor entry was not recorded");
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "different_event");
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "event_story");
            PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "event_story", "open");
            PlaytestStatusRuntime.OriginalScriptStarted("original_target");
            var notTriggered = Read(root, miss);
            Check(notTriggered["state"].Equals("event_not_triggered") && notTriggered["event_entered"].Equals(false)
                && notTriggered["original_target_entered"].Equals(true), "another event with the same script was mistaken for the requested event");
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "wanted");
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "event_story");
            Check(Read(root, miss)["state"].Equals("event_not_triggered"), "a later event overwrote the missed target boundary");
            string missSaved = File.ReadAllText(StatusPath(root, miss));

            const string success = "55555555555555555555555555555555";
            BeginEvent(root, success);
            Check(File.ReadAllText(StatusPath(root, miss)) == missSaved, "missed-event diagnosis was overwritten by a later request");
            PlaytestStatusRuntime.OriginalScriptStarted("original_before");
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "wanted");
            Check(Read(root, success)["event_entered"].Equals(false), "selection alone was reported as execution");
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "event_story");
            Check(Read(root, success)["event_entered"].Equals(true) && Read(root, success)["event_node_entered"].Equals(false), "event Lua entry and node trace were conflated");
            PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "event_story", "wrong");
            Check(Read(root, success)["event_node_entered"].Equals(false), "wrong event node acknowledged");
            PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "event_story", "open");
            PlaytestStatusRuntime.OriginalScriptStarted("original_target");
            var completed = Read(root, success);
            Check(completed["state"].Equals("original_target_entered") && completed["event_node_entered"].Equals(true)
                && completed["predecessor_entered"].Equals(true) && completed["original_target_entered"].Equals(true)
                && completed["evidence"].Equals("original_lua_start") && completed["visual_checked"].Equals(false), "natural event continuation evidence lost");

            const string replaced = "66666666666666666666666666666666";
            BeginEvent(root, replaced, null);
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "wanted");
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "different_event");
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "event_story");
            PlaytestStatusRuntime.OriginalScriptStarted("original_target");
            Check(Read(root, replaced)["state"].Equals("event_not_triggered"), "stale earlier selection acknowledged a different event");

            const string nested = "77777777777777777777777777777777";
            BeginEvent(root, nested);
            PlaytestStatusRuntime.EventSelected("lom_modkit_preview", "wanted");
            PlaytestStatusRuntime.ScriptStarted("lom_modkit_preview", "event_story");
            PlaytestStatusRuntime.NodeEntered("lom_modkit_preview", "event_story", "open");
            PlaytestStatusRuntime.OriginalScriptStarted("original_before");
            Check(Read(root, nested)["state"].Equals("node_entered") && Read(root, nested)["predecessor_entered"].Equals(true), "late predecessor postfix regressed the event state");
            PlaytestStatusRuntime.Shutdown();
        }

        private static string StatusPath(string root, string id) { return Path.Combine(root, "playtest-status", id + ".json"); }
        private static Dictionary<string, object> Read(string root, string id) { return (Dictionary<string, object>)MiniJson.Parse(File.ReadAllText(StatusPath(root, id))); }
        private static void Check(bool value, string message) { if (!value) throw new Exception(message); }
        private static void Throws(Action action, string message) { try { action(); } catch (InvalidOperationException) { return; } throw new Exception(message); }
    }
}
