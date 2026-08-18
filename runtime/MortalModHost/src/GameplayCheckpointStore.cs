using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// 保存 MOD Gameplay 自动档/手动档的宿主上下文。
    /// 原版 GameSave 没有 MOD 的 combat/battle 配置；只保存 CurrentSceneKey 会让
    /// 读档重新进入 CL_MORTALMODHOST_RUNTIME，却没有 PendingCombat 可供补丁接管。
    /// 文件名直接绑定实际的隔离槽，包 id、campaign_id 和完整 SHA-256 全部复核。
    /// </summary>
    internal static class GameplayCheckpointStore
    {
        private const int Format = 1;
        private const int MaxBytes = 256 * 1024;
        private static string _root;

        internal static void Initialize(string root)
        {
            if (string.IsNullOrWhiteSpace(root))
                throw new ArgumentException("Gameplay checkpoint 目录不能为空");
            _root = Path.GetFullPath(root);
        }

        internal static void Save(
            ModPackage package, string saveSlot, string sourceSlot)
        {
            ValidateSlot(package, saveSlot);
            if (!string.IsNullOrEmpty(sourceSlot)) ValidateSlot(package, sourceSlot);
            GameplayCheckpoint checkpoint = GameplaySession.CaptureCheckpoint(package);
            if (checkpoint == null)
            {
                Clear(package, saveSlot);
                return;
            }
            Directory.CreateDirectory(_root);
            string target = PathFor(saveSlot);
            string temporary = target + ".tmp." + Guid.NewGuid().ToString("N");
            byte[] payload = new UTF8Encoding(false, true).GetBytes(
                Serialize(checkpoint, saveSlot, sourceSlot));
            if (payload.Length > MaxBytes)
                throw new InvalidDataException("Gameplay checkpoint 超过 256 KiB");
            try
            {
                using (var stream = new FileStream(
                    temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    stream.Write(payload, 0, payload.Length);
                    stream.Flush(true);
                }
                if (File.Exists(target)) File.Replace(temporary, target, null);
                else File.Move(temporary, target);
            }
            finally
            {
                if (File.Exists(temporary)) File.Delete(temporary);
            }
        }

        internal static bool TryLoad(
            ModPackage package, string saveSlot, out GameplayCheckpoint checkpoint)
        {
            checkpoint = null;
            try
            {
                ValidateSlot(package, saveSlot);
                string path = PathFor(saveSlot);
                if (!File.Exists(path)) return false;
                var info = new FileInfo(path);
                if (info.Length > MaxBytes) return false;
                object parsed = MiniJson.Parse(
                    File.ReadAllText(path, new UTF8Encoding(false, true)));
                var root = parsed as Dictionary<string, object>;
                if (root == null || ReadInt(root, "format") != Format
                    || ReadString(root, "save_slot") != saveSlot
                    || ReadString(root, "mod_id") != package.Id
                    || ReadString(root, "campaign_id") != package.CampaignId
                    || ReadString(root, "package_fingerprint")
                        != package.PackageFingerprint)
                    return false;
                string sourceSlot = ReadString(root, "source_slot");
                if (!string.IsNullOrEmpty(sourceSlot)) ValidateSlot(package, sourceSlot);

                var result = new GameplayCheckpoint
                {
                    ModId = package.Id,
                    CampaignId = package.CampaignId,
                    PackageFingerprint = package.PackageFingerprint,
                    Kind = ReadString(root, "kind"),
                    Story = ReadString(root, "story"),
                    Node = ReadString(root, "node"),
                    WinTarget = ReadString(root, "win_target"),
                    LoseTarget = ReadString(root, "lose_target"),
                    Result = ReadString(root, "result"),
                    CombatDisplayName = ReadString(root, "combat_display_name"),
                    CombatIdleAddress = ReadString(root, "combat_idle_address"),
                    StoryBackground = ReadOptionalString(root, "story_background"),
                    SourceSlot = sourceSlot,
                    SaveSlot = saveSlot,
                };
                object rawConfig;
                var config = root.TryGetValue("config", out rawConfig)
                    ? rawConfig as Dictionary<string, object> : null;
                if (config == null || config.Count > 128) return false;
                foreach (KeyValuePair<string, object> pair in config)
                    result.Config.Add(pair.Key, pair.Value as string ?? "");
                checkpoint = result;
                return true;
            }
            catch
            {
                checkpoint = null;
                return false;
            }
        }

        internal static void Clear(ModPackage package, string saveSlot)
        {
            try
            {
                ValidateSlot(package, saveSlot);
                string path = PathFor(saveSlot);
                if (File.Exists(path)) File.Delete(path);
            }
            catch { }
        }

        private static void ValidateSlot(ModPackage package, string slot)
        {
            if (package == null || string.IsNullOrEmpty(package.Id)
                || !CampaignIdentity.IsValid(package.CampaignId)
                || string.IsNullOrEmpty(package.PackageFingerprint)
                || package.PackageFingerprint.Length != 64
                || !CampaignIdentity.OwnsSlot(package.CampaignId, slot))
                throw new InvalidOperationException("Gameplay checkpoint 槽身份不匹配");
        }

        private static string PathFor(string slot)
        {
            if (string.IsNullOrEmpty(_root))
                throw new InvalidOperationException("Gameplay checkpoint 存储尚未初始化");
            string path = Path.GetFullPath(Path.Combine(_root, slot + ".gameplay.json"));
            string prefix = _root.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)
                + Path.DirectorySeparatorChar;
            if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("Gameplay checkpoint 路径越出 Host 数据目录");
            return path;
        }

        private static string Serialize(
            GameplayCheckpoint checkpoint, string saveSlot, string sourceSlot)
        {
            var sb = new StringBuilder();
            sb.Append("{\n  \"format\": 1,")
              .Append("\n  \"save_slot\": ").Append(Quote(saveSlot)).Append(',')
              .Append("\n  \"source_slot\": ").Append(Quote(sourceSlot ?? "")).Append(',')
              .Append("\n  \"mod_id\": ").Append(Quote(checkpoint.ModId)).Append(',')
              .Append("\n  \"campaign_id\": ").Append(Quote(checkpoint.CampaignId)).Append(',')
              .Append("\n  \"package_fingerprint\": ").Append(Quote(checkpoint.PackageFingerprint)).Append(',')
              .Append("\n  \"kind\": ").Append(Quote(checkpoint.Kind)).Append(',')
              .Append("\n  \"story\": ").Append(Quote(checkpoint.Story)).Append(',')
              .Append("\n  \"node\": ").Append(Quote(checkpoint.Node)).Append(',')
              .Append("\n  \"win_target\": ").Append(Quote(checkpoint.WinTarget)).Append(',')
              .Append("\n  \"lose_target\": ").Append(Quote(checkpoint.LoseTarget)).Append(',')
              .Append("\n  \"result\": ").Append(Quote(checkpoint.Result)).Append(',')
              .Append("\n  \"combat_display_name\": ").Append(Quote(checkpoint.CombatDisplayName)).Append(',')
              .Append("\n  \"combat_idle_address\": ").Append(Quote(checkpoint.CombatIdleAddress)).Append(',')
              .Append("\n  \"story_background\": ").Append(Quote(checkpoint.StoryBackground)).Append(',')
              .Append("\n  \"config\": {");
            bool first = true;
            foreach (string key in checkpoint.Config.Keys.OrderBy(value => value, StringComparer.Ordinal))
            {
                if (!first) sb.Append(',');
                sb.Append("\n    ").Append(Quote(key)).Append(": ")
                  .Append(Quote(checkpoint.Config[key]));
                first = false;
            }
            if (!first) sb.Append('\n');
            sb.Append("  }\n}\n");
            return sb.ToString();
        }

        private static int ReadInt(Dictionary<string, object> dict, string key)
        {
            object value;
            if (!dict.TryGetValue(key, out value) || !(value is double))
                throw new InvalidDataException("Gameplay checkpoint 缺少整数 " + key);
            return (int)(double)value;
        }

        private static string ReadString(Dictionary<string, object> dict, string key)
        {
            object value;
            string text;
            if (!dict.TryGetValue(key, out value) || (text = value as string) == null
                || text.Length > 8192)
                throw new InvalidDataException("Gameplay checkpoint 缺少字符串 " + key);
            return text;
        }

        private static string ReadOptionalString(Dictionary<string, object> dict, string key)
        {
            object value;
            string text;
            if (!dict.TryGetValue(key, out value)) return "";
            text = value as string;
            return text != null && text.Length <= 8192 ? text : "";
        }

        private static string Quote(string value)
        {
            var sb = new StringBuilder("\"");
            foreach (char c in value ?? "")
            {
                switch (c)
                {
                    case '\\': sb.Append("\\\\"); break;
                    case '"': sb.Append("\\\""); break;
                    case '\b': sb.Append("\\b"); break;
                    case '\f': sb.Append("\\f"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("X4"));
                        else sb.Append(c);
                        break;
                }
            }
            return sb.Append('"').ToString();
        }
    }
}
