using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Text;
using Unity.Collections;
using Fungus;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.UI;

namespace MortalModHost
{
    // Capture the game's composited framebuffer, including overlay Canvas UI.
    // Time.captureFramerate drives game animation and Wwise's offline frame time;
    // custom package audio uses that same clock through VideoAudioMixer.
    internal static class VideoRenderRuntime
    {
        private static MonoBehaviour _owner;
        private static Session _session;
        internal static bool Busy { get { return _session != null; } }
        internal static bool KeepAudioAwake { get { return _session != null && !_session.Finishing; } }
        internal static void Init(MonoBehaviour owner) { _owner = owner; }

        internal static bool BlockPreview(PreviewRequest preview, string requestPath)
        {
            if (_session == null) return false;
            if (preview.VideoRenderId == _session.Id) return _session.Running;
            if (preview.VideoRenderId != null)
            {
                Status(VideoRenderRequest.DirectoryFor(preview.VideoRenderId), "error", 0, 0,
                    "游戏正在生成另一个视频，请等待完成");
                File.Delete(requestPath);
            }
            // In particular, do not ReloadMods for an ordinary F5 while recording.
            return true;
        }

        // 0 = resolution not applied yet, 1 = navigate, -1 = invalid/cancelled.
        internal static int Prepare(PreviewRequest preview)
        {
            if (preview.VideoRenderId == null) return 1;
            string directory = VideoRenderRequest.DirectoryFor(preview.VideoRenderId);
            try
            {
                if (_session == null)
                    _session = new Session(preview.VideoRenderId, directory, VideoRenderRequest.Read(directory, preview));
                if (File.Exists(Path.Combine(directory, "cancel"))) { _session.Finish("cancelled", "视频生成已取消"); return -1; }
                return _session.Ready() ? 1 : 0;
            }
            catch (Exception error)
            {
                if (_session != null) _session.Finish("error", error.Message);
                else
                {
                    try { Status(directory, "error", 0, 0, error.Message); }
                    finally { VideoWorkerRuntime.Complete(preview.VideoRenderId, "error"); }
                }
                return -1;
            }
        }

        internal static IEnumerator WhenReady(IEnumerator playback, ModPackage package)
        {
            if (_session == null || !RuntimeTrace.IsDevelopmentPackage(package)) return playback;
            _session.Staged = true;
            return WaitAndPlay(playback, _session);
        }

        private static IEnumerator WaitAndPlay(IEnumerator playback, Session session)
        {
            // LuaManager.Init begins UnloadLoading before ExecuteLuaScript. Wait
            // for that native coroutine before recording or advancing our Lua.
            while (!Plugin.IsSceneReadyForModNavigation(SceneController.Instance))
            {
                if (_session != session || session.Failure != null) yield break;
                yield return null;
            }
            if (_session != session) yield break;
            try { if (!session.Running) session.Begin(); }
            catch (Exception error) { session.Failure = error.Message; }
            if (session.Failure != null) yield break;
            StoryManager manager = StoryManager.Instance;
            if (manager != null) manager.AutoDialog(true);
            while (playback.MoveNext()) yield return playback.Current;
        }

        internal static void Tick()
        {
            var current = _session;
            if (current == null) return;
            try { current.Tick(); }
            catch (Exception error) { current.Failure = error.Message; }
        }

        internal static bool OnNode(string kind)
        {
            var current = _session;
            if (current == null || !current.Running) return false;
            string location = RuntimeTrace.CurrentStory + "/" + RuntimeTrace.CurrentNode;
            current.NodeChanged();
            if (current.Request.Terminals.Contains(location))
            {
                if (kind == "death" || current.Request.Cards.Contains(location)) current.TerminalCard = true;
                else { current.End = true; return true; }
            }
            // These nodes require gameplay rather than a story route. Never pick
            // an invented combat result or silently omit a native panel.
            if (kind == "combat" || kind == "battle" || kind == "shop" || kind == "panel"
                || kind == "dice" || kind == "intro")
            {
                current.Failure = "此步骤需要手动游戏操作，暂不能自动生成视频：" + location + "（" + kind + "）";
                return true;
            }
            return false;
        }

        internal static void Fail(string reason)
        {
            if (_session != null) _session.Failure = reason;
        }

        internal static void Shutdown()
        {
            if (_session != null) _session.Finish("cancelled", "游戏已关闭，视频生成已取消", false);
        }

        internal static void EndingPanel(EndGamePanel panel)
        {
            if (_session != null && _session.Running) _session.CardPanel = panel;
        }

        internal static void Status(string directory, string state, int frames, double seconds, string message)
        {
            string path = Path.Combine(directory, "status.json"), temporary = path + ".tmp";
            File.WriteAllText(temporary, ScenarioState.ToJson(new Dictionary<string, object>
            {
                {"state",state}, {"frames",frames}, {"seconds",seconds}, {"message",message}
            }), new UTF8Encoding(false));
            if (File.Exists(path)) File.Replace(temporary, path, null);
            else File.Move(temporary, path);
        }

        private sealed class Session
        {
            internal readonly string Id, Directory;
            internal readonly VideoRenderRequest Request;
            internal bool Running, Staged, End, TerminalCard;
            internal string Failure;
            internal EndGamePanel CardPanel;
            private readonly int _width, _height, _refresh, _captureFps, _vSync, _targetFps;
            private readonly FullScreenMode _mode;
            private readonly bool _background;
            private readonly float _preparedAt;
            private bool _audio, _finishing, _oldAuto;
            internal bool Finishing { get { return _finishing; } }
            private VideoNativeAudio _nativeAudio;
            private int _frames;
            private float _choiceElapsed, _cardElapsed;
            private float _cardHold = -1;
            private GameOverController _deadPanel;
            private readonly WaitForEndOfFrame _endOfFrame = new WaitForEndOfFrame();
            private string _choiceLocation;
            private Process _encoder;
            private readonly StringBuilder _encoderError = new StringBuilder();
            private RenderTexture _target;
            private Texture2D _pixels;
            private VideoFrameWriter _frameWriter;
            private sealed class ReadbackFrame
            {
                internal NativeArray<byte> Pixels;
                internal AsyncGPUReadbackRequest Request;
            }
            private readonly Queue<ReadbackFrame> _readbacks = new Queue<ReadbackFrame>();
            private readonly Stack<ReadbackFrame> _freeReadbacks = new Stack<ReadbackFrame>();
            private readonly List<ReadbackFrame> _readbackBuffers = new List<ReadbackFrame>();
            private bool _asyncReadback;
            private const int BufferedFrames = 3;
            private static readonly FieldInfo Auto = AccessTools.Field(typeof(StoryManager), "_autoDialog");
            private static readonly FieldInfo EndCanvas = AccessTools.Field(typeof(EndGamePanel), "_canvasGroup");
            private static readonly FieldInfo EndDesc = AccessTools.Field(typeof(EndGamePanel), "_descText");
            private static readonly FieldInfo DeadCanvas = AccessTools.Field(typeof(GameOverController), "_titleButtonCanvas");

            internal Session(string id, string directory, VideoRenderRequest request)
            {
                Id = id; Directory = directory; Request = request;
                _width = Screen.width; _height = Screen.height; _mode = Screen.fullScreenMode;
                _refresh = Screen.currentResolution.refreshRate; _captureFps = Time.captureFramerate;
                _vSync = QualitySettings.vSyncCount; _targetFps = Application.targetFrameRate;
                _background = Application.runInBackground; _preparedAt = Time.realtimeSinceStartup;
                Status(Directory, "preparing", 0, 0, "正在准备游戏原生画面");
                Application.runInBackground = true;
                Screen.SetResolution(request.Width, request.Height, FullScreenMode.Windowed);
            }

            internal bool Ready() { return Screen.width == Request.Width && Screen.height == Request.Height; }
            private string FileAt(string name) { return Path.Combine(Directory, name); }

            internal void Begin()
            {
                if (!Ready()) throw new InvalidOperationException("游戏窗口分辨率未准备好");
                _target = new RenderTexture(Request.Width, Request.Height, 0, RenderTextureFormat.ARGB32);
                if (!_target.Create()) throw new InvalidOperationException("游戏画面缓冲区创建失败");
                _asyncReadback = SystemInfo.supportsAsyncGPUReadback && SystemInfo.IsFormatSupported(
                    _target.graphicsFormat, UnityEngine.Experimental.Rendering.FormatUsage.ReadPixels);
                if (_asyncReadback)
                    for (int i = 0; i < BufferedFrames; i++)
                    {
                        var frame = new ReadbackFrame { Pixels = new NativeArray<byte>(
                            checked(Request.Width * Request.Height * 4), Allocator.Persistent, NativeArrayOptions.UninitializedMemory) };
                        _readbackBuffers.Add(frame);
                        _freeReadbacks.Push(frame);
                    }
                if (!_asyncReadback)
                    _pixels = new Texture2D(Request.Width, Request.Height, TextureFormat.RGBA32, false);
                _encoder = Encoder("-y -f rawvideo -pixel_format rgba -video_size " + Request.Width + "x" + Request.Height
                    + " -framerate " + Request.Fps + " -i pipe:0 -an -c:v libx264 -threads:v "
                    + Math.Max(1, Math.Min(4, Environment.ProcessorCount / 2)) + " -preset veryfast -crf 18"
                    + " -pix_fmt yuv420p " + Quote(FileAt("picture.mp4")), true);
                _frameWriter = new VideoFrameWriter(_encoder.StandardInput.BaseStream,
                    checked(Request.Width * Request.Height * 4), BufferedFrames);
                QualitySettings.vSyncCount = 0; Application.targetFrameRate = -1;
                Time.captureFramerate = Request.Fps;
                AutoDialogueRuntime.RenderSpeed = Request.Speed == "game" ? (AutoDialogueSpeed?)null
                    : Request.Speed == "slow" ? AutoDialogueSpeed.Slow : Request.Speed == "fast" ? AutoDialogueSpeed.Fast : AutoDialogueSpeed.Standard;
                if (StoryManager.Instance != null) _oldAuto = (bool)Auto.GetValue(StoryManager.Instance);
                Check(AkSoundEngine.WakeupFromSuspend(), "唤醒原生音频");
                Check(AkSoundEngine.SetOfflineRendering(true), "启用原生音频离线时钟");
                _audio = true;
                Check(AkSoundEngine.SetOfflineRenderingFrameTime(0), "准备原生音频时钟");
                AkSoundEngine.RenderAudio();
                _nativeAudio = new VideoNativeAudio(FileAt("wwise.wav"));
                Check(AkSoundEngine.SetOfflineRenderingFrameTime(1f / Request.Fps), "设置原生音频帧率");
                VideoAudioMixer.Begin(FileAt("custom.wav"), Request.Fps);
                Running = true;
                Status(Directory, "running", 0, 0, "正在生成游戏原生视频");
                _owner.StartCoroutine(Capture());
            }

            internal void NodeChanged() { _choiceLocation = null; _choiceElapsed = 0; }

            internal void Tick()
            {
                if (_finishing) return;
                // Update keeps running when a minimized window stops producing
                // end-of-frame callbacks, so cancellation must also live here.
                if (File.Exists(FileAt("cancel"))) { Finish("cancelled", "视频生成已取消"); return; }
                if (Failure != null) { Finish("error", Failure); return; }
                if (!Running)
                {
                    if (Time.realtimeSinceStartup - _preparedAt > 60)
                        Finish("error", "游戏演出启动超时");
                    return;
                }
                if (TerminalCard)
                {
                    bool ready = false;
                    string text = "";
                    if (CardPanel != null)
                    {
                        CanvasGroup canvas = (CanvasGroup)EndCanvas.GetValue(CardPanel);
                        Text desc = (Text)EndDesc.GetValue(CardPanel);
                        ready = canvas.alpha >= .999f && desc.color.a >= .999f;
                        if (_cardHold < 0 && ready) text = desc.text;
                    }
                    else if (SceneController.Instance != null && SceneController.Instance.CurrentScene == "GameOver")
                    {
                        if (_deadPanel == null) _deadPanel = UnityEngine.Object.FindObjectOfType<GameOverController>();
                        ready = _deadPanel != null && ((CanvasGroup)DeadCanvas.GetValue(_deadPanel)).alpha >= .999f;
                        if (_cardHold < 0 && ready)
                            text = (ModOverlay.EndingTitle ?? ModOverlay.DeathTitle) + (ModOverlay.EndingDesc ?? ModOverlay.DeathDesc);
                    }
                    if (ready)
                    {
                        _cardElapsed += Time.deltaTime;
                        if (_cardHold < 0)
                            _cardHold = new AutoDialogueTiming(text, false).Duration(AutoDialogueRuntime.RenderSpeed
                                ?? (AutoDialogueRuntime.Speed != null ? AutoDialogueRuntime.Speed.Value : AutoDialogueSpeed.Standard));
                        if (_cardElapsed >= _cardHold) End = true;
                    }
                    return;
                }
                MenuDialog menu = MenuDialog.ActiveMenuDialog;
                if (menu == null || !menu.IsActive() || menu.DisplayedOptionsCount == 0) return;
                string location = RuntimeTrace.CurrentStory + "/" + RuntimeTrace.CurrentNode;
                if (_choiceLocation != location) { _choiceLocation = location; _choiceElapsed = 0; }
                _choiceElapsed += Time.deltaTime;
                if (_choiceElapsed < Request.ChoiceHold) return;
                int selected;
                Button button = null;
                if (Request.Choices.TryGetValue(location, out selected))
                {
                    if (selected < menu.CachedButtons.Length) button = menu.CachedButtons[selected];
                    if (!Available(button)) throw new InvalidOperationException("预设选项在当前路线不可选：" + location + "，选项 " + (selected + 1));
                }
                else
                    foreach (Button candidate in menu.CachedButtons) if (Available(candidate)) { button = candidate; break; }
                if (button == null) throw new InvalidOperationException("剧情没有可用选项：" + location);
                button.onClick.Invoke();
                _choiceLocation = null;
            }

            private static bool Available(Button button)
            { return button != null && button.gameObject.activeInHierarchy && button.interactable; }

            private IEnumerator Capture()
            {
                while (!_finishing)
                {
                    yield return _endOfFrame;
                    if (_finishing) yield break;
                    if (File.Exists(FileAt("cancel"))) { Finish("cancelled", "视频生成已取消"); yield break; }
                    try
                    {
                        if (Failure != null) throw new InvalidOperationException(Failure);
                        if (!Ready()) throw new InvalidOperationException("生成期间游戏窗口尺寸发生变化，请保持游戏窗口尺寸");
                        _frameWriter.Check();
                        DrainReadbacks(false);
                        if (_readbacks.Count >= BufferedFrames) DrainReadback(true);
                        ScreenCapture.CaptureScreenshotIntoRenderTexture(_target);
                        if (_asyncReadback)
                        {
                            ReadbackFrame frame = _freeReadbacks.Pop();
                            frame.Request = AsyncGPUReadback.RequestIntoNativeArray(ref frame.Pixels,
                                _target, 0, TextureFormat.RGBA32);
                            _readbacks.Enqueue(frame);
                        }
                        else
                        {
                            RenderTexture previous = RenderTexture.active;
                            try
                            {
                                RenderTexture.active = _target;
                                _pixels.ReadPixels(new Rect(0, 0, Request.Width, Request.Height), 0, 0, false);
                            }
                            finally { RenderTexture.active = previous; }
                            byte[] pixels = _frameWriter.Rent();
                            _pixels.GetRawTextureData<byte>().CopyTo(pixels);
                            _frameWriter.Submit(pixels);
                        }
                        _nativeAudio.Frame();
                        VideoAudioMixer.Frame();
                        _frames++;
                        if (_frames % Request.Fps == 0)
                            Status(Directory, "running", _frames, (double)_frames / Request.Fps,
                                "正在生成：" + RuntimeTrace.CurrentStory + "/" + RuntimeTrace.CurrentNode);
                        if ((double)_frames / Request.Fps >= Request.MaxSeconds)
                            throw new InvalidOperationException("视频超过最长生成时间，停止于 " + RuntimeTrace.CurrentStory + "/" + RuntimeTrace.CurrentNode);
                    }
                    catch (Exception error) { Finish("error", error.Message); yield break; }
                    if (End) { Finish("complete", "视频生成完成"); yield break; }
                }
            }

            // Readbacks are consumed in submission order. Both GPU requests and
            // managed frames are bounded; a slow encoder applies backpressure.
            private void DrainReadbacks(bool all)
            {
                while (_readbacks.Count > 0)
                {
                    if (!all && !_readbacks.Peek().Request.done) break;
                    DrainReadback(all);
                }
            }

            private void DrainReadback(bool wait)
            {
                ReadbackFrame frame = _readbacks.Dequeue();
                AsyncGPUReadbackRequest request = frame.Request;
                if (wait && !request.done) request.WaitForCompletion();
                if (request.hasError) throw new InvalidOperationException("游戏画面异步读取失败，未导出不完整视频");
                byte[] pixels = _frameWriter.Rent();
                frame.Pixels.CopyTo(pixels);
                _frameWriter.Submit(pixels);
                _freeReadbacks.Push(frame);
            }

            internal void Finish(string state, string message, bool returnToTitle = true)
            {
                if (_finishing) return;
                _finishing = true;
                bool reported = false;
                try
                {
                    try
                    {
                        if (Staged && SceneController.Instance != null && SceneController.Instance.CurrentScene == "Story")
                            LuaManagerPatch.StopActiveDevelopmentPlayback(true);
                        StopAudio(state == "complete");
                        if (_encoder != null)
                        {
                            if (state == "complete")
                            {
                                DrainReadbacks(true);
                                _frameWriter.Complete();
                            }
                            else if (!_encoder.HasExited) _encoder.Kill();
                            Wait(_encoder);
                            if (state == "complete" && _encoder.ExitCode != 0)
                                throw new InvalidOperationException("画面编码失败：" + _encoderError);
                            _encoder.Dispose(); _encoder = null;
                        }
                        if (state == "complete")
                        {
                            Status(Directory, "encoding", _frames, (double)_frames / Request.Fps, "正在合成原生音轨");
                            if (!File.Exists(FileAt("wwise.wav")) || new FileInfo(FileAt("wwise.wav")).Length <= 44)
                                throw new InvalidOperationException("原生音频引擎没有输出音轨，视频未导出");
                            _encoderError.Clear();
                            using (Process mux = Encoder("-y -i " + Quote(FileAt("picture.mp4")) + " -i " + Quote(FileAt("wwise.wav"))
                                + " -i " + Quote(FileAt("custom.wav"))
                                + " -filter_complex \"[1:a][2:a]amix=inputs=2:duration=longest:normalize=0[a]\" -map 0:v:0 -map \"[a]\""
                                + " -c:v copy -c:a aac -ac 2 -b:a 192k -t " + ((double)_frames / Request.Fps).ToString("R", System.Globalization.CultureInfo.InvariantCulture)
                                + " -movflags +faststart " + Quote(FileAt("video.mp4")), false))
                            {
                                Wait(mux);
                                if (mux.ExitCode != 0) throw new InvalidOperationException("原生音轨合成失败：" + _encoderError);
                            }
                            if (!File.Exists(FileAt("video.mp4")) || new FileInfo(FileAt("video.mp4")).Length == 0)
                                throw new InvalidOperationException("视频编码器没有生成成片");
                        }
                        if (File.Exists(FileAt("cancel"))) { state = "cancelled"; message = "视频生成已取消"; }
                    }
                    catch (OperationCanceledException) { state = "cancelled"; message = "视频生成已取消"; }
                    catch (Exception error) { state = "error"; message = error.Message; }
                    finally
                    {
                        try { Restore(); }
                        catch (Exception error) { state = "error"; message += "；恢复游戏设置失败：" + error.Message; }
                        _session = null;
                        // The recording stopped the Story coroutine. Leaving that
                        // stage visible makes Continue return to a dead script.
                        // Exit only the staged recording session, after restoring
                        // its clocks/audio; application shutdown must not navigate.
                        if (!VideoWorkerRuntime.Active && returnToTitle && Staged && SceneController.Instance != null
                            && SceneController.Instance.CurrentScene != "Title")
                        {
                            try
                            {
                                // A system menu may be open at cancellation. Native
                                // scene loading yields before resetting timeScale.
                                Time.timeScale = 1f;
                                SceneController.Instance.LoadTitle();
                            }
                            catch (Exception error)
                            {
                                state = "error";
                                message += "；结束视频会话后返回标题失败：" + error.Message;
                            }
                        }
                        Status(Directory, state, _frames, (double)_frames / Request.Fps, message);
                        reported = true;
                    }
                    // Keep only diagnostics and a complete movie, never large WAVs.
                    foreach (string name in new[] { "picture.mp4", "wwise.wav", "custom.wav" })
                        if (File.Exists(FileAt(name))) File.Delete(FileAt(name));
                    if (state != "complete" && File.Exists(FileAt("video.mp4"))) File.Delete(FileAt("video.mp4"));
                }
                finally { VideoWorkerRuntime.Complete(Id, reported ? state : "error"); }
            }

            private void StopAudio(bool requireSamples)
            {
                try { VideoAudioMixer.End(); }
                finally
                {
                    try
                    {
                        if (_nativeAudio != null) { var native = _nativeAudio; _nativeAudio = null; native.End(requireSamples); }
                    }
                    finally
                    {
                        if (_audio)
                        {
                            _audio = false;
                            AkSoundEngine.SetOfflineRenderingFrameTime(0); AkSoundEngine.RenderAudio();
                            Check(AkSoundEngine.SetOfflineRendering(false), "恢复原生音频时钟");
                            AkSoundEngine.RenderAudio();
                        }
                    }
                }
            }

            private void Restore()
            {
                try { StopAudio(false); }
                finally
                {
                    try
                    {
                        if (_encoder != null)
                        {
                            try { if (!_encoder.HasExited) _encoder.Kill(); }
                            finally { _encoder.Dispose(); _encoder = null; }
                        }
                    }
                    finally
                    {
                        try { ReleaseFrames(); }
                        finally
                        {
                            if (!VideoWorkerRuntime.Active)
                            {
                                Time.captureFramerate = _captureFps; QualitySettings.vSyncCount = _vSync;
                                Application.targetFrameRate = _targetFps; Application.runInBackground = _background;
                                Screen.SetResolution(_width, _height, _mode, _refresh);
                                if (AkSoundEngineController.Instance != null)
                                    AkSoundEngineController.Instance.OnApplicationFocus(Application.isFocused);
                            }
                            AutoDialogueRuntime.RenderSpeed = null;
                            if (StoryManager.Instance != null && Running) StoryManager.Instance.AutoDialog(_oldAuto);
                            if (_target != null) { _target.Release(); UnityEngine.Object.Destroy(_target); }
                            if (_pixels != null) UnityEngine.Object.Destroy(_pixels);
                        }
                    }
                }
            }

            private void ReleaseFrames()
            {
                try
                {
                    if (_frameWriter != null) { var writer = _frameWriter; _frameWriter = null; writer.Dispose(); }
                }
                finally
                {
                    while (_readbacks.Count > 0)
                    {
                        AsyncGPUReadbackRequest request = _readbacks.Dequeue().Request;
                        if (!request.done) request.WaitForCompletion();
                    }
                    foreach (ReadbackFrame frame in _readbackBuffers)
                        if (frame.Pixels.IsCreated) frame.Pixels.Dispose();
                    _readbackBuffers.Clear();
                    _freeReadbacks.Clear();
                }
            }

            private Process Encoder(string arguments, bool pipe)
            {
                var process = new Process { StartInfo = new ProcessStartInfo(Request.Encoder,
                    "-hide_banner -loglevel error -filter_threads 1 -filter_complex_threads 1 " + arguments)
                { UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = pipe, RedirectStandardError = true } };
                process.ErrorDataReceived += (sender, args) => { if (args.Data != null && _encoderError.Length < 4000) _encoderError.AppendLine(args.Data); };
                if (!process.Start()) throw new InvalidOperationException("视频编码器启动失败");
                process.BeginErrorReadLine();
                return process;
            }

            private void Wait(Process process)
            {
                while (!process.WaitForExit(100))
                    if (File.Exists(FileAt("cancel"))) { process.Kill(); throw new OperationCanceledException(); }
                process.WaitForExit();
            }

            private static void Check(AKRESULT result, string action)
            { if (result != AKRESULT.AK_Success) throw new InvalidOperationException(action + "失败：" + result); }
            private static string Quote(string path) { return "\"" + path.Replace("\"", "\\\"") + "\""; }
        }
    }

    // Wwise's original focus handler suspends audio when the editor owns focus.
    // Offline audio must instead advance once per captured game frame.
    [HarmonyPatch(typeof(AkSoundEngineController), "OnApplicationFocus")]
    internal static class VideoAudioFocusPatch
    {
        private static bool Prefix() { return !VideoRenderRuntime.KeepAudioAwake; }
    }

    [HarmonyPatch(typeof(EndGamePanel), "Open")]
    internal static class VideoEndingPanelPatch
    {
        private static void Postfix(EndGamePanel __instance) { VideoRenderRuntime.EndingPanel(__instance); }
    }
}
