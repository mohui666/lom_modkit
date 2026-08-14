using HarmonyLib;
using Mortal.Story;

namespace MortalModHost
{
    /// <summary>
    /// 拦截 LuaManager 音频 API：官方名字放行给 Wwise；user: 引用改走 CustomAudioPlayer。
    /// 编译器仍发射 PlayMusic/PlaySound/PlayEnvSound，避免改 story schema。
    /// </summary>
    [HarmonyPatch(typeof(LuaManager), "PlayMusic")]
    internal static class PlayMusicPatch
    {
        private static bool Prefix(string name)
        {
            if (!ContentRef.IsUserRef(name))
            {
                CustomAudioPlayer.StopMusicImmediate();
                return true;
            }
            CustomAudioPlayer.PlayMusic(name);
            return false;
        }
    }

    [HarmonyPatch(typeof(LuaManager), "PlaySound")]
    internal static class PlaySoundPatch
    {
        private static bool Prefix(string name)
        {
            if (!ContentRef.IsUserRef(name))
                return true;
            CustomAudioPlayer.PlaySound(name);
            return false;
        }
    }

    [HarmonyPatch(typeof(LuaManager), "PlayEnvSound")]
    internal static class PlayEnvSoundPatch
    {
        private static bool Prefix(string name)
        {
            if (!ContentRef.IsUserRef(name))
            {
                CustomAudioPlayer.StopEnvImmediate();
                return true;
            }
            CustomAudioPlayer.PlayEnv(name);
            return false;
        }
    }

    [HarmonyPatch(typeof(LuaManager), "StopMusic")]
    internal static class StopMusicPatch
    {
        private static void Prefix()
        {
            CustomAudioPlayer.StopAllImmediate();
        }
    }

    [HarmonyPatch(typeof(LuaManager), "FadeOutMusic")]
    internal static class FadeOutMusicPatch
    {
        private static void Prefix(float second)
        {
            if (CustomAudioPlayer.IsCustomMusicPlaying())
                CustomAudioPlayer.FadeOutMusic(second);
        }
    }

    [HarmonyPatch(typeof(LuaManager), "FadeOutEnvSound")]
    internal static class FadeOutEnvSoundPatch
    {
        private static void Prefix(float second)
        {
            if (CustomAudioPlayer.IsCustomEnvPlaying())
                CustomAudioPlayer.FadeOutEnv(second);
        }
    }
}
