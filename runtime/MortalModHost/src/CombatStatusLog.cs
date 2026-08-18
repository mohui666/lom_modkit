using System;
using System.IO;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// 临时诊断：把决斗状态页的六维/评语写入插件目录 combat-status.log，
    /// 方便对照 F5 画面。正式修复确认后删除。
    /// </summary>
    internal static class CombatStatusLog
    {
        private static readonly object Gate = new object();
        private static string _path;

        internal static string Path
        {
            get
            {
                if (_path == null)
                {
                    string dir = System.IO.Path.GetDirectoryName(
                        typeof(CombatStatusLog).Assembly.Location);
                    _path = System.IO.Path.Combine(dir ?? ".", "combat-status.log");
                }
                return _path;
            }
        }

        internal static void Write(string line)
        {
            try
            {
                lock (Gate)
                {
                    string row = DateTime.Now.ToString("HH:mm:ss.fff") + " " + line
                        + Environment.NewLine;
                    File.AppendAllText(Path, row, Encoding.UTF8);
                }
                LuaManagerPatch.Log?.LogInfo("[combat-status] " + line);
                if (RuntimeTrace.Active)
                    RuntimeTrace.Record("combat_status", RuntimeTrace.CurrentNode, line);
            }
            catch
            {
            }
        }

        internal static void DumpConfig(string reason)
        {
            var text = new StringBuilder();
            text.Append(reason);
            text.Append(" pending=").Append(GameplaySession.PendingCombat);
            string[] keys =
            {
                "character", "max_health", "health", "max_stamina", "stamina",
                "strength", "stamina_power", "dexterity", "sword",
                "fist", "martial_weapon", "talking", "disposition", "behaviour",
                "karma", "training", "internal",
                "player_max_health", "player_health", "player_max_stamina", "player_stamina",
                "player_strength", "player_stamina_power", "player_talents"
            };
            for (int i = 0; i < keys.Length; i++)
            {
                text.Append(' ').Append(keys[i]).Append('=');
                text.Append(GameplaySession.HasConfig(keys[i])
                    ? GameplaySession.ConfigString(keys[i]) : "MISSING");
            }
            Write(text.ToString());
        }
    }
}
