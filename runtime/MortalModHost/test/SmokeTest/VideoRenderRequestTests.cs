using System;
using System.IO;

namespace MortalModHost
{
    internal static class VideoRenderRequestTests
    {
        internal static void Run()
        {
            string directory = Path.Combine(Path.GetTempPath(), "lom-video-request-" + Guid.NewGuid().ToString("N"));
            System.IO.Directory.CreateDirectory(directory);
            try
            {
                string path = Path.Combine(directory, "preview.json");
                string ordinary = "{\"format\":1,\"mod_id\":\"lom_modkit_preview\",\"script_id\":\"main\",\"node_id\":\"start\"";
                File.WriteAllText(path, ordinary + "}");
                PreviewRequest preview; string error;
                Require(PreviewRequest.TryRead(path, out preview, out error) && preview.VideoRenderId == null, "old F5 request rejected");
                foreach (string id in new[] { "../outside", "", "0123" })
                {
                    File.WriteAllText(path, ordinary + ",\"video_render_id\":\"" + id + "\"}");
                    Require(!PreviewRequest.TryRead(path, out preview, out error), "unsafe video id accepted");
                }
                File.WriteAllText(path, ordinary + ",\"video_render_id\":\"0123456789abcdef0123456789abcdef\"}");
                Require(PreviewRequest.TryRead(path, out preview, out error), error);
                string encoder = Path.Combine(directory, "ffmpeg.exe"); File.WriteAllText(encoder, "test encoder path");
                string valid = "{\"format\":1,\"width\":1280,\"height\":720,\"fps\":30,\"choice_hold\":1.5,\"max_seconds\":3600,"
                    + "\"encoder\":\"" + encoder.Replace("\\", "\\\\") + "\",\"auto_speed\":\"game\",\"entry_story\":\"main\",\"entry_node\":\"start\","
                    + "\"choices\":{\"main/route\":1},\"terminal_nodes\":[\"main/end\"],\"terminal_cards\":[]}";
                string request = Path.Combine(directory, "request.json"); File.WriteAllText(request, valid);
                var parsed = VideoRenderRequest.Read(directory, preview);
                Require(parsed.Choices["main/route"] == 1 && parsed.Terminals.Contains("main/end"), "route was changed");
                foreach (string invalid in new[] { valid.Replace("\"fps\":30", "\"fps\":30.5"),
                    valid.Replace("\"main/route\":1", "\"main/route\":-1"), valid.Replace("\"entry_node\":\"start\"", "\"entry_node\":\"other\"") })
                {
                    File.WriteAllText(request, invalid);
                    bool rejected = false;
                    try { VideoRenderRequest.Read(directory, preview); } catch (FormatException) { rejected = true; }
                    Require(rejected, "invalid video request accepted");
                }
            }
            finally { System.IO.Directory.Delete(directory, true); }
            Console.WriteLine("PASS native video request routing, old F5 and invalid job rejection (offline)");
        }

        private static void Require(bool value, string message) { if (!value) throw new Exception(message); }
    }
}
