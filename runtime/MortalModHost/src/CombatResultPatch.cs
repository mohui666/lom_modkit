using HarmonyLib;
using Mortal.Combat;

namespace MortalModHost
{
    /// <summary>只观察原版 CombatManager 的真实 bool 结果，不推测 draw/escape。</summary>
    [HarmonyPatch(typeof(CombatManager), "GameOver", new System.Type[] { typeof(bool) })]
    internal static class CombatResultPatch
    {
        private static void Prefix(bool win)
        {
            GameplaySession.RecordResult("combat", win ? "win" : "lose");
        }
    }

    /// <summary>
    /// 高层 combat 明确提供 lose 续接时，把原版 DeadEnd 局限在该 MOD 战斗会话内改为 false，
    /// 让 CombatManager 走已验证的 LoadNextScene() 回 Story。其它战斗完全不变。
    /// </summary>
    [HarmonyPatch(typeof(CombatLevel), "get_DeadEnd")]
    internal static class CombatDeadEndPatch
    {
        private static void Postfix(ref bool __result)
        {
            if (GameplaySession.ShouldForceCombatReturn)
                __result = false;
        }
    }
}
