using System;
using HarmonyLib;
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
}
