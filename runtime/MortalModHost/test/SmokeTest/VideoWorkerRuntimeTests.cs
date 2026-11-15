using System;
using System.IO;

namespace MortalModHost
{
    internal static class VideoWorkerRuntimeTests
    {
        internal static void Run()
        {
            const string id = "0123456789abcdef0123456789abcdef";
            Require(VideoWorkerRuntime.WorkerIdFromArgs(new[] { "Mortal.exe", "-screen-width", "1280" }) == null, "ordinary game became worker");
            Require(VideoWorkerRuntime.WorkerIdFromArgs(new[] { "Mortal.exe", "--lom-video-worker", id }) == id, "valid worker argument rejected");
            foreach (var args in new[]
            {
                new[] { "--lom-video-worker" }, new[] { "--lom-video-worker", id.ToUpperInvariant() },
                new[] { "--lom-video-worker", "../outside" }, new[] { "--lom-video-worker=" + id },
                new[] { "--lom-video-worker", id, "--lom-video-worker", id }
            }) Reject(() => VideoWorkerRuntime.WorkerIdFromArgs(args), "unsafe worker argument accepted");

            string job = Path.Combine(Path.GetTempPath(), "lom-worker-tests", id);
            string root = Path.Combine(job, "worker", "BepInEx");
            string profile = VideoWorkerRuntime.ProfileFor(job, root);
            Require(profile == Path.Combine(job, "worker", "profile"), "worker profile changed");
            Reject(() => VideoWorkerRuntime.ProfileFor(job, Path.Combine(job, "BepInEx")), "main game BepInEx accepted");
            Reject(() => VideoWorkerRuntime.ProfileFor(job, Path.Combine(job, "worker", "BepInEx-other")), "neighbor BepInEx accepted");
            Require(VideoWorkerRuntime.ProfileFile(profile, "Save_universe.dat") == Path.Combine(profile, "Save_universe.dat"), "universe escaped profile");
            Require(VideoWorkerRuntime.ProfileFile(profile, Path.Combine("Backup", "Save_universe.dat")) == Path.Combine(profile, "Backup", "Save_universe.dat"), "backup escaped profile");
            Reject(() => VideoWorkerRuntime.ProfileFile(profile, Path.Combine("..", "outside.dat")), "parent save path accepted");
            Reject(() => VideoWorkerRuntime.ProfileFile(profile, Path.Combine(job, "outside.dat")), "absolute save path accepted");

            var request = new PreviewRequest { ModId = "lom_modkit_preview", VideoRenderId = id };
            Require(VideoWorkerRuntime.Accepts(id, request), "own video request rejected");
            Require(!VideoWorkerRuntime.Accepts("fedcba9876543210fedcba9876543210", request), "different video request accepted");
            request.VideoRenderId = null;
            Require(!VideoWorkerRuntime.Accepts(id, request), "ordinary F5 accepted by worker");
            request.VideoRenderId = id; request.FreshCampaign = true;
            Require(!VideoWorkerRuntime.Accepts(id, request), "event playtest accepted by worker");
            request.FreshCampaign = false; request.ModId = "another_mod";
            Require(!VideoWorkerRuntime.Accepts(id, request), "other package accepted by worker");
            Console.WriteLine("PASS video worker origin, isolated save paths and own-request restriction (offline)");
        }

        private static void Require(bool value, string message) { if (!value) throw new Exception(message); }
        private static void Reject(Action action, string message)
        {
            bool rejected = false;
            try { action(); } catch (FormatException) { rejected = true; } catch (InvalidOperationException) { rejected = true; }
            Require(rejected, message);
        }
    }
}
