using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;
#if !LOM_OFFLINE_SMOKE
using System.Diagnostics;
using System.Reflection;
using BepInEx;
using HarmonyLib;
using Mortal.Core;
using UnityEngine.InputSystem;
#endif

namespace MortalModHost
{
    // A worker is a separate Player in a private desktop. Its save prefixes must
    // survive ordinary Host cleanup and remain installed until the process exits.
    internal static class VideoWorkerRuntime
    {
        private const string Flag = "--lom-video-worker";

        internal static string WorkerIdFromArgs(string[] args)
        {
            string id = null;
            for (int i = 0; i < args.Length; i++)
            {
                if (args[i].StartsWith(Flag + "=", StringComparison.Ordinal))
                    throw new FormatException("视频后台参数需要独立的任务编号");
                if (args[i] != Flag) continue;
                if (id != null || ++i >= args.Length
                    || !Regex.IsMatch(args[i], "\\A[a-f0-9]{32}\\z", RegexOptions.CultureInvariant))
                    throw new FormatException("视频后台任务编号无效");
                id = args[i];
            }
            return id;
        }

        internal static string ProfileFor(string jobDirectory, string bepinexRoot)
        {
            string expected = FullDirectory(Path.Combine(jobDirectory, "worker", "BepInEx"));
            if (!string.Equals(expected, FullDirectory(bepinexRoot), StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("视频后台只能从本任务的独立 BepInEx 目录启动");
            return FullDirectory(Path.Combine(jobDirectory, "worker", "profile"));
        }

        internal static string ProfileFile(string profile, string relative)
        {
            string root = FullDirectory(profile);
            string result = Path.GetFullPath(Path.Combine(root, relative));
            if (!result.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("视频后台存档路径超出独立目录");
            return result;
        }

        internal static bool Accepts(string id, PreviewRequest request)
        {
            return request != null && request.ModId == "lom_modkit_preview"
                && request.VideoRenderId == id && !request.FreshCampaign && request.RequestId == null;
        }

        private static string FullDirectory(string path)
        {
            return Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }

#if !LOM_OFFLINE_SMOKE
        private static string _id, _directory, _profile;
        private static bool _ready;
        private static int? _exitCode;
        internal static bool Active { get { return _id != null; } }

        // This is the first operation in Plugin.Awake, before Config, game save
        // initialization or any InputSystem access. Failure must terminate this
        // process rather than leave an unisolated game continuing to initialize.
        internal static void Initialize()
        {
            try
            {
                string id = WorkerIdFromArgs(Environment.GetCommandLineArgs());
                if (id == null) return;
                string directory = VideoRenderRequest.DirectoryFor(id);
                string profile = ProfileFor(directory, Paths.BepInExRootPath);
                if (!File.Exists(Path.Combine(directory, "request.json")))
                    throw new FileNotFoundException("视频后台任务请求不存在");
                _id = id; _directory = directory; _profile = profile;
                Directory.CreateDirectory(profile);
                var harmony = new Harmony(Plugin.GUID + ".video-worker");
                Patch(harmony, typeof(SaveSystem), "GetUserFolder", Type.EmptyTypes, "UserFolder");
                Patch(harmony, typeof(SaveSystem), "GetSaveDataFile", new[] { typeof(string) }, "SaveFile");
                Patch(harmony, typeof(SaveSystem), "GetLegendDataFile", new[] { typeof(int) }, "LegendFile");
                // Native Backup bypasses GetUserFolder and needs its own prefix.
                Patch(harmony, typeof(SaveSystem), "GetBackupDataFile", new[] { typeof(string) }, "BackupFile");
                Patch(harmony, typeof(SystemSettings), "IsRunningOnSteamDeck", Type.EmptyTypes, "DesktopLayout");
                // Older builds poll Keyboard.current in this component. In 1.0.6001.2,
                // SceneController handles F11 and already checks whether a keyboard exists.
                var legacyToggle = typeof(ToggleCanvasGroup).GetMethod("Update",
                    BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.DeclaredOnly);
                if (legacyToggle != null)
                    harmony.Patch(legacyToggle, prefix: new HarmonyMethod(typeof(VideoWorkerRuntime), "HasKeyboard"));
            }
            catch (Exception error) { Fatal(error); }
        }

        private static void Patch(Harmony harmony, Type type, string name, Type[] args, string prefix)
        {
            MethodInfo target = AccessTools.Method(type, name, args);
            if (target == null) throw new MissingMethodException(type.FullName, name);
            harmony.Patch(target, prefix: new HarmonyMethod(typeof(VideoWorkerRuntime), prefix));
        }

        internal static void InitializationFailed(Exception error)
        {
            if (Active) Fatal(error);
        }

        internal static void RequireRuntime(bool ready)
        {
            if (Active && !ready) Fatal(new InvalidOperationException("视频后台 Host 未能完成运行时初始化"));
        }

        // Runs from Update, after InputSystem.InitializeData has returned. Never
        // add a virtual device on the Awake stack that initializes InputSystem.
        internal static bool Tick()
        {
            if (!Active) return true;
            if (_exitCode.HasValue) Environment.Exit(_exitCode.Value);
            try
            {
                if (!VideoRenderRuntime.Busy && File.Exists(Path.Combine(_directory, "cancel")))
                {
                    VideoRenderRuntime.Status(_directory, "cancelled", 0, 0, "视频生成已取消");
                    Complete(_id, "cancelled");
                    return false;
                }
                if (_ready) return true;
                SaveSystem saves = SaveSystem.Instance;
                if (saves == null) return false;
                var paths = VerifySavePaths(saves);
                if (Keyboard.current == null) InputSystem.AddDevice<Keyboard>("LoMVideoWorkerKeyboard");
                string marker = Path.Combine(_directory, "worker-ready.json");
                File.WriteAllText(marker + ".tmp", ScenarioState.ToJson(new Dictionary<string, object>
                {
                    { "format", 1 }, { "job_id", _id }, { "pid", Process.GetCurrentProcess().Id },
                    { "profile", _profile }, { "save_paths", paths }
                }), new UTF8Encoding(false));
                File.Move(marker + ".tmp", marker);
                _ready = true;
                return true;
            }
            catch (Exception error) { Fatal(error); return false; }
        }

        private static Dictionary<string, object> VerifySavePaths(SaveSystem saves)
        {
            var paths = new Dictionary<string, object>();
            foreach (string name in new[] { "GetUserFolder", "GetSaveDataFile", "GetLegendDataFile", "GetBackupDataFile" })
            {
                object[] args = name == "GetUserFolder" ? null
                    : name == "GetLegendDataFile" ? new object[] { 987 } : new object[] { "video_worker_probe" };
                string path = (string)AccessTools.Method(typeof(SaveSystem), name).Invoke(saves, args);
                string full = FullDirectory(path);
                if (!string.Equals(full, _profile, StringComparison.OrdinalIgnoreCase)
                    && !full.StartsWith(_profile + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidOperationException(name + " 未进入视频独立存档目录");
                paths.Add(name, path);
            }
            return paths;
        }

        internal static bool AcceptPreview(PreviewRequest request)
        {
            if (!Active) return true;
            if (!_ready) return false;
            if (!Accepts(_id, request)) RejectPreview("视频后台拒绝其他任务或普通试玩请求");
            return true;
        }

        internal static void RejectPreview(string reason)
        {
            if (Active) Fatal(new InvalidOperationException(reason));
        }

        internal static void Complete(string id, string state)
        {
            if (!Active) return;
            if (id != _id) Fatal(new InvalidOperationException("视频后台完成了不匹配的任务"));
            _exitCode = state == "complete" || state == "cancelled" ? 0 : 1;
        }

        private static void Fatal(Exception error)
        {
            try
            {
                Console.Error.WriteLine("视频后台初始化或隔离失败：" + error);
                if (_directory != null && !VideoRenderRuntime.Busy)
                    VideoRenderRuntime.Status(_directory, "error", 0, 0, "视频后台失败：" + error.Message);
            }
            finally { Environment.Exit(1); }
        }

        private static string FileInProfile(string relative)
        {
            try { return ProfileFile(_profile, relative); }
            catch (Exception error) { Fatal(error); return null; }
        }

        private static bool UserFolder(ref string __result)
        {
            if (!Active) return true;
            __result = _profile; return false;
        }
        private static bool SaveFile(string name, ref string __result)
        {
            if (!Active) return true;
            __result = FileInProfile("Save_" + name + ".dat"); return false;
        }
        private static bool LegendFile(int slot, ref string __result)
        {
            if (!Active) return true;
            __result = FileInProfile("Legend_" + slot.ToString().PadLeft(3, '0') + ".dat"); return false;
        }
        private static bool BackupFile(string name, ref string __result)
        {
            if (!Active) return true;
            __result = FileInProfile(Path.Combine("Backup", "Save_" + name + ".dat")); return false;
        }
        private static bool DesktopLayout(ref bool __result)
        {
            if (!Active) return true;
            __result = false; return false;
        }
        private static bool HasKeyboard() { return !Active || Keyboard.current != null; }
#endif
    }
}
