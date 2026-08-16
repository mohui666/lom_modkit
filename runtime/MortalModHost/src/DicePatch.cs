using System;
using System.Collections.Generic;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// One author-configured roll rendered by the original DiceMenuDialog. The
    /// sentinel key never reaches the original checkpoint collection.
    /// </summary>
    internal static class CustomDiceSession
    {
        internal const string CheckName = "__lom_mod_custom";
        private static readonly List<int> UpperBounds = new List<int>();
        private static int _maximum;
        private static int _bonus;
        private static string _header = "";
        private static string _bonusName = "";
        private static string _bonusStatus = "";
        private static bool _active;

        internal static void Prepare(
            int maximum, string header, string encodedBounds, int bonus,
            string bonusName, string bonusStatus)
        {
            if (!DiceModSupport.IsModStoryActive())
                throw new InvalidOperationException("自定义骰子只能在 MOD 剧情会话中使用");
            if (maximum < 1 || maximum > 9999)
                throw new ArgumentOutOfRangeException("maximum");
            if (string.IsNullOrWhiteSpace(header) || header.Length > 80)
                throw new ArgumentException("骰子标题必须是 1~80 字符");
            if (bonus < -9999 || bonus > 9999)
                throw new ArgumentOutOfRangeException("bonus");
            var parsed = new List<int>();
            string[] pieces = (encodedBounds ?? "").Split(',');
            if (pieces.Length < 1 || pieces.Length > 3)
                throw new ArgumentException("骰子必须有 2~4 个结果分段");
            int previous = int.MinValue;
            foreach (string piece in pieces)
            {
                int value;
                if (!int.TryParse(piece, out value) || value <= previous)
                    throw new ArgumentException("骰子结果上限必须是严格递增的整数");
                if (value < bonus || value >= maximum + bonus)
                    throw new ArgumentException("骰子结果上限超出可投出的总点数，或使后一档不可到达");
                parsed.Add(value);
                previous = value;
            }
            UpperBounds.Clear();
            UpperBounds.AddRange(parsed);
            _maximum = maximum;
            _bonus = bonus;
            _header = header;
            _bonusName = bonusName ?? "";
            _bonusStatus = bonusStatus ?? "";
            _active = true;
        }

        internal static bool TryEvaluate(string checkName, int random, out DiceCheckResult result)
        {
            result = null;
            if (!_active || !string.Equals(checkName, CheckName, StringComparison.Ordinal))
                return false;
            if (random < -50 || random > Math.Max(150, _maximum))
                throw new InvalidOperationException("原版骰子返回了越界点数");
            result = new DiceCheckResult(random) { Header = _header };
            if (_bonus != 0)
            {
                result.Additions.Add(new DiceCheckAddition
                {
                    Name = string.IsNullOrEmpty(_bonusName) ? "加值" : _bonusName,
                    Status = _bonusStatus,
                    Value = _bonus,
                    Sign = _bonus > 0 ? 1 : -1,
                });
            }
            int total = random + _bonus;
            int selection = UpperBounds.Count + 1;
            for (int index = 0; index < UpperBounds.Count; index++)
            {
                if (total <= UpperBounds[index])
                {
                    selection = index + 1;
                    break;
                }
            }
            result.Result = selection;
            return true;
        }

        internal static void Clear()
        {
            _active = false;
            _maximum = 0;
            _bonus = 0;
            _header = "";
            _bonusName = "";
            _bonusStatus = "";
            UpperBounds.Clear();
        }
    }

    [HarmonyPatch(typeof(CheckPointManager), "Dice",
        new Type[] { typeof(string), typeof(int) })]
    internal static class CustomDiceResultPatch
    {
        private static bool Prefix(string name, int random, ref DiceCheckResult __result)
        {
            DiceCheckResult result;
            if (!CustomDiceSession.TryEvaluate(name, random, out result)) return true;
            __result = result;
            return false;
        }
    }

    /// <summary>
    /// 契约 §6.16：mod 剧情中放开骰子范围修改（官方剧情完全不受影响）。
    /// 根因（ilspycmd 反编译核实）：
    /// <list type="number">
    /// <item>逆天：DiceMenuDialog.CheckRevolution()（private bool）需要
    ///   PlayerStatManagerData.AlphaTest 或 _useFatePoint 或
    ///   Stats.Get(GameStatType.命運).FinalValue &gt; 0。mod 新战役命运=0 → 没有逆天流程。
    ///   修法：NewGameDataPatch.Postfix 给 mod 新战役发 2 点命运。</item>
    /// <item>修改范围：ChangeDiceRange() 门是 NewGamePlus &amp;&amp; _rangeButton.gameObject.activeSelf；
    ///   _rangeButton 显示条件（CheckRevolution 为 true 后的循环里）是 NewGamePlus &amp;&amp;
    ///   LibrarySystem.Achievement.Get("30016").Count &gt; 0——二周目 + 成就 30016，mod 新战役都不满足。
    ///   修法：get_NewGamePlus prefix 在 mod 剧情中返 true；CheckRevolution postfix 在原返回 true 且
    ///   mod 剧情中时直接 _rangeButton.gameObject.SetActive(true)，绕开成就门槛（绝不在 mod 里
    ///   解锁官方成就 30016，避免污染官方存档宇宙）。</item>
    /// </list>
    /// </summary>
    internal static class DiceModSupport
    {
        /// <summary>
        /// 当前是否处于 mod 剧情：PlayerStatManagerData.CurrentStoryScript 以 "MOD_" 开头。
        /// Instance 拿不到/异常一律按 false（不影响官方）。
        /// </summary>
        internal static bool IsModStoryActive()
        {
            try
            {
                PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                if (stat == null) return false;
                string script = stat.CurrentStoryScript;
                return script != null && script.StartsWith("MOD_", StringComparison.Ordinal);
            }
            catch
            {
                return false;
            }
        }
    }

    /// <summary>
    /// Harmony prefix：PlayerStatManagerData.get_NewGamePlus。
    /// mod 剧情进行中（CurrentStoryScript 以 MOD_ 开头）一律返回 true（跳过原 getter），
    /// 其余情况放行原方法——官方剧情/存档读写读到的仍是真实 _newGamePlus 值。
    /// 影响边界：只改读值不改 _newGamePlus 后备字段，SaveGameData 落盘不受影响；
    /// mod 剧情内任何读 NewGamePlus 的系统（骰子 ChangeDiceRange 等）都看到 true，
    /// 官方脚本演出时该前缀完全不生效。
    /// </summary>
    [HarmonyPatch(typeof(PlayerStatManagerData), "get_NewGamePlus")]
    internal static class NewGamePlusPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        private static bool Prefix(PlayerStatManagerData __instance, ref bool __result)
        {
            if (!DiceModSupport.IsModStoryActive()) return true; // 官方路径：放行原 getter
            __result = true;
            return false;
        }
    }

    /// <summary>
    /// Harmony postfix：DiceMenuDialog.CheckRevolution()（private bool）。
    /// 原返回 true（命运>0 等，mod 新战役有 NewGameDataPatch 发的 2 点命运）且 mod 剧情中时，
    /// 把 _rangeButton（MenuToggleButton）gameObject.SetActive(true)——补上官方要求
    /// "二周目 + 成就 30016"才显示的按钮（mod 里不解锁官方成就，直接激活按钮）。
    /// 按钮激活发生在官方 CheckRevolution 调用点的 SetActive 判定之前，效果等同官方显示分支。
    /// </summary>
    [HarmonyPatch(typeof(DiceMenuDialog), "CheckRevolution")]
    internal static class DiceRevolutionPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        private static void Postfix(DiceMenuDialog __instance, bool __result)
        {
            if (!__result || !DiceModSupport.IsModStoryActive()) return;
            try
            {
                MenuToggleButton button = Traverse.Create(__instance).Field("_rangeButton").GetValue<MenuToggleButton>();
                if (button != null)
                    button.gameObject.SetActive(true);
            }
            catch (Exception ex)
            {
                Log?.LogWarning("mod 剧情骰子范围按钮激活失败：" + ex.Message);
            }
        }
    }
}
