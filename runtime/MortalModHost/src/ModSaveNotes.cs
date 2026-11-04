using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace MortalModHost
{
    /// <summary>存档浏览器的备注和收藏。仅接受当前模式所属的已有隔离槽，不改 GameSave。</summary>
    internal sealed class ModSaveNote
    {
        internal string Text = "";
        internal bool Favorite;
    }

    internal static class ModSaveNotes
    {
        internal const int MaxLength = 240;

        internal static ModSaveNote Read(string root, string campaignId, bool addon, string slot)
        {
            string path = PathFor(root, campaignId, addon, slot);
            if (!File.Exists(path)) return new ModSaveNote();
            if (new FileInfo(path).Length > 8192)
                throw new InvalidDataException("存档备注文件过大");
            var data = MiniJson.Parse(File.ReadAllText(path, Encoding.UTF8)) as Dictionary<string, object>;
            object value;
            if (data == null || !data.TryGetValue("slot", out value) || value as string != slot)
                throw new InvalidDataException("存档备注归属不匹配");
            var note = new ModSaveNote();
            if (data.TryGetValue("text", out value)) note.Text = value as string ?? "";
            if (note.Text.Length > MaxLength) throw new InvalidDataException("存档备注过长");
            if (data.TryGetValue("favorite", out value)) note.Favorite = value is bool && (bool)value;
            return note;
        }

        internal static void Write(string root, string campaignId, bool addon, string slot, ModSaveNote note)
        {
            if (note == null) throw new ArgumentNullException(nameof(note));
            if ((note.Text ?? "").Length > MaxLength) throw new ArgumentException("备注最多 240 字");
            string path = PathFor(root, campaignId, addon, slot);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllText(path, "{\"slot\":" + Quote(slot) + ",\"text\":" + Quote(note.Text)
                + ",\"favorite\":" + (note.Favorite ? "true" : "false") + "}", new UTF8Encoding(false));
        }

        internal static void Delete(string root, string campaignId, bool addon, string slot)
        {
            string path = PathFor(root, campaignId, addon, slot);
            if (File.Exists(path)) File.Delete(path);
        }

        private static string PathFor(string root, string campaignId, bool addon, string slot)
        {
            if (!ModSaveSlotPolicy.OwnsBrowserSlot(campaignId, addon, slot, true))
                throw new ArgumentException("存档不属于当前模式或作品", nameof(slot));
            return Path.Combine(root, slot + ".json");
        }

        private static string Quote(string value)
        {
            var text = new StringBuilder("\"");
            foreach (char c in value ?? "")
            {
                switch (c)
                {
                    case '\\': text.Append("\\\\"); break;
                    case '"': text.Append("\\\""); break;
                    case '\n': text.Append("\\n"); break;
                    case '\r': text.Append("\\r"); break;
                    case '\t': text.Append("\\t"); break;
                    default:
                        if (c < 32) text.Append("\\u" + ((int)c).ToString("x4"));
                        else text.Append(c);
                        break;
                }
            }
            return text.Append('"').ToString();
        }
    }
}
