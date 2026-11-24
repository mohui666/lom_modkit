using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal sealed class VideoRenderRequest
    {
        internal int Width, Height, Fps;
        internal double ChoiceHold, MaxSeconds;
        internal string Encoder, Speed;
        internal string EntryStory, EntryNode, EndNode;
        internal readonly Dictionary<string, int> Choices = new Dictionary<string, int>(StringComparer.Ordinal);
        internal readonly HashSet<string> Terminals = new HashSet<string>(StringComparer.Ordinal);
        internal readonly HashSet<string> Cards = new HashSet<string>(StringComparer.Ordinal);

        internal static string DirectoryFor(string id)
        {
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "lom_modkit", "rust", "video-renders", id);
        }

        internal static VideoRenderRequest Read(string directory, PreviewRequest preview)
        {
            var root = MiniJson.Parse(File.ReadAllText(Path.Combine(directory, "request.json"))) as Dictionary<string, object>;
            if (root == null) throw new FormatException("视频请求格式无效");
            int format = Integer(root, "format");
            if ((format != 1 && format != 2) || (format == 2) != root.ContainsKey("end_node"))
                throw new FormatException("视频请求格式与录制范围不匹配");
            var result = new VideoRenderRequest
            {
                Width = Integer(root, "width"), Height = Integer(root, "height"),
                Fps = Integer(root, "fps"), ChoiceHold = Number(root, "choice_hold"),
                MaxSeconds = Number(root, "max_seconds"), Encoder = Text(root, "encoder"), Speed = Text(root, "auto_speed"),
                EntryStory = Text(root, "entry_story"), EntryNode = Text(root, "entry_node"),
                EndNode = format == 2 ? PreviewRequest.RequiredId(root, "end_node") : null
            };
            if (!((result.Width == 1280 && result.Height == 720) || (result.Width == 1920 && result.Height == 1080))
                || result.Fps != 30 && result.Fps != 60) throw new FormatException("视频分辨率或帧率无效");
            if (result.ChoiceHold < 0 || result.ChoiceHold > 30 || result.MaxSeconds <= 0 || result.MaxSeconds > 3600)
                throw new FormatException("视频时长设置无效");
            if (result.Speed != "game" && result.Speed != "slow" && result.Speed != "standard" && result.Speed != "fast")
                throw new FormatException("视频自动播放速度无效");
            if (!Path.IsPathRooted(result.Encoder) || !File.Exists(result.Encoder)
                || !string.Equals(Path.GetFileName(result.Encoder), "ffmpeg.exe", StringComparison.OrdinalIgnoreCase))
                throw new FormatException("找不到视频编码器");
            if (result.EntryStory != preview.ScriptId || result.EntryNode != preview.NodeId)
                throw new FormatException("视频任务与试玩入口不匹配");
            object choices;
            var routes = root.TryGetValue("choices", out choices) ? choices as Dictionary<string, object> : null;
            if (routes == null) throw new FormatException("视频预设选项无效");
            foreach (var choice in routes)
            {
                if (!(choice.Value is double) || (double)choice.Value < 0 || (double)choice.Value > 1000
                    || Math.Floor((double)choice.Value) != (double)choice.Value)
                    throw new FormatException("视频预设选项序号无效：" + choice.Key);
                result.Choices.Add(choice.Key, (int)(double)choice.Value);
            }
            ReadSet(root, "terminal_nodes", result.Terminals);
            ReadSet(root, "terminal_cards", result.Cards);
            return result;
        }

        private static string Text(Dictionary<string, object> root, string key)
        {
            object value;
            var text = root.TryGetValue(key, out value) ? value as string : null;
            if (string.IsNullOrEmpty(text)) throw new FormatException("视频请求缺少 " + key);
            return text;
        }

        private static double Number(Dictionary<string, object> root, string key)
        {
            object value;
            if (!root.TryGetValue(key, out value) || !(value is double) || double.IsNaN((double)value) || double.IsInfinity((double)value))
                throw new FormatException("视频请求数字无效：" + key);
            return (double)value;
        }

        private static int Integer(Dictionary<string, object> root, string key)
        {
            double number = Number(root, key);
            if (Math.Floor(number) != number || number < int.MinValue || number > int.MaxValue)
                throw new FormatException("视频请求整数无效：" + key);
            return (int)number;
        }

        private static void ReadSet(Dictionary<string, object> root, string key, HashSet<string> target)
        {
            object value;
            var list = root.TryGetValue(key, out value) ? value as List<object> : null;
            if (list == null) throw new FormatException("视频请求列表无效：" + key);
            foreach (object item in list)
            {
                var text = item as string;
                if (string.IsNullOrEmpty(text)) throw new FormatException("视频请求位置无效：" + key);
                target.Add(text);
            }
        }
    }
}
