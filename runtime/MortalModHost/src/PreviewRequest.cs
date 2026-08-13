using System;
using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>编辑器写入的单次试玩请求。文件协议保持极小，便于旧请求安全拒绝。</summary>
    internal sealed class PreviewRequest
    {
        private static readonly Regex SafeId = new Regex("^[A-Za-z0-9_-]+$", RegexOptions.CultureInvariant);

        public string ModId;
        public string ScriptId;
        public string NodeId;

        public static bool TryRead(string path, out PreviewRequest request, out string error)
        {
            request = null;
            error = null;
            try
            {
                var root = MiniJson.Parse(File.ReadAllText(path)) as Dictionary<string, object>;
                if (root == null)
                    throw new FormatException("顶层不是 JSON 对象");
                object format;
                if (!root.TryGetValue("format", out format) || !(format is double) || (double)format != 1.0)
                    throw new FormatException("format 不是 1");
                var parsed = new PreviewRequest
                {
                    ModId = RequiredId(root, "mod_id"),
                    ScriptId = RequiredId(root, "script_id"),
                    NodeId = RequiredId(root, "node_id")
                };
                request = parsed;
                return true;
            }
            catch (Exception ex)
            {
                error = ex.Message;
                return false;
            }
        }

        private static string RequiredId(Dictionary<string, object> root, string key)
        {
            object value;
            var text = root.TryGetValue(key, out value) ? value as string : null;
            if (string.IsNullOrEmpty(text) || !SafeId.IsMatch(text))
                throw new FormatException(key + " 缺失或格式不正确");
            return text;
        }
    }
}
