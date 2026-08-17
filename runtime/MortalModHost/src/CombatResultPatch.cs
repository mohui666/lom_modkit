using System;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;

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

    /// <summary>
    /// CombatManager.GameOver 是 iterator 工厂，且原版还有可能直接调用场景出口。
    /// 因此在真正的 SceneController.LoadNextScene 边界再次以双方死亡状态确认结果。
    /// 没有任何一方死亡却离开 Combat 属于异常生命周期，必须阻止重跑同一 Story。
    /// </summary>
    [HarmonyPatch(typeof(SceneController), "LoadNextScene")]
    internal static class CombatNextSceneGuardPatch
    {
        private static bool Prefix()
        {
            if (!GameplaySession.PendingCombat || GameplaySession.HasRecordedResult)
                return true;
            try
            {
                CombatManager manager = CombatManager.Instance;
                if (manager == null)
                    throw new InvalidOperationException("Combat 离场时 CombatManager.Instance 为 null");
                CombatActionController enemy = Traverse.Create(manager)
                    .Field("_enemyAction").GetValue<CombatActionController>();
                CombatActionController player = Traverse.Create(manager)
                    .Field("_playerAction").GetValue<CombatActionController>();
                if (enemy == null || player == null)
                    throw new InvalidOperationException("Combat 离场时敌我 ActionController 不完整");

                // 原版 ActionRoundHandle 先判敌人死亡，再判玩家死亡；同时死亡沿用 win。
                if (enemy.IsDead)
                {
                    GameplaySession.RecordResult("combat", "win");
                    LuaManagerPatch.Log?.LogInfo("Combat 离场结果已在场景边界确认为 win");
                    return true;
                }
                if (player.IsDead)
                {
                    GameplaySession.RecordResult("combat", "lose");
                    LuaManagerPatch.Log?.LogInfo("Combat 离场结果已在场景边界确认为 lose");
                    return true;
                }
                throw new InvalidOperationException("敌我均未死亡，拒绝无结果离开 Combat");
            }
            catch (Exception ex)
            {
                LuaManagerPatch.AbortActivePlayback(
                    "Combat 在没有可验证胜负时请求离场",
                    null, ex, "combat_lifecycle");
                return false;
            }
        }
    }
}
