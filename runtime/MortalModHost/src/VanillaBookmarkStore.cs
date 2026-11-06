using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;

namespace MortalModHost
{
    /// <summary>Author bookmarks are independent files, never game saves or project assets.</summary>
    internal static class VanillaBookmarkStore
    {
        internal static string Root
        {
            get { return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "lom_modkit", "rust", "story-bookmarks"); }
        }

        internal static string Write(string root, string script, string key, string text,
            string game, int year, int month, int stage)
        {
            if (!VanillaStoryPatch.ValidTarget(script)) throw new ArgumentException("只能标记原版章节");
            Directory.CreateDirectory(root);
            DateTime now = DateTime.UtcNow;
            string file = Path.Combine(root, "mark-" + now.Ticks.ToString("D19", CultureInfo.InvariantCulture)
                + "-" + Guid.NewGuid().ToString("N") + ".json");
            var record = new Dictionary<string,object> {
                {"schema",1},{"script",script},{"text_key",key ?? ""},
                {"text",text != null && text.Length > 2048 ? text.Substring(0,2048) : text ?? ""},
                {"game_dir",game ?? ""},{"year",year},{"month",month},{"stage",stage},
                {"captured_utc",now.ToString("o",CultureInfo.InvariantCulture)}
            };
            string temporary = file+".tmp";
            try
            {
                using (var stream = new FileStream(temporary,FileMode.CreateNew,FileAccess.Write,FileShare.None))
                using (var writer = new StreamWriter(stream,new UTF8Encoding(false)))
                    writer.Write(ScenarioState.ToJson(record));
                File.Move(temporary,file);
            }
            finally { if (File.Exists(temporary)) File.Delete(temporary); }
            return file;
        }

        internal static void WriteTimeline(string root, object timeline)
        {
            Directory.CreateDirectory(root);
            string temporary = Path.Combine(root,"timeline-"+Guid.NewGuid().ToString("N")+".tmp");
            string path = Path.Combine(root,"timeline.json");
            try
            {
                using (var stream = new FileStream(temporary,FileMode.CreateNew,FileAccess.Write,FileShare.None))
                using (var writer = new StreamWriter(stream,new UTF8Encoding(false)))
                    writer.Write(ScenarioState.ToJson(timeline));
                if (File.Exists(path)) File.Replace(temporary,path,null);
                else File.Move(temporary,path);
            }
            finally { if (File.Exists(temporary)) File.Delete(temporary); }
        }
    }
}
