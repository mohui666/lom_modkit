using System;
using System.IO;
using System.Linq;

namespace MortalModHost
{
    internal static class AddonSaveDependencyTests
    {
        internal static void Run()
        {
            string root = Path.Combine(Path.GetTempPath(), "lom_addon_dependencies_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                AddonSaveDependencies.Initialize(root);
                var first = new ModPackage { Id = "first_addon", Name = "中文\"功法\"\\附加包", Version = "1.0.0", PlayMode = "addon" };
                var second = new ModPackage { Id = "second_addon", Name = "物品附加包", Version = "1.2.0", PlayMode = "addon" };
                var campaign = new ModPackage { Id = "campaign", Name = "纯 MOD 剧本", Version = "1.0.0", PlayMode = "sandbox" };
                string slot = ModSaveSlotPolicy.AddonManualSlot(2), path = Path.Combine(root, slot + ".addons.json");
                string save = Path.Combine(root, "Save_" + slot + ".dat");
                File.WriteAllText(save, "original GameSave bytes");
                AddonSaveDependencies.Record(slot, new[] { first, second, campaign });
                string record = File.ReadAllText(path);
                Check(!record.Contains("\"id\":\"campaign\""), "不记录纯 MOD 包");
                Check(AddonSaveDependencies.HasRecord(slot), "新档有依赖记录");
                Check(AddonSaveDependencies.Missing(slot, new[] { first, second }).Count == 0, "所有依赖仍启用");
                var removed = AddonSaveDependencies.Missing(slot, new[] { first, campaign });
                Check(removed.Count == 1 && removed[0].Contains(second.Id) && removed[0].Contains(second.Name), "卸载或禁用时列出具体缺失包");
                var reinstall = new ModPackage { Id = second.Id, Name = "重装后的同一 MOD", Version = "1.3.0", PlayMode = "addon" };
                Check(AddonSaveDependencies.Missing(slot, new[] { first, reinstall }).Count == 0, "重装同 ID 后恢复可读");
                Check(File.ReadAllText(path) == record && File.ReadAllText(save) == "original GameSave bytes", "核对缺失不改依赖记录或游戏存档");
                var allMissing = AddonSaveDependencies.Missing(slot, new ModPackage[0]);
                Check(allMissing.Count == 2 && allMissing.Any(name => name.Contains(first.Name)), "中文、引号、反斜杠名称往返");

                foreach (string kind in new[] { "auto", "auto_free", "auto_battle" })
                {
                    string auto = ModSaveSlotPolicy.AddonAutoSlot(kind);
                    AddonSaveDependencies.Record(auto, new[] { second });
                    Check(AddonSaveDependencies.Missing(auto, new[] { first }).Count == 1, "自动档独立记录依赖");
                }
                AddonSaveDependencies.Record(slot, new[] { first });
                Check(AddonSaveDependencies.Missing(slot, new[] { first }).Count == 0, "成功新存档可替换依赖列表");
                AddonSaveDependencies.Record(ModSaveSlotPolicy.AddonManualSlot(3), new ModPackage[0]);
                Check(AddonSaveDependencies.Missing(ModSaveSlotPolicy.AddonManualSlot(3), new ModPackage[0]).Count == 0, "明确无依赖与缺少记录不同");
                string legacy = ModSaveSlotPolicy.AddonManualSlot(20);
                Check(!AddonSaveDependencies.HasRecord(legacy), "旧档不自动生成依赖");
                Reject(delegate { AddonSaveDependencies.Missing(legacy, new[] { first }); }, "无记录不能断言依赖完整", "无法确认");

                foreach (string rejected in new[] { "001", "auto", "mod_campaign_story", "../outside", "mod_addon_default_s001", "mod_addon_default_s021", null })
                {
                    Reject(delegate { AddonSaveDependencies.Record(rejected, new[] { first }); }, "非 addon 写入拒绝");
                    Reject(delegate { AddonSaveDependencies.Missing(rejected, new[] { first }); }, "非 addon 读取拒绝");
                    Reject(delegate { AddonSaveDependencies.HasRecord(rejected); }, "非 addon 文件查询拒绝");
                }
                foreach (string corrupt in new[] {
                    "{}", "{\"format\":2,\"slot\":\"" + slot + "\",\"mods\":[]}",
                    "{\"format\":1,\"slot\":\"001\",\"mods\":[]}",
                    "{\"format\":1,\"slot\":\"" + slot + "\",\"mods\":[{\"id\":\"../bad\",\"name\":\"bad\",\"version\":\"1.0.0\"}]}",
                    "{\"format\":1,\"slot\":\"" + slot + "\",\"mods\":[{\"id\":\"first_addon\"}]}",
                    "not json"
                })
                {
                    File.WriteAllText(path, corrupt);
                    Reject(delegate { AddonSaveDependencies.Missing(slot, new[] { first, second }); }, "损坏记录不能绕过缺包保护");
                    Check(File.ReadAllText(path) == corrupt, "拒绝损坏记录时不重写文件");
                }
                Check(File.ReadAllText(save) == "original GameSave bytes", "所有校验均保留游戏存档字节");
                Console.WriteLine("PASS: 附加模式存档依赖记录、缺包阻止、重装与槽归属测试。");
            }
            finally { Directory.Delete(root, true); }
        }

        private static void Check(bool condition, string label)
        {
            if (!condition) throw new Exception("附加模式依赖测试失败：" + label);
        }
        private static void Reject(Action action, string label, string message = null)
        {
            try { action(); }
            catch (ArgumentException exception) { if (message == null || exception.Message.Contains(message)) return; }
            catch (InvalidDataException exception) { if (message == null || exception.Message.Contains(message)) return; }
            throw new Exception("应拒绝：" + label);
        }
    }
}
