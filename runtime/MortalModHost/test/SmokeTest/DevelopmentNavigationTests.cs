using System;
using System.IO;

namespace MortalModHost
{
    internal static class DevelopmentNavigationTests
    {
        internal static void Run()
        {
            string root = Path.Combine(Path.GetTempPath(), "lom-dev-navigation-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                string path = Path.Combine(root, "request.json");
                string body = "{\"format\":3,\"mod_id\":\"lom_modkit_preview\",\"script_id\":\"setup\",\"node_id\":\"notice\","
                    + "\"fresh_campaign\":true,\"developer_mode\":true,\"event_id\":\"wanted\","
                    + "\"request_id\":\"1234567890abcdef1234567890abcdef\",\"event_script\":\"story\",\"event_node\":\"begin\"}";
                PreviewRequest request; string error;
                File.WriteAllText(path, body);
                Check(PreviewRequest.TryRead(path, out request, out error) && request.DeveloperMode, "developer format3 rejected: " + error);
                foreach (string invalid in new[] {
                    body.Replace("\"format\":3", "\"format\":1"),
                    body.Replace("\"format\":3", "\"format\":2"),
                    body.Replace("\"developer_mode\":true", "\"developer_mode\":false"),
                    body.Replace("\"developer_mode\":true,", ""),
                    body.Replace("\"developer_mode\":true", "\"developer_mode\":\"true\""),
                    body.Replace("\"fresh_campaign\":true", "\"fresh_campaign\":false"),
                    body.Replace("lom_modkit_preview", "another_mod"),
                    body.Replace(",\"request_id\":\"1234567890abcdef1234567890abcdef\"", ""),
                    body.Replace(",\"event_script\":\"story\"", ""),
                    body.Replace(",\"event_node\":\"begin\"", "") })
                {
                    File.WriteAllText(path, invalid);
                    Check(!PreviewRequest.TryRead(path, out request, out error), "unsafe developer preview request accepted");
                }
                File.WriteAllText(path, body.Replace("\"format\":3", "\"format\":2").Replace("\"developer_mode\":true,", ""));
                Check(PreviewRequest.TryRead(path, out request, out error) && !request.DeveloperMode, "normal format2 event preview changed");
                var developer = new PreviewRequest { ModId = "lom_modkit_preview", FreshCampaign = true, DeveloperMode = true };
                var package = new ModPackage { Id = "lom_modkit_preview", CampaignId = "preview",
                    PackagePath = Path.Combine(root, "__lom_modkit_preview.lommod"),
                    Campaign = new CampaignConfig { Id = "preview", NewGame = true, DisableOfficialEvents = false } };
                DevelopmentNavigationPolicy.ValidatePreview(package, developer);
                package.Campaign.DisableOfficialEvents = true;
                Throws(() => DevelopmentNavigationPolicy.ValidatePreview(package, developer), "suppressed original events accepted");
                package.Campaign.DisableOfficialEvents = false;
                package.PackagePath = Path.Combine(root, "installed.lommod");
                Throws(() => DevelopmentNavigationPolicy.ValidatePreview(package, developer), "installed same-id package gained developer access");

                Func<ModPlayModeKind,string,string,string,string,string,bool,bool> allowed = DevelopmentNavigationPolicy.AllowsContext;
                string slot = CampaignIdentity.SaveSlot("preview");
                Check(allowed(ModPlayModeKind.Addon, ModSaveSlotPolicy.AddonManualSlot(1), null, null, null, null, true), "addon progress rejected");
                Check(!allowed(ModPlayModeKind.Addon, "001", null, null, null, null, true), "official progress accepted");
                Check(!allowed(ModPlayModeKind.Original, ModSaveSlotPolicy.AddonManualSlot(1), null, null, null, null, true), "original mode accepted");
                Check(allowed(ModPlayModeKind.Campaign, slot, slot, "preview", "lom_modkit_preview", "preview", true), "armed preview rejected");
                Check(!allowed(ModPlayModeKind.Campaign, slot, null, null, "lom_modkit_preview", "preview", true), "unarmed campaign accepted");
                Check(!allowed(ModPlayModeKind.Campaign, slot, slot, "preview", "other", "preview", true), "another campaign accepted");
                Check(!allowed(ModPlayModeKind.Campaign, CampaignIdentity.SaveSlot("other"), slot, "preview", "lom_modkit_preview", "preview", true), "switched slot accepted");
                Check(!allowed(ModPlayModeKind.Campaign, slot, slot, "preview", "lom_modkit_preview", "preview", false), "original suppression accepted");
                Check(DevelopmentNavigationPolicy.IsDate(1, 1, 1) && DevelopmentNavigationPolicy.IsDate(9, 12, 3), "valid date boundary rejected");
                Check(!DevelopmentNavigationPolicy.IsDate(0, 4, 1) && !DevelopmentNavigationPolicy.IsDate(10, 4, 1)
                    && !DevelopmentNavigationPolicy.IsDate(1, 13, 1) && !DevelopmentNavigationPolicy.IsDate(1, 4, 0), "invalid date accepted");
            }
            finally { Directory.Delete(root, true); }
            Console.WriteLine("DevelopmentNavigationTests PASS: versioned preview, original-event policy and progress ownership");
        }

        private static void Check(bool value, string message) { if (!value) throw new Exception(message); }
        private static void Throws(Action action, string message)
        {
            try { action(); } catch (InvalidOperationException) { return; }
            throw new Exception(message);
        }
    }
}
