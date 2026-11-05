using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace MortalModHost
{
    // Development-only export of frames rendered by the installed game itself.
    internal static class EffectPreviewCapture
    {
        private const int Width = 640, Height = 360, Fps = 15, CaptureLayer = 31;
        private const float Step = 1f / Fps;
        private static readonly FieldInfo NativeInstances = AccessTools.Field(typeof(StoryEffectsController), "_nonUniqueInstances");
        private static readonly FieldInfo RendererIndex = AccessTools.Field(typeof(UniversalAdditionalCameraData), "m_RendererIndex");
        private static MonoBehaviour _host;
        private static Coroutine _runner;
        private static bool _busy, _cancel, _pending, _frameDone;
        private static Exception _renderError;
        private static StoryEffectsController _controller;
        private static GameObject _root, _cameraObject;
        private static ParticleSystem _system;
        private static ParticleSystem[] _particles;
        private static float[] _emissionEnds;
        private static Camera _camera;
        private static RenderTexture _target;
        private static Texture2D _black, _white, _png;
        private static string _directory, _frameFile, _effectName;
        private static int _completed, _total;
        private static double _frameScore;
        private static Dictionary<string, object> _map, _effects;

        internal static bool Busy { get { return _busy; } }
        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static void Start(StoryEffectsController controller, string requestId, string namesJson)
        {
            if (!RuntimeTrace.IsDevelopmentPackage(ModOverlay.CurrentPackage))
                throw new InvalidOperationException("原版特效帧捕获只允许固定 F5 开发试玩包");
            if (!Regex.IsMatch(requestId ?? "", "^[0-9a-fA-F]{32}$"))
                throw new ArgumentException("特效捕获请求 ID 必须是 32 位十六进制字符");
            string directory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "lom_modkit", "effect-previews", requestId);
            Directory.CreateDirectory(directory);
            if (_busy)
            {
                WriteStatus(directory, "error", 0, 0, "已有原版特效捕获正在运行");
                return;
            }
            try
            {
                var raw = MiniJson.Parse(namesJson) as List<object>;
                if (raw == null || raw.Count == 0) throw new ArgumentException("特效名称必须是非空 JSON 数组");
                var names = new List<string>();
                foreach (object item in raw)
                {
                    string name = item as string;
                    if (string.IsNullOrEmpty(name)) throw new ArgumentException("特效数组只能包含非空名称");
                    if (!names.Contains(name)) names.Add(name);
                }
                if (File.Exists(Path.Combine(directory, "cancel")))
                {
                    WriteStatus(directory, "cancelled", 0, names.Count, "已取消原版特效捕获");
                    return;
                }
                if (_host == null || controller == null) throw new InvalidOperationException("原版特效捕获宿主或 effects 绑定尚未就绪");
                _controller = controller;
                _directory = directory;
                _completed = 0;
                _total = names.Count;
                _cancel = false;
                _busy = true;
                WriteStatus(directory, "running", 0, _total, "正在准备原版 Unity 特效捕获");
                Coroutine runner = _host.StartCoroutine(Run(names));
                _runner = _busy ? runner : null;
            }
            catch (Exception error)
            {
                WriteStatus(directory, "error", 0, 0, error.Message);
                if (_busy) Finish("error", error.Message);
            }
        }

        internal static void Cancel()
        {
            if (!_busy) return;
            _cancel = true;
            if (_runner != null && _host != null) _host.StopCoroutine(_runner);
            Finish("cancelled", "已取消原版特效捕获");
        }

        private static IEnumerator Run(List<string> names)
        {
            IEnumerator routine = Capture(names);
            Exception failure = null;
            try
            {
                while (true)
                {
                    object next;
                    try
                    {
                        if (!routine.MoveNext()) break;
                        next = routine.Current;
                    }
                    catch (Exception error) { failure = error; break; }
                    yield return next;
                }
            }
            finally
            {
                var disposable = routine as IDisposable;
                if (disposable != null) disposable.Dispose();
                Finish(failure is OperationCanceledException || _cancel ? "cancelled" : failure != null ? "error" : "complete",
                    failure != null ? (string.IsNullOrEmpty(_effectName) ? "" : _effectName + ": ") + failure.Message
                        : _cancel ? "已取消原版特效捕获" : "原版特效帧捕获完成");
            }
        }

        private static IEnumerator Capture(List<string> names)
        {
            CreateCamera();
            _effects = new Dictionary<string, object>(StringComparer.Ordinal);
            _map = new Dictionary<string, object>
            {
                {"format", 1}, {"source", "game-render"}, {"width", Width}, {"height", Height},
                {"world_width", Math.Round(_camera.orthographicSize * 2.0 * Width / Height, 4)},
                {"world_height", Math.Round(_camera.orthographicSize * 2.0, 4)},
                {"alpha_mode", "premultiplied"}, {"color_space", "gamma"}, {"effects", _effects}
            };
            RenderPipelineManager.endFrameRendering += RenderFrame;
            for (int index = 0; index < names.Count; index++)
            {
                CheckCancellation();
                _effectName = names[index];
                WriteStatus(_directory, "running", _completed, _total, "正在捕获：" + _effectName);
                CreateEffect(_effectName);
                bool looping = false;
                float warmup = 0f;
                foreach (ParticleSystem particle in _particles)
                {
                    if (!particle.gameObject.activeInHierarchy) continue;
                    ParticleSystem.MainModule main = particle.main;
                    if (main.loop) { looping = true; warmup = Mathf.Max(warmup, main.duration); }
                }
                int warmupFrames = looping ? Mathf.CeilToInt(Mathf.Clamp(warmup, 0.5f, 2f) * Fps) : 0;
                int limit = looping ? warmupFrames + 2 * Fps : 8 * Fps;
                var frames = new List<object>();
                bool complete = looping;
                double previewTime = 0.0, bestScore = -1.0;
                for (int frame = 0; frame < limit; frame++)
                {
                    CheckCancellation();
                    if (frame > 0) Advance(Step);
                    string relative = "frames/" + index.ToString("D3") + "/play_" + frame.ToString("D4") + ".png";
                    RequestFrame(relative);
                    while (!_frameDone) { CheckCancellation(); yield return null; }
                    if (_renderError != null) throw _renderError;
                    frames.Add(new Dictionary<string, object> { { "file", relative }, { "time", (double)frame / Fps } });
                    if (_frameScore > bestScore)
                    {
                        bestScore = _frameScore;
                        previewTime = (double)frame / Fps;
                    }
                    if (!looping && frame > 0 && PlaybackFinished()) { complete = true; break; }
                }
                double duration = (double)frames.Count / Fps;
                float sampleTime = looping ? (float)warmupFrames / Fps + 1f : Mathf.Max(0f, (float)duration / 2f);
                ResetSimulation(sampleTime);
                if (_system != null)
                {
                    _system.Stop(true, ParticleSystemStopBehavior.StopEmitting);
                    _system.Pause(true);
                }
                var tail = new List<object>();
                bool tailComplete = false;
                for (int frame = 0; frame < 6 * Fps; frame++)
                {
                    CheckCancellation();
                    if (frame > 0) Advance(Step);
                    string relative = "frames/" + index.ToString("D3") + "/tail_" + frame.ToString("D4") + ".png";
                    RequestFrame(relative);
                    while (!_frameDone) { CheckCancellation(); yield return null; }
                    if (_renderError != null) throw _renderError;
                    tail.Add(new Dictionary<string, object> { { "file", relative }, { "time", (double)frame / Fps } });
                    if (!HasParticles()) { tailComplete = true; break; }
                }
                _effects[_effectName] = new Dictionary<string, object>
                {
                    {"frames", frames}, {"duration", duration}, {"looping", looping},
                    {"preview_time", previewTime},
                    {"loop_start", (double)warmupFrames / Fps}, {"complete", complete},
                    {"tail_frames", tail}, {"tail_duration", (double)tail.Count / Fps},
                    {"tail_sample_time", sampleTime}, {"tail_complete", tailComplete}
                };
                DestroyEffect();
                AtomicJson(Path.Combine(_directory, "effect_map.json"), _map);
                _completed++;
                WriteStatus(_directory, "running", _completed, _total, "已捕获：" + _effectName);
                yield return null;
            }
        }

        private static void CheckCancellation()
        {
            if (_cancel || File.Exists(Path.Combine(_directory, "cancel"))
                || !RuntimeTrace.IsDevelopmentPackage(ModOverlay.CurrentPackage))
                throw new OperationCanceledException("已取消原版特效捕获");
        }

        private static void CreateCamera()
        {
            if (!(RenderPipelineManager.currentPipeline is UniversalRenderPipeline))
                throw new InvalidOperationException("当前渲染管线不是已核对的 UniversalRenderPipeline");
            if (QualitySettings.activeColorSpace != ColorSpace.Gamma)
                throw new InvalidOperationException("当前原版帧捕获只核对了本游戏的 Gamma 色彩空间");
            Camera source = null;
            foreach (Camera candidate in Resources.FindObjectsOfTypeAll<Camera>())
            {
                if (!candidate.isActiveAndEnabled || candidate.gameObject.name != "StageCamera"
                    || candidate.gameObject.scene != _controller.gameObject.scene) continue;
                if (source != null) throw new InvalidOperationException("当前场景存在多个 StageCamera");
                source = candidate;
            }
            if (source == null || !source.orthographic) throw new InvalidOperationException("找不到已核对的原版正交 StageCamera");
            UniversalAdditionalCameraData sourceData = source.GetComponent<UniversalAdditionalCameraData>();
            if (sourceData == null || RendererIndex == null)
                throw new InvalidOperationException("原版 StageCamera 缺少已核对的 URP renderer 配置");
            if (!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBFloat))
                throw new InvalidOperationException("当前显卡不支持原版帧透明度恢复所需的 ARGBFloat RenderTexture");
            _cameraObject = new GameObject("lom_effect_capture_camera");
            _camera = _cameraObject.AddComponent<Camera>();
            _camera.CopyFrom(source);
            _camera.transform.SetPositionAndRotation(source.transform.position, source.transform.rotation);
            _camera.enabled = false;
            _camera.aspect = (float)Width / Height;
            _camera.cullingMask = 1 << CaptureLayer;
            _camera.clearFlags = CameraClearFlags.SolidColor;
            _camera.allowHDR = true;
            _camera.allowMSAA = false;
            var data = _cameraObject.AddComponent<UniversalAdditionalCameraData>();
            data.renderType = CameraRenderType.Base;
            data.SetRenderer((int)RendererIndex.GetValue(sourceData));
            data.renderPostProcessing = false;
            data.antialiasing = AntialiasingMode.None;
            _target = new RenderTexture(Width, Height, 24, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            if (!_target.Create()) throw new InvalidOperationException("无法创建原版帧浮点 RenderTexture");
            _camera.targetTexture = _target;
            _black = new Texture2D(Width, Height, TextureFormat.RGBAFloat, false, true);
            _white = new Texture2D(Width, Height, TextureFormat.RGBAFloat, false, true);
            _png = new Texture2D(Width, Height, TextureFormat.RGBA32, false, true);
        }

        private static void CreateEffect(string name)
        {
            var list = NativeInstances != null ? NativeInstances.GetValue(_controller) as List<GameObject> : null;
            if (list == null) throw new InvalidOperationException("原版缺少 _nonUniqueInstances，无法捕获自己创建的特效");
            int before = list.Count;
            _controller.SetupEffect(name, 0f, 0f, 1f, 1f, 0, 1);
            if (list.Count != before + 1 || list[before] == null)
                throw new InvalidOperationException("原版未创建特效：" + name);
            _root = list[before];
            _system = _root.GetComponent<ParticleSystem>();
            if (_system == null) throw new InvalidOperationException("原版特效没有根 ParticleSystem：" + name);
            foreach (Transform transform in _root.GetComponentsInChildren<Transform>(true)) transform.gameObject.layer = CaptureLayer;
            _particles = _root.GetComponentsInChildren<ParticleSystem>(true);
            _emissionEnds = new float[_particles.Length];
            for (int index = 0; index < _particles.Length; index++)
            {
                _particles[index].useAutoRandomSeed = false;
                _particles[index].randomSeed = (uint)(17389 + index * 7919);
                _emissionEnds[index] = EmissionEnd(_particles[index]);
            }
            ResetSimulation(0f);
        }

        private static void ResetSimulation(float time)
        {
            if (_system == null) return;
            _system.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
            _system.Simulate(time, true, true, false);
            _system.Pause(true);
        }

        private static void Advance(float delta)
        {
            if (_system == null) return;
            _system.Simulate(delta, true, false, false);
            _system.Pause(true);
        }

        private static bool HasParticles()
        {
            foreach (ParticleSystem particle in _particles)
                if (particle != null && particle.gameObject.activeInHierarchy && particle.particleCount > 0) return true;
            return false;
        }

        private static bool PlaybackFinished()
        {
            if (HasParticles()) return false;
            for (int index = 0; index < _particles.Length; index++)
            {
                ParticleSystem particle = _particles[index];
                if (particle == null || !particle.gameObject.activeInHierarchy || _emissionEnds[index] < 0f) continue;
                ParticleSystem.MainModule main = particle.main;
                // Simulate pauses after every step. IsAlive also counts an emitter that
                // remains ready to play, so use its real playback time and emission range.
                // time excludes startDelay; zero must not finish a delayed time-zero burst.
                float time = particle.time, end = _emissionEnds[index];
                if (main.loop || time <= 0f || time < end || (end < main.duration && time == end)) return false;
            }
            return true;
        }

        private static float EmissionEnd(ParticleSystem particle)
        {
            ParticleSystem.EmissionModule emission = particle.emission;
            if (!emission.enabled) return -1f;
            float duration = particle.main.duration;
            if (MayEmit(emission.rateOverTime) || MayEmit(emission.rateOverDistance)) return duration;
            float end = -1f;
            for (int index = 0; index < emission.burstCount; index++)
            {
                ParticleSystem.Burst burst = emission.GetBurst(index);
                if (burst.time > duration) continue;
                float last = burst.cycleCount <= 0 ? duration
                    : burst.time + (burst.cycleCount - 1) * burst.repeatInterval;
                end = Mathf.Max(end, Mathf.Min(duration, last));
            }
            return end;
        }

        private static bool MayEmit(ParticleSystem.MinMaxCurve rate)
        {
            if (rate.mode == ParticleSystemCurveMode.Constant) return rate.constant > 0f;
            if (rate.mode == ParticleSystemCurveMode.TwoConstants) return rate.constantMin > 0f || rate.constantMax > 0f;
            // A curve may have later positive intervals; do not truncate in a gap.
            return true;
        }

        private static void RequestFrame(string relative)
        {
            _frameFile = relative;
            _renderError = null;
            _frameDone = false;
            _pending = true;
        }

        private static void RenderFrame(ScriptableRenderContext context, Camera[] cameras)
        {
            if (!_busy || !_pending) return;
            _pending = false;
            RenderTexture previous = RenderTexture.active;
            try
            {
                CheckCancellation();
                _camera.backgroundColor = Color.black;
                UniversalRenderPipeline.RenderSingleCamera(context, _camera);
                RenderTexture.active = _target;
                _black.ReadPixels(new Rect(0, 0, Width, Height), 0, 0, false);
                CheckCancellation();
                _camera.backgroundColor = Color.white;
                UniversalRenderPipeline.RenderSingleCamera(context, _camera);
                RenderTexture.active = _target;
                _white.ReadPixels(new Rect(0, 0, Width, Height), 0, 0, false);
                // These views reference the textures' CPU storage. Keep them local and
                // never dispose them; the textures own the memory. This avoids allocating
                // and copying two float images plus an RGBA image for every frame.
                var black = _black.GetRawTextureData<Color>();
                var white = _white.GetRawTextureData<Color>();
                var pixels = _png.GetRawTextureData<Color32>();
                double score = 0.0;
                for (int index = 0; index < pixels.Length; index++)
                {
                    Color b = black[index], w = white[index];
                    if (float.IsNaN(b.r + b.g + b.b + w.r + w.g + w.b)
                        || float.IsInfinity(b.r + b.g + b.b + w.r + w.g + w.b))
                        throw new InvalidOperationException("特效 " + _effectName + " 的原版 shader 输出非有限颜色");
                    float tr = w.r - b.r, tg = w.g - b.g, tb = w.b - b.b;
                    if (Mathf.Max(tr, Mathf.Max(tg, tb)) - Mathf.Min(tr, Mathf.Min(tg, tb)) > 0.08f)
                        throw new InvalidOperationException("特效 " + _effectName + " 的原版 shader 透射不能用单一 RGBA alpha 表示");
                    float alpha = 1f - Mathf.Clamp01((tr + tg + tb) / 3f);
                    pixels[index] = new Color32(Byte(b.r), Byte(b.g), Byte(b.b), Byte(alpha));
                    Color32 pixel = pixels[index];
                    // Include RGB independently of alpha so additive particles count too.
                    score += Math.Max(pixel.r, Math.Max(pixel.g, pixel.b)) + pixel.a;
                }
                // Store raw premultiplied RGB, including positive RGB at A=0.
                // Encode the readable CPU image directly; it is never sampled by a GPU.
                string path = Path.Combine(_directory, _frameFile.Replace('/', Path.DirectorySeparatorChar));
                Directory.CreateDirectory(Path.GetDirectoryName(path));
                File.WriteAllBytes(path, _png.EncodeToPNG());
                _frameScore = score;
            }
            catch (Exception error) { _renderError = error; }
            finally { RenderTexture.active = previous; _frameDone = true; }
        }

        private static byte Byte(float value) { return (byte)Mathf.RoundToInt(Mathf.Clamp01(value) * 255f); }

        private static void DestroyEffect()
        {
            if (_controller != null && !ReferenceEquals(_root, null) && NativeInstances != null)
            {
                var list = NativeInstances.GetValue(_controller) as List<GameObject>;
                if (list != null) list.Remove(_root);
            }
            if (_root != null)
            {
                if (_system != null) _system.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
                _root.SetActive(false);
                UnityEngine.Object.Destroy(_root);
            }
            _root = null;
            _system = null;
            _particles = null;
            _emissionEnds = null;
        }

        private static void Finish(string state, string message)
        {
            if (!_busy) return;
            try
            {
                RenderPipelineManager.endFrameRendering -= RenderFrame;
                _pending = false;
                DestroyEffect();
                if (_camera != null) _camera.targetTexture = null;
                if (_target != null) { _target.Release(); UnityEngine.Object.Destroy(_target); }
                if (_black != null) UnityEngine.Object.Destroy(_black);
                if (_white != null) UnityEngine.Object.Destroy(_white);
                if (_png != null) UnityEngine.Object.Destroy(_png);
                if (_cameraObject != null) UnityEngine.Object.Destroy(_cameraObject);
                WriteStatus(_directory, state, _completed, _total, message);
            }
            finally
            {
                _cameraObject = null; _camera = null; _target = null; _black = null; _white = null; _png = null;
                _controller = null; _runner = null; _busy = false;
                _map = null; _effects = null; _effectName = null; _frameFile = null;
            }
        }

        private static void WriteStatus(string directory, string state, int completed, int total, string message)
        {
            AtomicJson(Path.Combine(directory, "status.json"), new Dictionary<string, object>
            {
                {"state", state}, {"completed", completed}, {"total", total}, {"message", message}
            });
        }

        private static void AtomicJson(string path, object value)
        {
            string temporary = path + ".tmp";
            File.WriteAllText(temporary, ScenarioState.ToJson(value), new UTF8Encoding(false));
            if (File.Exists(path)) File.Replace(temporary, path, null);
            else File.Move(temporary, path);
        }
    }
}
