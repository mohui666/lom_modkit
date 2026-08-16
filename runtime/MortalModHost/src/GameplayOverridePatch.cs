using System;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Battle;
using Mortal.Combat;
using Mortal.Core;
using UnityEngine;
using UnityObject = UnityEngine.Object;

namespace MortalModHost
{
    internal static class GameplayOverrideFailure
    {
        internal static void Abort(string surface, Exception error)
        {
            LuaManagerPatch.AbortActivePlayback(
                surface + "配置无法安全应用", null, error, "gameplay_override");
        }
    }

    /// <summary>
    /// 高层 Combat 只克隆并覆盖当前决斗的 CombatStat，不修改原版共享资产。
    /// 角色外观、动画与基础招式仍来自作者选择的原版 CL_ 模板。
    /// </summary>
    [HarmonyPatch(typeof(CombatActionController), "SetStat",
        new Type[] { typeof(CombatStat) })]
    internal static class CombatStatOverridePatch
    {
        private static CombatStat _lastClone;

        private static void Prefix(CombatActionController __instance, ref CombatStat data)
        {
            if (!GameplaySession.PendingCombat || data == null) return;
            try
            {
                CombatManager manager = CombatManager.Instance;
                if (manager == null) throw new InvalidOperationException("CombatManager.Instance 为 null");
                CombatActionController enemy = Traverse.Create(manager)
                    .Field("_enemyAction").GetValue<CombatActionController>();
                if (!ReferenceEquals(__instance, enemy)) return;

                if (_lastClone != null) UnityObject.Destroy(_lastClone);
                CombatStat clone = UnityObject.Instantiate(data);
                clone.name = data.name + "__MortalModHost";
                ApplyInt("max_health", 1, 10000000, delegate(int v) { clone.MaxHealth = v; });
                ApplyInt("health", 0, 10000000, delegate(int v) { clone.DefaultHealth = v; });
                ApplyInt("max_stamina", 0, 100000, delegate(int v) { clone.MaxStamina = v; });
                ApplyInt("stamina", 0, 100000, delegate(int v) { clone.DefaultStamina = v; });
                ApplyInt("strength", 0, 10000, delegate(int v) { clone.Strength = v; });
                ApplyInt("internal", 0, 10000, delegate(int v) { clone.Internal = v; });
                ApplyInt("dexterity", 0, 10000, delegate(int v) { clone.Dexterity = v; });
                ApplyInt("talking", 0, 10000, delegate(int v) { clone.Talking = v; });
                ApplyInt("defence", 0, 10000, delegate(int v) { clone.Defence = v; });
                ApplyInt("sword", 0, 10000, delegate(int v) { clone.Sword = v; });
                ApplyInt("fist", 0, 10000, delegate(int v) { clone.Fist = v; });
                ApplyInt("martial_weapon", 0, 10000, delegate(int v) { clone.MartialWeapon = v; });
                ApplyInt("mental", 0, 10000, delegate(int v) { clone.Mental = v; });
                ApplyFloat("talk_rate", delegate(float v) { clone.TalkRate = v; });
                ApplyFloat("attack_rate", delegate(float v) { clone.AttackRate = v; });
                ApplyFloat("weapon_rate", delegate(float v) { clone.WeaponkRate = v; });
                ApplyFloat("ultimate_rate", delegate(float v) { clone.UltimateRate = v; });
                ApplyFloat("block_rate", delegate(float v) { clone.BlockRate = v; });
                ApplyString("ultimate_one", delegate(string v) { clone.UltimateOne = v; });
                ApplyString("ultimate_two", delegate(string v) { clone.UltimateTwo = v; });
                ApplyString("ultimate_three", delegate(string v) { clone.UltimateThree = v; });
                ApplyTalents(clone);

                if (clone.MaxHealth > 0)
                    clone.DefaultHealth = Mathf.Clamp(clone.DefaultHealth, 0, clone.MaxHealth);
                if (clone.MaxStamina > 0)
                    clone.DefaultStamina = Mathf.Clamp(clone.DefaultStamina, 0, clone.MaxStamina);
                clone.Reset();
                _lastClone = clone;
                data = clone;
            }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("决斗对手", ex);
            }
        }

        private static void ApplyInt(string key, int min, int max, Action<int> setter)
        {
            int value;
            if (GameplaySession.TryConfigInt(key, min, max, out value)) setter(value);
        }

        private static void ApplyFloat(string key, Action<float> setter)
        {
            float value;
            if (GameplaySession.TryConfigFloat(key, 0f, 1f, out value)) setter(value);
        }

        private static void ApplyString(string key, Action<string> setter)
        {
            string value = GameplaySession.ConfigString(key);
            if (!string.IsNullOrEmpty(value)) setter(value);
        }

        private static void ApplyTalents(CombatStat clone)
        {
            if (!GameplaySession.HasConfig("talents")) return;
            List<CombatTalentItem> items = new List<CombatTalentItem>();
            string encoded = GameplaySession.ConfigString("talents");
            if (!string.IsNullOrEmpty(encoded))
            {
                foreach (string row in encoded.Split(','))
                {
                    string[] columns = row.Split(':');
                    int level;
                    if (columns.Length != 2 || !int.TryParse(columns[1], out level)
                        || level < 0 || level > 999)
                        throw new InvalidOperationException("Combat talent 配置格式错误");
                    PlayerTalentData talent = PlayerStatManagerData.Instance.Talents.Get(columns[0]);
                    if (talent == null || !talent.CombatSkill)
                        throw new InvalidOperationException("不是有效的原版决斗技能：" + columns[0]);
                    items.Add(new CombatTalentItem { Data = talent, Level = level });
                }
            }
            clone.TalentItems = items;
        }
    }

    internal static class BattleOverrideResolver
    {
        internal static BattleLevel ResolveLevel(string key)
        {
            if (string.IsNullOrEmpty(key) || GameLevelManager.Instance == null) return null;
            BattleLevelConfig config = Traverse.Create(GameLevelManager.Instance)
                .Field("_levelConfig").GetValue<BattleLevelConfig>();
            return config == null ? null : config.Get("BL_" + key);
        }

        internal static GameObject ResolveSpawner(string configKey, string fieldName)
        {
            if (!GameplaySession.HasConfig(configKey)) return null;
            BattleLevel source = ResolveLevel(GameplaySession.ConfigString(configKey));
            if (source == null)
            {
                GameplayOverrideFailure.Abort(
                    "战役阵容", new InvalidOperationException("找不到原版 Battle 模板：" + GameplaySession.ConfigString(configKey)));
                return null;
            }
            GameObject result = Traverse.Create(source).Field(fieldName).GetValue<GameObject>();
            if (result == null)
                GameplayOverrideFailure.Abort(
                    "战役阵容", new InvalidOperationException("原版 Battle 模板缺少阵容 Prefab：" + configKey));
            return result;
        }

        internal static bool TryCount(string key, out int value)
        {
            value = 0;
            if (!GameplaySession.PendingBattle) return false;
            try { return GameplaySession.TryConfigInt(key, 0, 10000, out value); }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("战役人数", ex);
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "GetFriendPeople")]
    internal static class BattleFriendPeoplePatch
    {
        private static bool Prefix(ref int __result)
        {
            int value;
            if (!BattleOverrideResolver.TryCount("friend_people", out value)) return true;
            __result = value;
            return false;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "GetEnemyPeople")]
    internal static class BattleEnemyPeoplePatch
    {
        private static bool Prefix(ref int __result)
        {
            int value;
            if (!BattleOverrideResolver.TryCount("enemy_people", out value)) return true;
            __result = value;
            return false;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_NeutralPeople")]
    internal static class BattleNeutralPeoplePatch
    {
        private static void Postfix(ref int __result)
        {
            int value;
            if (BattleOverrideResolver.TryCount("neutral_people", out value)) __result = value;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_FriendSpawnerPrefab")]
    internal static class BattleFriendRosterPatch
    {
        private static void Postfix(ref GameObject __result)
        {
            if (!GameplaySession.PendingBattle) return;
            GameObject replacement = BattleOverrideResolver.ResolveSpawner(
                "friend_roster", "_friendSpawnerPrefab");
            if (replacement != null) __result = replacement;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_EnemySpawnerPrefab")]
    internal static class BattleEnemyRosterPatch
    {
        private static void Postfix(ref GameObject __result)
        {
            if (!GameplaySession.PendingBattle) return;
            GameObject replacement = BattleOverrideResolver.ResolveSpawner(
                "enemy_roster", "_enemySpawnerPrefab");
            if (replacement != null) __result = replacement;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_NeutralSpawnerPrefab")]
    internal static class BattleNeutralRosterPatch
    {
        private static void Postfix(ref GameObject __result)
        {
            if (!GameplaySession.PendingBattle) return;
            GameObject replacement = BattleOverrideResolver.ResolveSpawner(
                "neutral_roster", "_neutralSpawnerPrefab");
            if (replacement != null) __result = replacement;
        }
    }

    /// <summary>按阵营给每个已实例化 NPC 增加临时生命修正，不写共享 HealthData。</summary>
    [HarmonyPatch(typeof(CharacterHealth), "Start")]
    internal static class BattleNpcHealthPatch
    {
        private static void Postfix(CharacterHealth __instance)
        {
            if (!GameplaySession.PendingBattle || GameLevelManager.Instance == null) return;
            try
            {
                int target;
                string side = FindSide(__instance.transform);
                if (side.Length == 0
                    || !GameplaySession.TryConfigInt(side + "_health", 1, 10000000, out target))
                    return;
                HealthData data = Traverse.Create(__instance).Field("_defaultHealth")
                    .GetValue<HealthData>();
                if (data == null) throw new InvalidOperationException("CharacterHealth 缺少默认 HealthData");
                Traverse.Create(__instance).Field("_defaultAddHealth")
                    .SetValue(target - data.Health);
                Traverse.Create(__instance).Property("CurrentHealth").SetValue(target);
            }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("战役 NPC 血量", ex);
            }
        }

        private static string FindSide(Transform transform)
        {
            Traverse manager = Traverse.Create(GameLevelManager.Instance);
            if (Inside(manager.Field("_friendSpawner").GetValue<NpcSpawner>(), transform))
                return "friend";
            if (Inside(manager.Field("_enemySpawner").GetValue<NpcSpawner>(), transform))
                return "enemy";
            if (Inside(manager.Field("_neutralSpawner").GetValue<NpcSpawner>(), transform))
                return "neutral";
            return "";
        }

        private static bool Inside(NpcSpawner spawner, Transform target)
        {
            if (spawner == null) return false;
            CharacterSpawnPoint[] points = Traverse.Create(spawner)
                .Field("_spawnPoints").GetValue<CharacterSpawnPoint[]>();
            if (points == null) return false;
            foreach (CharacterSpawnPoint point in points)
            {
                if (point != null && target.IsChildOf(point.transform)) return true;
            }
            return false;
        }
    }
}
