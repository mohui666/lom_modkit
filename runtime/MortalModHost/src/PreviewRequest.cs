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
        public bool FreshCampaign;
        public bool DeveloperMode;
        public string EventId, Target, From;
        public string SourceScript, EventScript, EventNode;
        public string VideoRenderId;
        public string RequestId;
        internal (long Length, long Created, long Written) SourceStamp { get; private set; }
        private string _source;

        internal static (long Length, long Created, long Written) FileStamp(string path)
        {
            var file = new FileInfo(path);
            return file.Exists ? (file.Length, file.CreationTimeUtc.Ticks, file.LastWriteTimeUtc.Ticks) : default;
        }

        internal bool IsSameFile(PreviewRequest other)
        {
            return other != null && SourceStamp == other.SourceStamp && _source == other._source;
        }

        internal bool IsCurrent(string path)
        {
            PreviewRequest current;
            string error;
            return TryRead(path, out current, out error) && IsSameFile(current);
        }

        public static bool TryRead(string path, out PreviewRequest request, out string error)
        {
            request = null;
            error = null;
            try
            {
                var stamp = FileStamp(path);
                string source;
                using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete))
                using (var reader = new StreamReader(file))
                    source = reader.ReadToEnd();
                if (FileStamp(path) != stamp) return false;
                var root = MiniJson.Parse(source) as Dictionary<string, object>;
                if (root == null)
                    throw new FormatException("顶层不是 JSON 对象");
                object format;
                if (!root.TryGetValue("format", out format) || !(format is double)
                    || (double)format != 1.0 && (double)format != 2.0 && (double)format != 3.0)
                    throw new FormatException("format 不是 1、2 或 3");
                if ((double)format == 1.0 && (root.ContainsKey("source_script")
                    || root.ContainsKey("event_script") || root.ContainsKey("event_node")))
                    throw new FormatException("source_script/event_script/event_node 需要 format 2，不能按旧试玩请求解释");
                var parsed = new PreviewRequest
                {
                    ModId = RequiredId(root, "mod_id"),
                    ScriptId = RequiredId(root, "script_id"),
                    NodeId = RequiredId(root, "node_id"),
                    SourceStamp = stamp,
                    _source = source
                };
                object video;
                if (root.TryGetValue("video_render_id", out video))
                {
                    parsed.VideoRenderId = video as string;
                    if (parsed.ModId != "lom_modkit_preview" || parsed.VideoRenderId == null
                        || !Regex.IsMatch(parsed.VideoRenderId, "^[a-f0-9]{32}$", RegexOptions.CultureInvariant))
                        throw new FormatException("video_render_id 只允许编辑器视频任务编号");
                }
                object tracking;
                if (root.TryGetValue("request_id", out tracking))
                {
                    parsed.RequestId = tracking as string;
                    if (parsed.ModId != "lom_modkit_preview" || parsed.RequestId == null
                        || !Regex.IsMatch(parsed.RequestId, "^[a-f0-9]{32}$", RegexOptions.CultureInvariant)
                        || parsed.VideoRenderId != null)
                        throw new FormatException("request_id 只允许独立的编辑器试玩状态编号");
                }
                object fresh;
                if (root.TryGetValue("fresh_campaign", out fresh))
                {
                    if (!(fresh is bool)) throw new FormatException("fresh_campaign 必须是布尔值");
                    parsed.FreshCampaign = (bool)fresh;
                }
                object developer;
                if (root.TryGetValue("developer_mode", out developer))
                {
                    if (!(developer is bool)) throw new FormatException("developer_mode 必须是布尔值");
                    parsed.DeveloperMode = (bool)developer;
                }
                if (parsed.DeveloperMode && ((double)format != 3.0
                    || parsed.ModId != "lom_modkit_preview" || !parsed.FreshCampaign))
                    throw new FormatException("developer_mode 需要 format 3 的固定编辑器隔离事件测试");
                if ((double)format == 3.0 && !parsed.DeveloperMode)
                    throw new FormatException("format 3 需要 developer_mode=true");
                if (parsed.FreshCampaign)
                {
                    if (parsed.ModId != "lom_modkit_preview") throw new FormatException("事件测试只允许固定的编辑器临时包");
                    parsed.EventId = RequiredId(root, "event_id");
                    if (root.ContainsKey("target")) parsed.Target = RequiredId(root, "target");
                    if (root.ContainsKey("from")) parsed.From = RequiredId(root, "from");
                    if (parsed.Target != null && parsed.Target.StartsWith("MOD_", StringComparison.Ordinal)
                        || parsed.From != null && (parsed.Target == null || parsed.From.StartsWith("MOD_", StringComparison.Ordinal)))
                        throw new FormatException("事件测试 target/from 必须是原版章节");
                }
                if (root.ContainsKey("source_script"))
                {
                    parsed.SourceScript = RequiredId(root, "source_script");
                    if (!parsed.FreshCampaign || parsed.Target == null
                        || parsed.SourceScript.StartsWith("MOD_", StringComparison.Ordinal))
                        throw new FormatException("source_script 需要原版入口事件测试及 target");
                }
                if (root.ContainsKey("event_script") != root.ContainsKey("event_node"))
                    throw new FormatException("event_script/event_node 必须成对提供");
                if (root.ContainsKey("event_script"))
                {
                    if (!parsed.FreshCampaign || parsed.RequestId == null)
                        throw new FormatException("event_script/event_node 需要 fresh_campaign 和 request_id");
                    parsed.EventScript = RequiredId(root, "event_script");
                    parsed.EventNode = RequiredId(root, "event_node");
                }
                if ((double)format >= 2.0 && (!parsed.FreshCampaign || parsed.RequestId == null
                    || parsed.EventScript == null || parsed.EventNode == null))
                    throw new FormatException("format 2/3 需要 fresh_campaign、request_id 和 event_script/event_node");
                request = parsed;
                return true;
            }
            catch (FileNotFoundException) { return false; }
            catch (DirectoryNotFoundException) { return false; }
            catch (Exception ex)
            {
                error = ex.Message;
                return false;
            }
        }

        internal ScenarioEvent ValidateEvent(ScenarioConfig scenario, Func<string, bool> originalExists)
        {
            var selected = scenario.Events.Find(e => e.Id == EventId);
            if (selected == null) throw new InvalidOperationException("测试事件不存在：" + EventId);
            if (EventScript != null && selected.Script != EventScript)
                throw new InvalidOperationException("测试事件脚本与 event_script 不匹配：" + EventId + " → " + selected.Script);
            if (Target != null && !originalExists(Target))
                throw new InvalidOperationException("当前游戏缺少测试原版章节：" + Target);
            if (SourceScript != null && (Target == null || !originalExists(SourceScript)))
                throw new InvalidOperationException("当前游戏缺少测试原版起点或 target：" + SourceScript);
            return selected;
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
