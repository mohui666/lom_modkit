using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using System.Reflection.Emit;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using Mortal.Story;
using GameLevelManager = Mortal.Battle.GameLevelManager;

namespace MortalModHost
{
    [HarmonyPatch]
    internal static class GameplayMusic
    {
        private static readonly MethodInfo PlayMusic = AccessTools.Method(
            typeof(SoundManager), nameof(SoundManager.PlayMusic), new[] { typeof(string) });

        internal static string Resolve(LuaManager manager, string name)
        {
            if (ContentRef.IsUserRef(name))
                return name;

            var collection = Traverse.Create(manager).Field("_musicData").GetValue<MusicDataCollection>();
            string key = collection != null ? collection.GetKey(name) : "";
            if (string.IsNullOrEmpty(key))
                throw new InvalidOperationException("游戏音乐库中找不到战斗音乐：" + name);
            return key;
        }

        private static IEnumerable<MethodBase> TargetMethods()
        {
            yield return AccessTools.EnumeratorMoveNext(
                AccessTools.Method(typeof(CombatManager), "LoadCombatLevelAsset"));
            yield return AccessTools.EnumeratorMoveNext(
                AccessTools.Method(typeof(GameLevelManager), "Setup"));
        }

        // Replace the opening cue in these two setup coroutines. The game owns
        // every other music call, including the duel victory cue.
        private static IEnumerable<CodeInstruction> Transpiler(
            IEnumerable<CodeInstruction> instructions, MethodBase original)
        {
            var replacement = AccessTools.Method(typeof(GameplayMusic), nameof(PlayOpening));
            int count = 0;
            foreach (CodeInstruction instruction in instructions)
            {
                if (instruction.Calls(PlayMusic))
                {
                    instruction.opcode = OpCodes.Call;
                    instruction.operand = replacement;
                    count++;
                }
                yield return instruction;
            }
            if (count != 1)
                throw new InvalidOperationException("无法定位原版战斗进场音乐：" + original.DeclaringType);
        }

        private static void PlayOpening(SoundManager sound, string originalKey)
        {
            GameplaySession.MusicOverrideApplied = false;
            string selected = GameplaySession.ConfigString("music");
            if (GameplaySession.HasPending && !string.IsNullOrEmpty(selected))
            {
                if (!ContentRef.IsUserRef(selected))
                {
                    sound.PlayMusic(selected);
                    GameplaySession.MusicOverrideApplied = true;
                    return;
                }
                ModPackage owner = CustomGameplayRuntime.CurrentPackage();
                if (GameplaySession.BelongsTo(owner) && CustomAudioPlayer.PlayMusic(selected, owner))
                {
                    GameplaySession.MusicOverrideApplied = true;
                    return;
                }
                LuaManagerPatch.Log?.LogWarning("无法播放本场自定义音乐，使用关卡音乐：" + selected);
            }
            sound.PlayMusic(originalKey);
        }
    }

    [HarmonyPatch(typeof(GameLevelManager), "ChangeMusic")]
    internal static class BattleMusicChangePatch
    {
        private static bool Prefix(ref IEnumerator __result)
        {
            if (!GameplaySession.PendingBattle || !GameplaySession.MusicOverrideApplied)
                return true;
            __result = Array.Empty<object>().GetEnumerator();
            return false;
        }
    }

    [HarmonyPatch(typeof(SceneController), nameof(SceneController.LoadNextScene))]
    internal static class GameplayMusicExitPatch
    {
        private static void Prefix()
        {
            // This return path calls StartLoadScene directly, bypassing LoadNewScene.
            if (GameplaySession.MusicOverrideApplied
                && ContentRef.IsUserRef(GameplaySession.ConfigString("music")))
                CustomAudioPlayer.StopMusicImmediate();
        }
    }
}
