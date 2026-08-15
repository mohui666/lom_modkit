using System;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;

namespace MortalModHost
{
    /// <summary>
    /// 拦截 LuaManager 音频 API：官方名字放行给 Wwise；user: 引用改走 CustomAudioPlayer。
    /// Prefix 一律吞掉异常，避免自定义音频把整局游戏打崩。
    /// </summary>
    [HarmonyPatch(typeof(LuaManager), "PlayMusic")]
    internal static class PlayMusicPatch
    {
        private static bool Prefix(string name)
        {
            try
            {
                if (!ContentRef.IsUserRef(name))
                {
                    CustomAudioPlayer.StopMusicImmediate();
                    return true;
                }
                CustomAudioPlayer.PlayMusic(name);
                return false;
            }
            catch (Exception ex)
            {
                if (CustomAudioPlayer.Log != null)
                    CustomAudioPlayer.Log.LogError("PlayMusic 补丁异常：" + ex);
                return !ContentRef.IsUserRef(name);
            }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "PlaySound")]
    internal static class PlaySoundPatch
    {
        private static bool Prefix(string name)
        {
            try
            {
                if (!ContentRef.IsUserRef(name))
                    return true;
                CustomAudioPlayer.PlaySound(name);
                return false;
            }
            catch (Exception ex)
            {
                if (CustomAudioPlayer.Log != null)
                    CustomAudioPlayer.Log.LogError("PlaySound 补丁异常：" + ex);
                return !ContentRef.IsUserRef(name);
            }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "PlayEnvSound")]
    internal static class PlayEnvSoundPatch
    {
        private static bool Prefix(string name)
        {
            try
            {
                if (!ContentRef.IsUserRef(name))
                {
                    CustomAudioPlayer.StopEnvImmediate();
                    return true;
                }
                CustomAudioPlayer.PlayEnv(name);
                return false;
            }
            catch (Exception ex)
            {
                if (CustomAudioPlayer.Log != null)
                    CustomAudioPlayer.Log.LogError("PlayEnvSound 补丁异常：" + ex);
                return !ContentRef.IsUserRef(name);
            }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "StopMusic")]
    internal static class StopMusicPatch
    {
        private static void Prefix()
        {
            try { CustomAudioPlayer.StopAllImmediate(); }
            catch { }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "FadeOutMusic")]
    internal static class FadeOutMusicPatch
    {
        private static void Prefix(float second)
        {
            try
            {
                if (CustomAudioPlayer.IsCustomMusicPlaying())
                    CustomAudioPlayer.FadeOutMusic(second);
            }
            catch { }
        }
    }

    [HarmonyPatch(typeof(LuaManager), "FadeOutEnvSound")]
    internal static class FadeOutEnvSoundPatch
    {
        private static void Prefix(float second)
        {
            try
            {
                if (CustomAudioPlayer.IsCustomEnvPlaying())
                    CustomAudioPlayer.FadeOutEnv(second);
            }
            catch { }
        }
    }

    /// <summary>
    /// 标题 / 自由模式 / 读档等走 <see cref="SoundManager"/>，不经过 LuaManager。
    /// 官方 BGM 一起就必须掐掉自定义 waveOut，否则两轨叠播。
    /// </summary>
    [HarmonyPatch(typeof(SoundManager), "PlayMusic")]
    internal static class SoundManagerPlayMusicPatch
    {
        private static void Prefix()
        {
            try { CustomAudioPlayer.StopMusicImmediate(); }
            catch { }
        }
    }

    /// <summary>
    /// 官方 <c>StopMusic</c> 实际是停掉全部 Wwise（含环境音）。自定义 waveOut 不在
    /// Wwise 里，音乐/环境/音效必须一并清掉。对白语音单独通道，留给切场景处理，
    /// 避免剧情里的 StopMusic 误杀正在说的那句。
    /// 自定义切歌主动调用时由 <see cref="CustomAudioPlayer.SuppressOfficialStopHook"/> 跳过。
    /// </summary>
    [HarmonyPatch(typeof(SoundManager), "StopMusic")]
    internal static class SoundManagerStopMusicPatch
    {
        private static void Prefix()
        {
            if (CustomAudioPlayer.SuppressOfficialStopHook)
                return;
            try { CustomAudioPlayer.StopAllImmediate(); }
            catch { }
        }
    }

    /// <summary>
    /// 切场景（回标题、进自由、战斗、死亡、结局）立刻停自定义音频。
    /// 不能等目标场景 Start 再停：Loading 期间 waveOut 会继续响。
    /// </summary>
    [HarmonyPatch(typeof(SceneController), "LoadNewScene")]
    internal static class LoadNewSceneAudioPatch
    {
        private static void Prefix()
        {
            try
            {
                CustomAudioPlayer.StopEverything();
                CustomCharacterRuntime.ClearAll();
                CustomImageRuntime.ClearAll();
            }
            catch { }
        }
    }
}
