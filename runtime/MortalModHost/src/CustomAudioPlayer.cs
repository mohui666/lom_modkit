using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using BepInEx.Logging;
using Mortal.Core;
using UnityEngine;
using UnityEngine.Networking;

namespace MortalModHost
{
    /// <summary>
    /// 从当前演出 Mod 包播放自定义音频。只解析 <see cref="ModOverlay.CurrentPackage"/>，
    /// 不读取编辑器仓库，也不查找其它已加载 Mod 的同名 ID。
    ///
    /// 官方音频继续走 Wwise。自定义音频用 Unity AudioSource：
    /// WAV 优先同步解码；OGG（以及无法同步解码的 WAV）写入按 Mod 隔离的临时文件后
    /// 用 UnityWebRequestMultimedia 加载。
    /// </summary>
    internal static class CustomAudioPlayer
    {
        internal static ManualLogSource Log;

        private static MonoBehaviour _host;
        private static AudioSource _music;
        private static AudioSource _sfx;
        private static AudioSource _env;
        private static readonly Dictionary<string, AudioClip> _clips = new Dictionary<string, AudioClip>();
        private static Coroutine _musicFade;
        private static Coroutine _envFade;
        private static Coroutine _musicLoad;
        private static Coroutine _envLoad;
        private static string _currentMusicKey;
        private static string _currentEnvKey;

        public static void Init(MonoBehaviour host)
        {
            _host = host;
            var go = host.gameObject;
            _music = EnsureSource(go, "ModCustomMusic");
            _sfx = EnsureSource(go, "ModCustomSfx");
            _env = EnsureSource(go, "ModCustomEnv");
            _music.loop = true;
            _env.loop = true;
            _sfx.loop = false;
        }

        private static AudioSource EnsureSource(GameObject go, string name)
        {
            Transform child = go.transform.Find(name);
            GameObject target = child != null ? child.gameObject : new GameObject(name);
            if (child == null)
                target.transform.SetParent(go.transform, false);
            AudioSource source = target.GetComponent<AudioSource>();
            if (source == null)
                source = target.AddComponent<AudioSource>();
            source.playOnAwake = false;
            source.spatialBlend = 0f;
            source.volume = 1f;
            return source;
        }

        public static void StopAllImmediate()
        {
            StopCoroutine(ref _musicFade);
            StopCoroutine(ref _envFade);
            StopCoroutine(ref _musicLoad);
            StopCoroutine(ref _envLoad);
            if (_music != null) { _music.Stop(); _music.clip = null; _music.volume = 1f; }
            if (_env != null) { _env.Stop(); _env.clip = null; _env.volume = 1f; }
            if (_sfx != null) { _sfx.Stop(); }
            _currentMusicKey = null;
            _currentEnvKey = null;
        }

        public static void ReleaseAll()
        {
            StopAllImmediate();
            foreach (var pair in _clips)
            {
                if (pair.Value != null)
                    UnityEngine.Object.Destroy(pair.Value);
            }
            _clips.Clear();
            try
            {
                string root = Path.Combine(Path.GetTempPath(), "lom_modkit_audio");
                if (Directory.Exists(root))
                    Directory.Delete(root, true);
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogDebug("清理自定义音频临时目录失败：" + ex.Message);
            }
        }

        public static bool PlayMusic(string name)
        {
            return PlayKeyed(name, _music, loop: true, isMusic: true);
        }

        public static bool PlaySound(string name)
        {
            return PlayKeyed(name, _sfx, loop: false, isMusic: false);
        }

        public static bool PlayEnv(string name)
        {
            return PlayKeyed(name, _env, loop: true, isMusic: false);
        }

        public static void StopMusicImmediate()
        {
            StopCoroutine(ref _musicFade);
            StopCoroutine(ref _musicLoad);
            if (_music != null)
            {
                _music.Stop();
                _music.clip = null;
                _music.volume = 1f;
            }
            _currentMusicKey = null;
        }

        public static void StopEnvImmediate()
        {
            StopCoroutine(ref _envFade);
            StopCoroutine(ref _envLoad);
            if (_env != null)
            {
                _env.Stop();
                _env.clip = null;
                _env.volume = 1f;
            }
            _currentEnvKey = null;
        }

        public static bool IsCustomMusicPlaying()
        {
            return _music != null && _music.isPlaying;
        }

        public static bool IsCustomEnvPlaying()
        {
            return _env != null && _env.isPlaying;
        }

        public static void FadeOutMusic(float seconds)
        {
            if (_music == null || (!_music.isPlaying && _musicLoad == null))
                return;
            StopCoroutine(ref _musicFade);
            if (_host == null || seconds <= 0f)
            {
                StopMusicImmediate();
                return;
            }
            _musicFade = _host.StartCoroutine(FadeThenStop(_music, seconds, StopMusicImmediate));
        }

        public static void FadeOutEnv(float seconds)
        {
            if (_env == null || (!_env.isPlaying && _envLoad == null))
                return;
            StopCoroutine(ref _envFade);
            if (_host == null || seconds <= 0f)
            {
                StopEnvImmediate();
                return;
            }
            _envFade = _host.StartCoroutine(FadeThenStop(_env, seconds, StopEnvImmediate));
        }

        private static bool PlayKeyed(string name, AudioSource source, bool loop, bool isMusic)
        {
            ContentRef parsed;
            string error;
            if (!ContentRef.TryParse(name, out parsed, out error))
            {
                if (Log != null) Log.LogWarning("自定义音频引用无效：" + error);
                return false;
            }
            ModPackage package = ModOverlay.CurrentPackage;
            if (package == null)
            {
                if (Log != null) Log.LogWarning("当前没有演出中的 Mod，无法播放 " + parsed.Raw);
                return false;
            }
            UserContent content;
            if (!package.TryGetUserContent(parsed.ContentId, out content) || content == null || content.Bytes == null)
            {
                if (Log != null)
                    Log.LogWarning("Mod " + package.Id + " 的包内找不到用户音频 " + parsed.Raw + "（不会去其它 Mod 或本机内容库查找）");
                return false;
            }
            if (content.Type != "audio")
            {
                if (Log != null) Log.LogWarning(parsed.Raw + " 不是音频内容");
                return false;
            }

            if (isMusic)
                StopOfficialMusic();

            string cacheKey = package.Id + "/" + content.Id;
            AudioClip cached;
            if (_clips.TryGetValue(cacheKey, out cached) && cached != null)
            {
                StartSource(source, cached, loop, isMusic, cacheKey);
                return true;
            }

            AudioClip wavClip;
            string wavError;
            if (TryCreateWavClip(content.Bytes, cacheKey, out wavClip, out wavError))
            {
                _clips[cacheKey] = wavClip;
                StartSource(source, wavClip, loop, isMusic, cacheKey);
                return true;
            }

            if (_host == null)
            {
                if (Log != null) Log.LogWarning("无法异步加载自定义音频：" + parsed.Raw);
                return false;
            }
            if (isMusic)
            {
                StopCoroutine(ref _musicLoad);
                _musicLoad = _host.StartCoroutine(LoadThenPlay(package, content, source, loop, true, cacheKey));
            }
            else if (source == _env)
            {
                StopCoroutine(ref _envLoad);
                _envLoad = _host.StartCoroutine(LoadThenPlay(package, content, source, loop, false, cacheKey));
            }
            else
            {
                _host.StartCoroutine(LoadThenPlay(package, content, source, loop, false, cacheKey));
            }
            return true;
        }

        private static void StartSource(AudioSource source, AudioClip clip, bool loop, bool isMusic, string cacheKey)
        {
            if (source == null || clip == null)
                return;
            if (isMusic)
            {
                StopCoroutine(ref _musicFade);
                source.volume = 1f;
                _currentMusicKey = cacheKey;
            }
            else if (source == _env)
            {
                StopCoroutine(ref _envFade);
                source.volume = 1f;
                _currentEnvKey = cacheKey;
            }
            source.clip = clip;
            source.loop = loop;
            source.Play();
        }

        private static IEnumerator LoadThenPlay(
            ModPackage package,
            UserContent content,
            AudioSource source,
            bool loop,
            bool isMusic,
            string cacheKey)
        {
            string tempDir = Path.Combine(Path.GetTempPath(), "lom_modkit_audio", package.Id);
            Directory.CreateDirectory(tempDir);
            string ext = Path.GetExtension(content.MainFile);
            if (string.IsNullOrEmpty(ext))
                ext = ".ogg";
            string tempPath = Path.Combine(tempDir, content.Id + ext);
            File.WriteAllBytes(tempPath, content.Bytes);
            string uri = new Uri(tempPath).AbsoluteUri;
            AudioType audioType = ext.Equals(".wav", StringComparison.OrdinalIgnoreCase)
                ? AudioType.WAV
                : AudioType.OGGVORBIS;
            using (UnityWebRequest req = UnityWebRequestMultimedia.GetAudioClip(uri, audioType))
            {
                yield return req.SendWebRequest();
                if (req.result != UnityWebRequest.Result.Success)
                {
                    if (Log != null)
                        Log.LogWarning("加载自定义音频失败 " + content.Id + "：" + req.error);
                    yield break;
                }
                AudioClip clip = DownloadHandlerAudioClip.GetContent(req);
                if (clip == null)
                {
                    if (Log != null) Log.LogWarning("自定义音频解码失败：" + content.Id);
                    yield break;
                }
                clip.name = cacheKey;
                _clips[cacheKey] = clip;
                StartSource(source, clip, loop, isMusic, cacheKey);
            }
        }

        private static IEnumerator FadeThenStop(AudioSource source, float seconds, Action done)
        {
            float start = source != null ? source.volume : 0f;
            float elapsed = 0f;
            while (elapsed < seconds && source != null)
            {
                elapsed += Time.unscaledDeltaTime;
                source.volume = Mathf.Lerp(start, 0f, Mathf.Clamp01(elapsed / seconds));
                yield return null;
            }
            if (done != null)
                done();
        }

        private static void StopOfficialMusic()
        {
            try
            {
                if (SoundManager.Instance != null)
                    SoundManager.Instance.StopMusic();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogDebug("停止官方音乐失败：" + ex.Message);
            }
        }

        private static void StopCoroutine(ref Coroutine routine)
        {
            if (routine != null && _host != null)
                _host.StopCoroutine(routine);
            routine = null;
        }

        /// <summary>仅支持常见 PCM / IEEE float WAV；失败时由调用方改走 UnityWebRequest。</summary>
        internal static bool TryCreateWavClip(byte[] data, string name, out AudioClip clip, out string error)
        {
            clip = null;
            error = null;
            if (data == null || data.Length < 44)
            {
                error = "文件太短，不是完整 WAV";
                return false;
            }
            if (ReadFour(data, 0) != "RIFF" || ReadFour(data, 8) != "WAVE")
            {
                error = "不是 RIFF/WAVE";
                return false;
            }
            int offset = 12;
            int channels = 0;
            int sampleRate = 0;
            int bits = 0;
            int formatTag = 0;
            int dataPos = -1;
            int dataLen = 0;
            while (offset + 8 <= data.Length)
            {
                string chunk = ReadFour(data, offset);
                int size = BitConverter.ToInt32(data, offset + 4);
                if (size < 0 || offset + 8 + size > data.Length)
                    break;
                if (chunk == "fmt ")
                {
                    formatTag = BitConverter.ToInt16(data, offset + 8);
                    channels = BitConverter.ToInt16(data, offset + 10);
                    sampleRate = BitConverter.ToInt32(data, offset + 12);
                    bits = BitConverter.ToInt16(data, offset + 22);
                }
                else if (chunk == "data")
                {
                    dataPos = offset + 8;
                    dataLen = size;
                }
                offset += 8 + size + (size & 1);
            }
            if (dataPos < 0 || channels <= 0 || sampleRate <= 0 || dataLen <= 0)
            {
                error = "WAV 缺少 fmt/data";
                return false;
            }
            int bytesPerSample = bits / 8;
            if (bytesPerSample <= 0)
            {
                error = "不支持的位深 " + bits;
                return false;
            }
            int sampleCount = dataLen / (bytesPerSample * channels);
            if (sampleCount <= 0)
            {
                error = "WAV 没有采样";
                return false;
            }
            float[] samples = new float[sampleCount * channels];
            try
            {
                if (formatTag == 1 && bits == 16)
                {
                    int i = 0;
                    for (int s = 0; s < sampleCount * channels; s++)
                        samples[s] = BitConverter.ToInt16(data, dataPos + (i += 2) - 2) / 32768f;
                }
                else if (formatTag == 1 && bits == 8)
                {
                    for (int s = 0; s < sampleCount * channels; s++)
                        samples[s] = (data[dataPos + s] - 128) / 128f;
                }
                else if ((formatTag == 3 || formatTag == 1) && bits == 32)
                {
                    int i = 0;
                    for (int s = 0; s < sampleCount * channels; s++)
                        samples[s] = BitConverter.ToSingle(data, dataPos + (i += 4) - 4);
                }
                else
                {
                    error = "不支持的 WAV 格式 tag=" + formatTag + " bits=" + bits;
                    return false;
                }
            }
            catch (Exception ex)
            {
                error = "解析 WAV 失败：" + ex.Message;
                return false;
            }
            clip = AudioClip.Create(name, sampleCount, channels, sampleRate, false);
            clip.SetData(samples, 0);
            return true;
        }

        private static string ReadFour(byte[] data, int offset)
        {
            return ((char)data[offset]).ToString()
                + (char)data[offset + 1]
                + (char)data[offset + 2]
                + (char)data[offset + 3];
        }
    }
}
