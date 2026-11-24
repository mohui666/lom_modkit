using System;
using System.IO;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;

namespace MortalModHost
{
    /// <summary>Copies an official save snapshot into an empty add-on slot using the game's save format.</summary>
    internal static class AddonOriginalProgress
    {
        private static readonly MethodInfo SavePath = AccessTools.Method(
            typeof(SaveSystem), "GetSaveDataFile", new[] { typeof(string) });
        private static readonly MethodInfo WriteSnapshot = AccessTools.Method(
            typeof(SaveSystem), "ExecuteSaveData", new[] { typeof(string), typeof(object) });

        internal static GameSave Read(SaveSystem saves, string slot)
        {
            if (ModSaveSlotPolicy.IsOfficialManualSlot(slot))
                return saves.GetSaveData(slot);
            if (!ModSaveSlotPolicy.IsOfficialAutoSlot(slot))
                throw new ArgumentException("只能读取原版手动存档或原版自动存档。", nameof(slot));
            AutoGameSave automatic = saves.GetAutoSaveData(slot);
            return automatic != null && ModSaveSlotPolicy.IsOfficialManualSlot(automatic.Slot)
                ? automatic.GameSave : null;
        }

        internal static string FindEmptySlot(SaveSystem saves, string source)
        {
            int preferred = ModSaveSlotPolicy.IsOfficialManualSlot(source) ? int.Parse(source) : 0;
            if (preferred > 0 && preferred <= ModSaveSlotPolicy.OfficialManualCount)
            {
                string slot = ModSaveSlotPolicy.AddonManualSlot(preferred);
                if (!File.Exists(PathFor(saves, slot))) return slot;
            }
            for (int index = 1; index <= ModSaveSlotPolicy.OfficialManualCount; index++)
            {
                string slot = ModSaveSlotPolicy.AddonManualSlot(index);
                if (!File.Exists(PathFor(saves, slot))) return slot;
            }
            throw new InvalidOperationException("原版＋MOD 的手动栏位已满。请先在该模式的存档页腾出一个空栏位。");
        }

        internal static void ValidateSnapshot(GameSave snapshot)
        {
            if (snapshot == null || string.IsNullOrWhiteSpace(snapshot.CurrentScene))
                throw new InvalidOperationException("原版存档不存在或已损坏。");
            if (IsModScript(snapshot.StartStoryScript) || IsModScript(snapshot.CurrentStoryScript)
                || snapshot.CurrentScene == "Combat" && snapshot.CurrentSceneKey == GameplaySceneTransition.ModCombatKey)
                throw new InvalidOperationException("此存档包含 MOD 剧情或战斗上下文，请从对应玩法读取。");
        }

        internal static bool IsBattle(GameSave snapshot)
        {
            return snapshot != null && (snapshot.CurrentScene == "Combat" || snapshot.CurrentScene == "Battle");
        }

        internal static void Commit(SaveSystem saves, string slot, GameSave snapshot, Action persistAddonState)
        {
            if (WriteSnapshot == null) throw new MissingMethodException("SaveSystem.ExecuteSaveData(string, object)");
            if (!ModSaveSlotPolicy.IsAddonManualSlot(slot))
                throw new ArgumentException("只能写入原版＋MOD 的空手动栏位。", nameof(slot));
            string target = PathFor(saves, slot);
            if (File.Exists(target))
                throw new InvalidOperationException("目标栏位已有进度，请重新选择原版存档。");
            string staged = target + ".import." + Guid.NewGuid().ToString("N");
            try
            {
                WriteSnapshot.Invoke(saves, new object[] { staged, snapshot });
                if (!File.Exists(staged))
                    throw new IOException("原版进度未能写入原版＋MOD 存档，请检查游戏日志。");
                // Move without overwrite so a slot filled since selection cannot be replaced.
                File.Move(staged, target);
                try { persistAddonState(); }
                catch
                {
                    File.Delete(target);
                    throw;
                }
            }
            finally
            {
                File.Delete(staged);
                File.Delete(staged + ".tmp");
            }
        }

        private static string PathFor(SaveSystem saves, string slot)
        {
            if (SavePath == null) throw new MissingMethodException("SaveSystem.GetSaveDataFile(string)");
            return (string)SavePath.Invoke(saves, new object[] { slot });
        }

        private static bool IsModScript(string script)
        {
            return script != null && script.StartsWith("MOD_", StringComparison.Ordinal);
        }
    }
}
