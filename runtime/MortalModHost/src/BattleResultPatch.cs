using HarmonyLib;
using Mortal.Battle;

namespace MortalModHost
{
    /// <summary>
    /// 只消费原版明确标为 finish=true 的 FriendWin / EnemyWin。
    /// PlayerDie(false) 保持原版重试/标题流程，不伪造为可续接结果。
    /// </summary>
    [HarmonyPatch(typeof(GameLevelManager), "ShowGameOver",
        new System.Type[] { typeof(GameOverType), typeof(bool) })]
    internal static class BattleResultPatch
    {
        private static void Prefix(GameOverType type, bool finish)
        {
            if (!finish || !GameplaySession.PendingBattle) return;
            if (type == GameOverType.FriendWin)
                GameplaySession.RecordResult("battle", "win");
            else if (type == GameOverType.EnemyWin)
                GameplaySession.RecordResult("battle", "lose");
        }
    }
}
