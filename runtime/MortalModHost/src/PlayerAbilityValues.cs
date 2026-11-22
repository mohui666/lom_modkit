using System;
using System.Collections.Generic;
using System.Reflection;
using System.Reflection.Emit;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    [HarmonyPatch]
    internal static class PlayerAbilityValues
    {
        private const int OriginalCap = 100;
        private static readonly MethodInfo Clamp = AccessTools.Method(typeof(Mathf), nameof(Mathf.Clamp),
            new[] { typeof(int), typeof(int), typeof(int) });

        private static IEnumerable<MethodBase> TargetMethods()
        {
            yield return AccessTools.Method(typeof(GameStat), nameof(GameStat.SetValue));
            yield return AccessTools.Method(typeof(GameStat), nameof(GameStat.AddValue));
            yield return AccessTools.PropertyGetter(typeof(GameStat), nameof(GameStat.FinalValue));
        }

        // Keep the game's writes, events, total-add stat and save format. Only
        // replace the value bound. Max remains the original scale for the UI.
        private static IEnumerable<CodeInstruction> Transpiler(
            IEnumerable<CodeInstruction> instructions, MethodBase original)
        {
            bool replaceAdd = original.Name != nameof(GameStat.SetValue);
            int replacedClamps = 0;
            foreach (CodeInstruction instruction in instructions)
            {
                string replacement = null;
                if (replaceAdd && instruction.opcode == OpCodes.Add)
                {
                    replacement = nameof(AddValue);
                    replaceAdd = false;
                }
                else if (instruction.Calls(Clamp))
                {
                    replacement = nameof(ClampValue);
                    replacedClamps++;
                }
                if (replacement == null)
                {
                    yield return instruction;
                    continue;
                }

                var loadStat = new CodeInstruction(OpCodes.Ldarg_0);
                loadStat.labels.AddRange(instruction.labels);
                loadStat.blocks.AddRange(instruction.blocks);
                yield return loadStat;
                yield return CodeInstruction.Call(typeof(PlayerAbilityValues), replacement);
            }
            if (replacedClamps != 1 || replaceAdd)
                throw new InvalidOperationException("无法定位原版能力值运算：" + original.Name);
        }

        private static int AddValue(int value, int delta, GameStat stat)
        {
            long sum = (long)value + delta;
            if ((sum > int.MaxValue || sum < int.MinValue) && CanExceedCap(stat))
                return (int)Math.Max(int.MinValue, Math.Min(int.MaxValue, sum));
            return unchecked((int)sum);
        }

        private static int ClampValue(int value, int min, int max, GameStat stat)
        {
            return value > max && CanExceedCap(stat) ? value : Mathf.Clamp(value, min, max);
        }

        private static bool CanExceedCap(GameStat stat)
        {
            if (ModPlayModeSession.Selected == ModPlayModeKind.Original || stat.Max != OriginalCap)
                return false;
            GameStatType type = stat.StatType;
            if (!IsAbility(type)) return false;
            PlayerStatManagerData player = PlayerStatManagerData.Instance;
            return player != null && player.Stats != null && ReferenceEquals(player.Stats.Get(type), stat);
        }

        private static bool IsAbility(GameStatType type)
        {
            switch (type)
            {
                case GameStatType.體力:
                case GameStatType.內力:
                case GameStatType.輕功:
                case GameStatType.魅力:
                case GameStatType.學問:
                case GameStatType.嘴力:
                case GameStatType.廚藝:
                case GameStatType.抗毒:
                case GameStatType.抗麻:
                case GameStatType.防禦:
                case GameStatType.武功刀劍:
                case GameStatType.武功暗器:
                case GameStatType.武功拳掌:
                case GameStatType.武功腿法:
                case GameStatType.武功奇門:
                case GameStatType.武功軟兵器:
                case GameStatType.武功槍棍:
                case GameStatType.武功內功:
                case GameStatType.儒學:
                case GameStatType.道學:
                case GameStatType.佛學:
                    return true;
                default:
                    return false;
            }
        }

        [HarmonyPatch(typeof(GameStatUtils), nameof(GameStatUtils.GetGameStatLevel))]
        private static class AbilityLevelText
        {
            private static bool Prefix(int value, int max, int levelLength, ref int __result)
            {
                if (ModPlayModeSession.Selected == ModPlayModeKind.Original
                    || max != OriginalCap || value <= max || levelLength == 0)
                    return true;
                __result = levelLength - 1;
                return false;
            }
        }
    }
}
