using System;
using System.Collections;
using BepInEx.Logging;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 从当前演出 Mod 包播放自定义音频。只解析 <see cref="ModOverlay.CurrentPackage"/>。
    /// 流式解码后走 winmm waveOut，不经过 Unity 混音器（Wwise 工程里 AudioSource 经常无声）。
    /// </summary>
    internal static class CustomAudioPlayer
    {
        internal static ManualLogSource Log;

        private static MonoBehaviour _host;
        private static NativePcmPlayer _music;
        private static NativePcmPlayer _sfx;
        private static NativePcmPlayer _env;
        private static NativePcmPlayer _voice;
        private static Coroutine _musicFade;
        private static Coroutine _envFade;

        public static void Init(MonoBehaviour host)
        {
            _host = host;
            if (_music == null) _music = new NativePcmPlayer();
            if (_sfx == null) _sfx = new NativePcmPlayer();
            if (_env == null) _env = new NativePcmPlayer();
            if (_voice == null) _voice = new NativePcmPlayer();
        }

        public static void StopAllImmediate()
        {
            try
            {
                StopCoroutine(ref _musicFade);
                StopCoroutine(ref _envFade);
                if (_music != null) _music.Stop();
                if (_env != null) _env.Stop();
                if (_sfx != null) _sfx.Stop();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("停止自定义音频失败：" + ex.Message);
            }
        }

        public static void StopVoice()
        {
            try
            {
                if (_voice != null) _voice.Stop();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("停止对白语音失败：" + ex.Message);
            }
        }

        public static void StopEverything()
        {
            StopAllImmediate();
            StopVoice();
        }

        public static bool PlayVoice(string name)
        {
            StopVoice();
            if (string.IsNullOrEmpty(name))
                return true;
            return PlayKeyed(name, _voice, loop: false, isMusic: false);
        }

        public static void ReleaseAll()
        {
            StopEverything();
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
            try
            {
                StopCoroutine(ref _musicFade);
                if (_music != null) _music.Stop();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("停止自定义音乐失败：" + ex.Message);
            }
        }

        public static void StopEnvImmediate()
        {
            try
            {
                StopCoroutine(ref _envFade);
                if (_env != null) _env.Stop();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("停止自定义环境音失败：" + ex.Message);
            }
        }

        public static bool IsCustomMusicPlaying()
        {
            return _music != null && _music.IsPlaying;
        }

        public static bool IsCustomEnvPlaying()
        {
            return _env != null && _env.IsPlaying;
        }

        public static void FadeOutMusic(float seconds)
        {
            if (_music == null || !_music.IsPlaying)
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
            if (_env == null || !_env.IsPlaying)
                return;
            StopCoroutine(ref _envFade);
            if (_host == null || seconds <= 0f)
            {
                StopEnvImmediate();
                return;
            }
            _envFade = _host.StartCoroutine(FadeThenStop(_env, seconds, StopEnvImmediate));
        }

        private static bool PlayKeyed(string name, NativePcmPlayer player, bool loop, bool isMusic)
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
                    Log.LogWarning("Mod " + package.Id + " 的包内找不到用户音频 " + parsed.Raw);
                return false;
            }
            if (player == null)
            {
                if (Log != null) Log.LogWarning("自定义音频播放器未初始化");
                return false;
            }

            if (isMusic)
                StopOfficialMusic();

            try
            {
                if (isMusic)
                    StopCoroutine(ref _musicFade);
                else if (player == _env)
                    StopCoroutine(ref _envFade);
                string ext = System.IO.Path.GetExtension(content.MainFile ?? "");
                player.Play(content.Bytes, ext, loop);
                player.SetVolume(GameVolume(isMusic || player == _env));
                if (Log != null)
                    Log.LogInfo("自定义音频开始播放：" + parsed.Raw + "（waveOut 流式 " + ext + "）");
                return true;
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("自定义音频播放失败 " + parsed.Raw + "：" + ex.Message);
                return false;
            }
        }

        private static float GameVolume(bool musicLike)
        {
            try
            {
                float master = SystemSettings.MasterVolume / 100f;
                float bus = musicLike
                    ? SystemSettings.MusicVolume / 100f
                    : SystemSettings.SoundVolume / 100f;
                float v = master * bus;
                if (v < 0.05f) v = 0.05f;
                if (v > 1f) v = 1f;
                return v;
            }
            catch
            {
                return 1f;
            }
        }

        private static IEnumerator FadeThenStop(NativePcmPlayer player, float seconds, Action done)
        {
            float elapsed = 0f;
            float start = GameVolume(player == _music || player == _env);
            while (elapsed < seconds && player != null && player.IsPlaying)
            {
                elapsed += Time.unscaledDeltaTime;
                try { player.SetVolume(Mathf.Lerp(start, 0f, Mathf.Clamp01(elapsed / seconds))); }
                catch { break; }
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
            {
                try { _host.StopCoroutine(routine); } catch { }
            }
            routine = null;
        }
    }
}
