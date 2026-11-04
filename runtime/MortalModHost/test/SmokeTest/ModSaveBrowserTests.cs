using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class ModSaveBrowserTests
    {
        internal static void Run()
        {
            var slots = new HashSet<string>(StringComparer.Ordinal);
            for (int i = 1; i <= 20; i++)
            {
                string addon = ModSaveSlotPolicy.AddonManualSlot(i);
                Check(slots.Add(addon) && ModSaveSlotPolicy.IsAddonManualSlot(addon), "addon手动槽唯一");
                Check(ModSaveSlotPolicy.OwnsBrowserSlot(null, true, addon, false), "addon拥有自己的槽");
                Check(!ModSaveSlotPolicy.OwnsBrowserSlot("story_one", false, addon, true), "剧本不拥有addon槽");
                Check(!ModSaveSlotPolicy.OwnsBrowserSlot(null, true, i.ToString("000"), true), "addon不拥有原版槽");
                Check(!ModSaveSlotPolicy.OwnsBrowserSlot(null, true,
                    ModSaveSlotPolicy.IsolatedManualSlot("story_one", i), true), "addon不拥有剧本槽");
            }
            foreach (string kind in new[] { "auto", "auto_free", "auto_battle" })
            {
                string slot = ModSaveSlotPolicy.AddonAutoSlot(kind);
                Check(slots.Add(slot) && ModSaveSlotPolicy.IsAddonSlot(slot), "addon自动槽唯一");
                Check(!ModSaveSlotPolicy.IsAddonManualSlot(slot), "自动槽不得被当作手动槽");
                Check(!ModSaveSlotPolicy.OwnsBrowserSlot(null, true, slot, false), "保存页拒绝写自动槽");
                Check(ModSaveSlotPolicy.RedirectOfficialAutoSlot(kind,
                    ModSaveSlotPolicy.AddonManualSlot(2), null, null) == slot, "addon自动槽重定向");
            }
            Check(slots.Count == 23 && !ModSaveSlotPolicy.IsAddonSlot("mod_addon_default_s021")
                && !ModSaveSlotPolicy.IsAddonSlot("mod_addon_default_s001"), "无隐式额外栏位");
            Check(ModSaveSlotPolicy.IsModSlot(ModSaveSlotPolicy.AddonRoot)
                && ModSaveSlotPolicy.ObserveOfficialSlot("006", ModSaveSlotPolicy.AddonRoot) == "006",
                "addon不改变原版最近存档指针");

            string root = Path.Combine(Path.GetTempPath(), "lom_save_notes_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            try
            {
                string slot = ModSaveSlotPolicy.IsolatedManualSlot("story_one", 3);
                var note = new ModSaveNote { Text = "药铺前\n先看看\"功法\"\\物品", Favorite = true };
                ModSaveNotes.Write(root, "story_one", false, slot, note);
                ModSaveNote read = ModSaveNotes.Read(root, "story_one", false, slot);
                Check(read.Text == note.Text && read.Favorite, "中文、引号、反斜杠和换行备注可持久化");
                Reject(delegate { ModSaveNotes.Read(root, "story_two", false, slot); }, "跨作品读备注");
                Reject(delegate { ModSaveNotes.Write(root, null, true, slot, note); }, "addon写剧本备注");
                Reject(delegate { ModSaveNotes.Write(root, null, true, "001", note); }, "写原版槽备注");
                Reject(delegate { ModSaveNotes.Write(root, "story_one", false, "../outside", note); }, "备注路径穿越");
                Reject(delegate { ModSaveNotes.Write(root, "story_one", false, slot,
                    new ModSaveNote { Text = new string('字', ModSaveNotes.MaxLength + 1) }); }, "过长备注");

                string addon = ModSaveSlotPolicy.AddonAutoSlot("auto_free");
                ModSaveNotes.Write(root, null, true, addon, new ModSaveNote { Text = "月底养成" });
                Check(ModSaveNotes.Read(root, null, true, addon).Text == "月底养成", "自动档可备注");
                File.WriteAllText(Path.Combine(root, slot + ".json"), "{\"slot\":\"001\",\"text\":\"不应读入\"}");
                Reject(delegate { ModSaveNotes.Read(root, "story_one", false, slot); }, "元数据归属不一致");
                ModSaveNotes.Delete(root, null, true, addon);
                Check(!File.Exists(Path.Combine(root, addon + ".json")), "删除只移除此槽备注");
                Check(File.Exists(Path.Combine(root, slot + ".json")), "删除不影响其他作品备注");
            }
            finally
            {
                Directory.Delete(root, true);
            }
            Console.WriteLine("PASS: MOD 存档浏览器隔离与备注测试。");
        }

        private static void Reject(Action action, string label)
        {
            try { action(); }
            catch (ArgumentException) { return; }
            catch (InvalidDataException) { return; }
            throw new Exception("应拒绝：" + label);
        }

        private static void Check(bool condition, string label)
        {
            if (!condition) throw new Exception("存档浏览器测试失败：" + label);
        }
    }
}
