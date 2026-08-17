using System;
using System.Collections;
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
    /// 高层 Combat 克隆当前决斗的 CombatStat，再覆盖作者填写的数值。人物选择只
    /// 决定经过四路径一致性验证的 CombatEnemyAvatar，不带入人物预设属性。
    /// </summary>
    [HarmonyPatch(typeof(CombatActionController), "SetStat",
        new Type[] { typeof(CombatStat) })]
    internal static class CombatStatOverridePatch
    {
        private static CombatStat _lastClone;

        internal static bool Owns(CombatStat data)
        {
            // UnityEngine.Object 可能为同一 native 对象生成不同托管 wrapper；
            // CLR ReferenceEquals 会把它们误判为不同。克隆的 Unity InstanceID
            // 在本场唯一，正好用于识别敌方 Stat 而不依赖 wrapper 身份。
            return data != null && _lastClone != null
                && data.GetInstanceID() == _lastClone.GetInstanceID();
        }

        internal static void Clear()
        {
            if (_lastClone != null) UnityObject.Destroy(_lastClone);
            _lastClone = null;
        }

        private static void Prefix(CombatActionController __instance, ref CombatStat data)
        {
            if (!GameplaySession.PendingCombat || data == null) return;
            try
            {
                CombatManager manager = CombatManager.Instance;
                if (manager == null) throw new InvalidOperationException("CombatManager.Instance 为 null");
                CombatLevel activeLevel = Traverse.Create(manager)
                    .Field("_combatLevel").GetValue<CombatLevel>();
                // 原版 LoadCombatLevelAsset 的实证顺序是：
                // _combatLevel = level; _enemyAction.SetStat(_combatLevel.EnemyStat)。
                // 不再依赖 _enemyAction 包装对象的引用身份，它在这个时点
                // 与后续字段可以不是同一个托管 wrapper。
                if (activeLevel == null || data != activeLevel.EnemyStat) return;

                Clear();
                CombatStat clone = UnityObject.Instantiate(data);
                clone.name = data.name + "__MortalModHost";
                string character = GameplaySession.ConfigString("character");
                if (!CombatCharacterPolicy.IsUserCharacter(character))
                {
                    // CombatLevel.EnemyStat.Name 是原版人物与 Combat 资源之间的真实
                    // 主键。资源目录包含中文描述和补零，不能再从路径猜人物 ID。
                    CombatStat identity = BattleOverrideResolver.TryResolveOfficialCombatStat(character);
                    if (identity != null)
                    {
                        clone.Name = identity.Name;
                        clone.CombatAvatar = identity.CombatAvatar;
                        clone.AvatarAddressKey = identity.AvatarAddressKey;
                        clone.HeadImage = identity.HeadImage;
                        clone.CharacterPrefab = identity.CharacterPrefab;
                    }
                }
                ApplyInt("max_health", 1, 10000000, delegate(int v) { clone.MaxHealth = v; });
                ApplyInt("health", 0, 10000000, delegate(int v) { clone.DefaultHealth = v; });
                ApplyInt("max_stamina", 0, 100000, delegate(int v) { clone.MaxStamina = v; });
                ApplyInt("stamina", 0, 100000, delegate(int v) { clone.DefaultStamina = v; });
                ApplyInt("strength", 0, 10000, delegate(int v) { clone.Strength = v; });
                ApplyInt("stamina_power", 0, 10000, delegate(int v) { clone.Stamina = v; });
                ApplyInt("internal", 0, 10000, delegate(int v) { clone.Internal = v; });
                ApplyInt("dexterity", 0, 10000, delegate(int v) { clone.Dexterity = v; });
                ApplyInt("talking", 0, 10000, delegate(int v) { clone.Talking = v; });
                ApplyInt("defence", 0, 10000, delegate(int v) { clone.Defence = v; });
                ApplyInt("sword", 0, 10000, delegate(int v) { clone.Sword = v; });
                ApplyInt("fist", 0, 10000, delegate(int v) { clone.Fist = v; });
                ApplyInt("martial_weapon", 0, 10000, delegate(int v) { clone.MartialWeapon = v; });
                ApplyInt("mental", 0, 10000, delegate(int v) { clone.Mental = v; });
                ApplyInt("disposition", 0, 10000, delegate(int v) { clone.Disposition = v; });
                ApplyInt("training", 0, 10000, delegate(int v) { clone.Training = v; });
                ApplyInt("karma", 0, 10000, delegate(int v) { clone.Karma = v; });
                ApplyInt("behaviour", 0, 10000, delegate(int v) { clone.Behaviour = v; });
                ApplyInt("poison_resist", 0, 10000, delegate(int v) { clone.PoisonResist = v; });
                ApplyInt("paralyzed_resist", 0, 10000, delegate(int v) { clone.ParalyzedResist = v; });
                ApplyInt("weapon_poison_value", 0, 10000, delegate(int v) { clone.WeaponPoisonValue = v; });
                ApplyInt("weapon_paralyzed_value", 0, 10000, delegate(int v) { clone.WeaponParalyzedValue = v; });
                ApplyInt("confucianism", 0, 10000, delegate(int v) { clone.Confucianism = v; });
                ApplyInt("buddhism", 0, 10000, delegate(int v) { clone.Buddhism = v; });
                ApplyInt("taoism", 0, 10000, delegate(int v) { clone.Taoism = v; });
                ApplyInt("xingyi", 0, 10000, delegate(int v) { clone.Xingyi = v; });
                ApplyInt("strategy_level", 0, 10, delegate(int v) { clone.StrategyLevel = v; });
                ApplyInt("weapon_hit_addition", 0, 10000, delegate(int v) { clone.WeaponHitAddition = v; });
                ApplyInt("weapon_damage_addition", -100000, 100000, delegate(int v) { clone.WeaponDamageAddition = v; });
                ApplyInt("weapon_dice_addition", -1000, 1000, delegate(int v) { clone.WeaponDiceAddition = v; });
                ApplyInt("attack_damage_addition", -100000, 100000, delegate(int v) { clone.AttackDamageAddition = v; });
                ApplyInt("attack_dice_addition", -1000, 1000, delegate(int v) { clone.AttackDiceAddition = v; });
                ApplyFloatRange("block_dodge_addition", -1f, 1f,
                    delegate(float v) { clone.BlockDodgeAddition = v; });
                ApplyFloatRange("block_parry_addition", -1f, 1f,
                    delegate(float v) { clone.BlockParryAddition = v; });
                ApplyFloatRange("attack_parry_addition", -1f, 1f,
                    delegate(float v) { clone.AttackParryAddition = v; });
                ApplyFloatRange("ultimate_damage_rate", 0f, 100f,
                    delegate(float v) { clone.UltimateDamageRate = v; });
                ApplyInt("defence_addition", -100000, 100000,
                    delegate(int v) { clone.DefenceAddition = v; });
                ApplyFloat("talk_rate", delegate(float v) { clone.TalkRate = v; });
                ApplyFloat("attack_rate", delegate(float v) { clone.AttackRate = v; });
                ApplyFloat("weapon_rate", delegate(float v) { clone.WeaponkRate = v; });
                ApplyFloat("ultimate_rate", delegate(float v) { clone.UltimateRate = v; });
                ApplyFloat("block_rate", delegate(float v) { clone.BlockRate = v; });
                ApplyTalents(clone);

                if (clone.MaxHealth > 0)
                    clone.DefaultHealth = Mathf.Clamp(clone.DefaultHealth, 0, clone.MaxHealth);
                if (clone.MaxStamina > 0)
                    clone.DefaultStamina = Mathf.Clamp(clone.DefaultStamina, 0, clone.MaxStamina);
                clone.Reset();
                _lastClone = clone;
                data = clone;
                LuaManagerPatch.Log?.LogInfo(
                    "Combat 敌方自由配置已应用：character=" + character
                    + "; name=" + clone.Name
                    + "; health=" + clone.DefaultHealth + "/" + clone.MaxHealth
                    + "; stamina=" + clone.DefaultStamina + "/" + clone.MaxStamina
                    + "; strength=" + clone.Strength
                    + "; stamina_power=" + clone.Stamina
                    + "; internal=" + clone.Internal
                    + "; dexterity=" + clone.Dexterity
                    + "; talking=" + clone.Talking
                    + "; defence=" + clone.Defence
                    + "; sword=" + clone.Sword
                    + "; fist=" + clone.Fist
                    + "; martial_weapon=" + clone.MartialWeapon
                    + "; mental=" + clone.Mental
                    + "; disposition=" + clone.Disposition
                    + "; training=" + clone.Training
                    + "; karma=" + clone.Karma
                    + "; behaviour=" + clone.Behaviour
                    + "; poison_resist=" + clone.PoisonResist
                    + "; paralyzed_resist=" + clone.ParalyzedResist
                    + "; weapon_poison_value=" + clone.WeaponPoisonValue
                    + "; weapon_paralyzed_value=" + clone.WeaponParalyzedValue
                    + "; confucianism=" + clone.Confucianism
                    + "; buddhism=" + clone.Buddhism
                    + "; taoism=" + clone.Taoism
                    + "; xingyi=" + clone.Xingyi
                    + "; strategy_level=" + clone.StrategyLevel
                    + "; weapon_hit_addition=" + clone.WeaponHitAddition
                    + "; weapon_damage_addition=" + clone.WeaponDamageAddition
                    + "; weapon_dice_addition=" + clone.WeaponDiceAddition
                    + "; attack_damage_addition=" + clone.AttackDamageAddition
                    + "; attack_dice_addition=" + clone.AttackDiceAddition
                    + "; block_dodge_addition=" + clone.BlockDodgeAddition
                    + "; block_parry_addition=" + clone.BlockParryAddition
                    + "; attack_parry_addition=" + clone.AttackParryAddition
                    + "; ultimate_damage_rate=" + clone.UltimateDamageRate
                    + "; defence_addition=" + clone.DefenceAddition
                    + "; talents=" + (clone.TalentItems == null ? 0 : clone.TalentItems.Count));
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
            ApplyFloatRange(key, 0f, 1f, setter);
        }

        private static void ApplyFloatRange(string key, float min, float max, Action<float> setter)
        {
            float value;
            if (GameplaySession.TryConfigFloat(key, min, max, out value)) setter(value);
        }

        private static void ApplyTalents(CombatStat clone)
        {
            if (!GameplaySession.HasConfig("talents")) return;
            List<CombatTalentItem> items = new List<CombatTalentItem>();
            var seenTalentIds = new HashSet<string>(StringComparer.Ordinal);
            var seenEffectKeys = new HashSet<string>(StringComparer.Ordinal);
            string encoded = GameplaySession.ConfigString("talents");
            if (!string.IsNullOrEmpty(encoded))
            {
                foreach (string row in encoded.Split(','))
                {
                    string[] columns = row.Split(':');
                    int level;
                    if (columns.Length != 2 || !int.TryParse(columns[1], out level))
                        throw new InvalidOperationException("Combat talent 配置格式错误");
                    PlayerTalentData talent = PlayerStatManagerData.Instance.Talents.Get(columns[0]);
                    if (talent == null || !talent.CombatSkill)
                        throw new InvalidOperationException("不是有效的原版决斗技能：" + columns[0]);
                    if (!seenTalentIds.Add(columns[0]))
                        throw new InvalidOperationException("决斗技能不得重复：" + columns[0]);
                    int maxLevel = Traverse.Create(talent).Field("_maxLevel").GetValue<int>();
                    if (maxLevel < 1 || level < 1 || level > maxLevel)
                        throw new InvalidOperationException(
                            "决斗技能等级越界：" + columns[0] + "=" + level
                            + "（允许 1~" + maxLevel + "）");
                    string effectKey = talent.CombatSkillKey;
                    if (string.IsNullOrWhiteSpace(effectKey))
                        throw new InvalidOperationException(
                            "原版决斗技能缺少 CombatSkillKey：" + columns[0]);
                    if (talent.DisplayLevel) effectKey += "_" + level;
                    if (!seenEffectKeys.Add(effectKey))
                        throw new InvalidOperationException(
                            "多个决斗技能解析到相同 EffectDatabase key：" + effectKey);
                    CombatManager manager = CombatManager.Instance;
                    CombatStateEffectScriptable effect = manager != null
                        && manager.EffectDatabase != null
                        ? manager.EffectDatabase.GetByKey(effectKey) : null;
                    if (effect == null)
                        throw new InvalidOperationException(
                            "原版决斗技能没有可执行的 EffectDatabase 项："
                            + columns[0] + " -> " + effectKey);
                    if (effect.Units == null || Array.Exists(effect.Units, unit => unit == null))
                        throw new InvalidOperationException(
                            "原版决斗技能的 EffectDatabase 项不完整："
                            + columns[0] + " -> " + effectKey);
                    items.Add(new CombatTalentItem { Data = talent, Level = level });
                    LuaManagerPatch.Log?.LogInfo(
                        "Combat 技能已验证：" + columns[0] + " Lv" + level
                        + " -> " + effectKey);
                }
            }
            clone.TalentItems = items;
        }
    }

    internal static class BattleOverrideResolver
    {
        internal static CombatStat TryResolveOfficialCombatStat(string character)
        {
            if (string.IsNullOrEmpty(character) || CombatCharacterPolicy.IsUserCharacter(character))
                throw new InvalidOperationException("必须提供官方 Combat 人物 id");
            CombatLevelConfig config = Traverse.Create(CombatManager.Instance)
                .Field("_levelConfig").GetValue<CombatLevelConfig>();
            if (config == null || config.List == null)
                throw new InvalidOperationException("CombatLevelConfig 不可用");
            CombatStat selected = null;
            for (int i = 0; i < config.List.Count; i++)
            {
                CombatLevel level = config.List[i];
                CombatStat candidate = level != null ? level.EnemyStat : null;
                if (candidate == null
                    || !string.Equals(candidate.Name, character, StringComparison.Ordinal)
                    || candidate.CombatAvatar == null)
                    continue;
                if (selected == null) selected = candidate;
                else if (!SameAvatar(selected.CombatAvatar, candidate.CombatAvatar))
                    throw new InvalidOperationException(
                        "官方人物存在多套不同 Combat 动画，无法唯一绑定：" + character);
            }
            return selected;
        }

        internal static BattleLevel ResolveLevel(string key)
        {
            if (string.IsNullOrEmpty(key) || GameLevelManager.Instance == null) return null;
            BattleLevelConfig config = Traverse.Create(GameLevelManager.Instance)
                .Field("_levelConfig").GetValue<BattleLevelConfig>();
            return config == null ? null : config.Get("BL_" + key);
        }

        internal static CombatEnemyAvatar ResolveCombatAvatar(CombatEnemyAvatar fallback)
        {
            string character = GameplaySession.ConfigString("character");
            if (string.IsNullOrEmpty(character))
                throw new InvalidOperationException("Combat v3 缺少 character");
            if (CombatCharacterPolicy.IsUserCharacter(character)) return fallback;
            CombatStat source = TryResolveOfficialCombatStat(character);
            if (source == null) return null;
            CombatEnemyAvatar selected = source.CombatAvatar;
            LuaManagerPatch.Log?.LogInfo(
                "Combat 官方动画已按 EnemyStat.Name 绑定为同一人物 " + character
                + "：idle=" + selected.NormalKey
                + "；attack=" + selected.AttackKey
                + "；hurt=" + selected.HurtKey
                + "；defence=" + selected.DefenceKey);
            return selected;
        }

        private static bool SameAvatar(CombatEnemyAvatar left, CombatEnemyAvatar right)
        {
            return ReferenceEquals(left, right) || (left != null && right != null
                && string.Equals(left.NormalKey, right.NormalKey, StringComparison.Ordinal)
                && string.Equals(left.AttackKey, right.AttackKey, StringComparison.Ordinal)
                && string.Equals(left.HurtKey, right.HurtKey, StringComparison.Ordinal)
                && string.Equals(left.DefenceKey, right.DefenceKey, StringComparison.Ordinal));
        }

        internal static GameObject ResolveFactionSpawner(string side, string fieldName)
        {
            if (!GameplaySession.PendingBattle) return null;
            string faction = GameplaySession.ConfigString(side + "_faction");
            int total;
            if (!GameplaySession.TryConfigInt(side + "_people", 0, 10000, out total)) total = 0;
            List<string> namedCharacters = BattleCompositionPolicy.ParseCharacters(
                GameplaySession.ConfigString(side + "_characters"), total);
            if (namedCharacters.Count > 0 && !BattleNamedCharacterPatch.Ready)
                throw new InvalidOperationException(
                    "具名角色补丁不可用；为避免悄悄生成错误阵容，本场战役已取消");
            if (string.IsNullOrEmpty(faction))
            {
                if (total == 0) return null;
                throw new InvalidOperationException(side + "_faction 在人数非零时不能为空");
            }
            BattleLevelConfig config = Traverse.Create(GameLevelManager.Instance)
                .Field("_levelConfig").GetValue<BattleLevelConfig>();
            if (config == null || config.List == null)
                throw new InvalidOperationException("BattleLevelConfig 不可用");
            for (int i = 0; i < config.List.Count; i++)
            {
                BattleLevel level = config.List[i];
                if (level == null || !string.Equals(level.NameKey, faction, StringComparison.Ordinal)) continue;
                GameObject prefab = Traverse.Create(level).Field(fieldName).GetValue<GameObject>();
                if (prefab == null)
                    throw new InvalidOperationException("战役阵营缺少可用 NPC 生成器：" + faction);
                return prefab;
            }
            throw new InvalidOperationException("找不到战役阵营：" + faction);
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
            // story_schema=2 只公开友/敌双方；内部 BL_0000 若自带中立人数必须清零。
            if (GameplaySession.PendingBattle) __result = 0;
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_FriendSpawnerPrefab")]
    internal static class BattleFriendRosterPatch
    {
        private static void Postfix(ref GameObject __result)
        {
            if (!GameplaySession.PendingBattle) return;
            try
            {
                GameObject replacement = BattleOverrideResolver.ResolveFactionSpawner(
                    "friend", "_enemySpawnerPrefab");
                if (replacement != null) __result = replacement;
            }
            catch (Exception ex) { GameplayOverrideFailure.Abort("我方战役阵营", ex); }
        }
    }

    [HarmonyPatch(typeof(BattleLevel), "get_EnemySpawnerPrefab")]
    internal static class BattleEnemyRosterPatch
    {
        private static void Postfix(ref GameObject __result)
        {
            if (!GameplaySession.PendingBattle) return;
            try
            {
                GameObject replacement = BattleOverrideResolver.ResolveFactionSpawner(
                    "enemy", "_enemySpawnerPrefab");
                if (replacement != null) __result = replacement;
            }
            catch (Exception ex) { GameplayOverrideFailure.Abort("敌方战役阵营", ex); }
        }
    }

    /// <summary>
    /// CombatEnemyController.SetData 是原版真正读取敌方四帧地址的唯一入口。
    /// 官方人物在这里临时换入独立 CombatEnemyAvatar，避免早于 Setup 的
    /// SetStat 补丁受敌人实例时序影响，也不修改原版 ScriptableObject。
    /// 自定义人物仍在原版协程完成后替换渲染器；离场立即销毁。
    /// </summary>
    [HarmonyPatch(typeof(CombatEnemyController), "SetData")]
    internal static class CombatCustomAvatarPatch
    {
        private static readonly System.Reflection.FieldInfo CombatStatField =
            AccessTools.Field(typeof(CombatCharacterController), "_combatStat");
        private static readonly List<Sprite> Sprites = new List<Sprite>();
        private static readonly List<Texture2D> Textures = new List<Texture2D>();

        private static void Postfix(CombatEnemyController __instance, ref IEnumerator __result)
        {
            if (!GameplaySession.PendingCombat || __instance == null) return;
            string character = GameplaySession.ConfigString("character");
            __result = Wrap(__instance, __result, character);
        }

        private static IEnumerator Wrap(
            CombatEnemyController controller, IEnumerator original, string raw)
        {
            if (CombatCharacterPolicy.IsUserCharacter(raw))
            {
                while (original != null && original.MoveNext()) yield return original.Current;
                try
                {
                    RendererLayout[] layout = CaptureRendererLayout(controller);
                    Apply(controller, raw);
                    RestoreRendererLayout(layout);
                }
                catch (Exception ex)
                {
                    LuaManagerPatch.Log?.LogError(
                        "自定义决斗动画覆盖失败；保留原版决斗并继续演出：" + ex);
                }
                yield break;
            }

            CombatStat stat = null;
            CombatEnemyAvatar originalAvatar = null;
            CombatEnemyAvatar isolatedAvatar = null;
            bool keepOriginalAvatar = false;
            bool usingPortraitFallback = false;
            RendererLayout[] officialLayout = null;
            try
            {
                try
                {
                    stat = CombatStatField != null
                        ? CombatStatField.GetValue(controller) as CombatStat : null;
                    if (!CombatStatOverridePatch.Owns(stat))
                        throw new InvalidOperationException(
                            "CombatEnemyController.SetData 的 _combatStat 不是本场已克隆的敌方 Stat");
                    originalAvatar = stat.CombatAvatar;
                    CombatEnemyAvatar verified = BattleOverrideResolver.ResolveCombatAvatar(originalAvatar);
                    if (verified != null)
                    {
                        isolatedAvatar = UnityObject.Instantiate(verified);
                        isolatedAvatar.name = verified.name + "__MortalModHost";
                    }
                    else
                    {
                        string idleAddress = GameplaySession.CombatIdleAddress;
                        if (string.IsNullOrEmpty(idleAddress))
                        {
                            keepOriginalAvatar = true;
                            LuaManagerPatch.Log?.LogError(
                                "所选官方人物没有专用 Combat 四帧或 normal 立绘；"
                                + "保留原版决斗壳并继续演出：" + raw);
                        }
                        else
                        {
                            isolatedAvatar = ScriptableObject.CreateInstance<CombatEnemyAvatar>();
                            isolatedAvatar.name = raw + "__MortalModHostPortraitFallback";
                            isolatedAvatar.NormalKey = idleAddress;
                            isolatedAvatar.AttackKey = idleAddress;
                            isolatedAvatar.HurtKey = idleAddress;
                            isolatedAvatar.DefenceKey = idleAddress;
                            usingPortraitFallback = true;
                            LuaManagerPatch.Log?.LogInfo(
                                "官方人物 " + raw + " 没有专用 Combat 四帧；四种状态按规则回退到该人物 normal 立绘："
                                + idleAddress);
                        }
                    }
                    if (!keepOriginalAvatar) stat.CombatAvatar = isolatedAvatar;
                    officialLayout = CaptureRendererLayout(controller);
                    // 原版 prefab 在 Addressables 失败时会保留序列化的旧 Sprite，
                    // 这正是“待机南宫深、攻击瑞笙”混搭能够悄然出现的条件。
                    // 先清空后再让原版载入，任一帧失败都会显式拒绝，
                    // 不再展示壳上的其他人物。
                    if (!keepOriginalAvatar) ClearOfficialRenderers(controller);
                }
                catch (Exception ex)
                {
                    keepOriginalAvatar = true;
                    LuaManagerPatch.Log?.LogError(
                        "官方决斗动画覆盖失败；保留原版决斗并继续演出：" + ex);
                }

                while (original != null)
                {
                    bool moved;
                    object current = null;
                    try
                    {
                        moved = original.MoveNext();
                        if (moved) current = original.Current;
                    }
                    catch (Exception ex)
                    {
                        LuaManagerPatch.Log?.LogError(
                            "原版决斗动画协程失败；不再终止整段 MOD 演出：" + ex);
                        yield break;
                    }
                    if (!moved) break;
                    yield return current;
                }
                if (!keepOriginalAvatar && isolatedAvatar != null)
                {
                    try
                    {
                        VerifyOfficialRenderers(controller, isolatedAvatar, raw);
                        // 原版专用 Combat 帧已经包含正确的画布/pivot；只有 Story
                        // normal 静态回退才需要适配 Combat prefab 的占位空间。
                        if (usingPortraitFallback) RestoreRendererLayout(officialLayout);
                    }
                    catch (Exception ex)
                    {
                        LuaManagerPatch.Log?.LogError(
                            "官方决斗动画校验失败；不再终止整段 MOD 演出：" + ex);
                    }
                }
            }
            finally
            {
                if (stat != null) stat.CombatAvatar = originalAvatar;
                if (isolatedAvatar != null) UnityObject.Destroy(isolatedAvatar);
            }
        }

        private static void ClearOfficialRenderers(CombatEnemyController controller)
        {
            Traverse owner = Traverse.Create(controller);
            Set(owner.Field("_idleSprite").GetValue<SpriteRenderer>(), null);
            Set(owner.Field("_chargeSprite").GetValue<SpriteRenderer>(), null);
            Set(owner.Field("_attackSprite").GetValue<SpriteRenderer>(), null);
            Set(owner.Field("_hurtSprite").GetValue<SpriteRenderer>(), null);
            Set(owner.Field("_defenceSprite").GetValue<SpriteRenderer>(), null);
        }

        /// <summary>
        /// Story 立绘与原生 Combat 帧的像素画布、pivot 和 PPU 并不相同。仅替换
        /// Sprite 会把半身立绘按原尺寸铺满屏幕。这里先记录原版 prefab 为每个状态
        /// 预留的世界空间，再让替换图等比缩放，并按水平中心/底边对齐回该空间。
        /// </summary>
        private sealed class RendererLayout
        {
            internal SpriteRenderer Renderer;
            internal Bounds TargetBounds;
            internal bool Valid;
        }

        private static RendererLayout[] CaptureRendererLayout(CombatEnemyController controller)
        {
            Traverse owner = Traverse.Create(controller);
            string[] fields = { "_idleSprite", "_chargeSprite", "_attackSprite", "_hurtSprite", "_defenceSprite" };
            var result = new RendererLayout[fields.Length];
            for (int i = 0; i < fields.Length; i++)
            {
                SpriteRenderer renderer = owner.Field(fields[i]).GetValue<SpriteRenderer>();
                Bounds bounds = renderer != null ? renderer.bounds : default(Bounds);
                result[i] = new RendererLayout
                {
                    Renderer = renderer,
                    TargetBounds = bounds,
                    Valid = renderer != null && renderer.sprite != null
                        && bounds.size.x > 0.001f && bounds.size.y > 0.001f
                };
            }
            return result;
        }

        private static void RestoreRendererLayout(RendererLayout[] layouts)
        {
            if (layouts == null) return;
            for (int i = 0; i < layouts.Length; i++)
            {
                RendererLayout layout = layouts[i];
                SpriteRenderer renderer = layout != null ? layout.Renderer : null;
                if (renderer == null || renderer.sprite == null || !layout.Valid) continue;
                Bounds current = renderer.bounds;
                if (current.size.x <= 0.001f || current.size.y <= 0.001f) continue;
                float scale = Mathf.Min(
                    layout.TargetBounds.size.x / current.size.x,
                    layout.TargetBounds.size.y / current.size.y);
                if (float.IsNaN(scale) || float.IsInfinity(scale) || scale <= 0f) continue;
                renderer.transform.localScale *= scale;
                current = renderer.bounds;
                Vector3 offset = new Vector3(
                    layout.TargetBounds.center.x - current.center.x,
                    layout.TargetBounds.min.y - current.min.y,
                    0f);
                renderer.transform.position += offset;
            }
        }

        private static void VerifyOfficialRenderers(
            CombatEnemyController controller, CombatEnemyAvatar avatar, string character)
        {
            Traverse owner = Traverse.Create(controller);
            VerifyRenderer(owner, "_idleSprite", avatar.NormalKey, "idle");
            VerifyRenderer(owner, "_attackSprite", avatar.AttackKey, "attack");
            VerifyRenderer(owner, "_hurtSprite", avatar.HurtKey, "hurt");
            VerifyRenderer(owner, "_defenceSprite", avatar.DefenceKey, "defence");
            LuaManagerPatch.Log?.LogInfo(
                "Combat 敌方四帧已在 SetData 绑定同一人物 " + character
                + "：idle=" + avatar.NormalKey
                + "；attack=" + avatar.AttackKey
                + "；hurt=" + avatar.HurtKey
                + "；defence=" + avatar.DefenceKey);
        }

        private static void VerifyRenderer(
            Traverse owner, string field, string addressKey, string logicalName)
        {
            SpriteRenderer renderer = owner.Field(field).GetValue<SpriteRenderer>();
            if (renderer == null || renderer.sprite == null)
                throw new InvalidOperationException(
                    "官方决斗动画未载入 " + logicalName + "：" + addressKey);
        }

        private static void Apply(CombatEnemyController controller, string raw)
        {
            Clear();
            ContentRef parsed;
            string error;
            ModPackage package = ModOverlay.CurrentPackage;
            UserContent content;
            if (!ContentRef.TryParse(raw, out parsed, out error) || package == null
                || !package.TryGetUserContent(parsed.ContentId, out content)
                || content == null || content.Type != "character")
                throw new InvalidOperationException("找不到当前包内自定义决斗人物：" + raw);
            string normal = null;
            if (content.Portraits != null) content.Portraits.TryGetValue("normal", out normal);
            Dictionary<string, string> frames = CombatCharacterPolicy.ResolveFrames(
                content.CombatFrames, normal);
            Sprite idle = Load(content, frames["idle"]);
            Sprite attack = Load(content, frames["attack"]);
            Sprite hurt = Load(content, frames["hurt"]);
            Sprite defence = Load(content, frames["defence"]);
            Traverse owner = Traverse.Create(controller);
            Set(owner.Field("_idleSprite").GetValue<SpriteRenderer>(), idle);
            Set(owner.Field("_chargeSprite").GetValue<SpriteRenderer>(), idle);
            Set(owner.Field("_attackSprite").GetValue<SpriteRenderer>(), attack);
            Set(owner.Field("_hurtSprite").GetValue<SpriteRenderer>(), hurt);
            Set(owner.Field("_defenceSprite").GetValue<SpriteRenderer>(), defence);
        }

        private static Sprite Load(UserContent content, string file)
        {
            byte[] bytes;
            if (content.Files == null || !content.Files.TryGetValue(file, out bytes)
                || bytes == null || bytes.Length == 0)
                throw new InvalidOperationException("自定义决斗动画文件不存在：" + file);
            Texture2D texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
            if (!texture.LoadImage(bytes))
            {
                UnityObject.Destroy(texture);
                throw new InvalidOperationException("自定义决斗动画图片解码失败：" + file);
            }
            texture.wrapMode = TextureWrapMode.Clamp;
            texture.filterMode = FilterMode.Bilinear;
            Sprite sprite = Sprite.Create(texture,
                new Rect(0f, 0f, texture.width, texture.height), new Vector2(0.5f, 0f), 100f);
            Textures.Add(texture);
            Sprites.Add(sprite);
            return sprite;
        }

        private static void Set(SpriteRenderer renderer, Sprite sprite)
        {
            if (renderer == null) throw new InvalidOperationException("CombatEnemyController 动画渲染器缺失");
            renderer.sprite = sprite;
        }

        internal static void Clear()
        {
            for (int i = 0; i < Sprites.Count; i++) if (Sprites[i] != null) UnityObject.Destroy(Sprites[i]);
            for (int i = 0; i < Textures.Count; i++) if (Textures[i] != null) UnityObject.Destroy(Textures[i]);
            Sprites.Clear();
            Textures.Clear();
        }
    }

    /// <summary>
    /// 人物选择只决定动画，不能让固定场景壳“唐升”的姓名泄漏到 MOD 对手。
    /// 名称与动画使用同一个已冻结人物身份；不得由作者另填，也不得泄漏固定壳姓名。
    /// </summary>
    [HarmonyPatch(typeof(CombatStatController), "get_CharacterName")]
    internal static class CombatDisplayNamePatch
    {
        private static void Postfix(CombatStatController __instance, ref string __result)
        {
            if (!GameplaySession.PendingCombat || __instance == null
                || !CombatStatOverridePatch.Owns(__instance.Data)) return;
            string displayName = GameplaySession.CombatDisplayName;
            if (!string.IsNullOrWhiteSpace(displayName)) __result = displayName;
        }
    }

    /// <summary>
    /// 顶部血条的名字并不调用 CombatStatController.CharacterName，而是
    /// CombatStatUI.Setup 直接再次解析 CombatStat.GetNameKey()。因此必须在
    /// UI 完成原版 Setup 后覆写实际 Text，才能避免固定 CL 壳的“唐升”泄漏。
    /// </summary>
    [HarmonyPatch(typeof(CombatStatUI), "Setup")]
    internal static class CombatDisplayNameUiPatch
    {
        private static void Postfix(CombatStatUI __instance)
        {
            if (!GameplaySession.PendingCombat || __instance == null) return;
            Traverse fields = Traverse.Create(__instance);
            CombatStat stat = fields.Field("_statData").GetValue<CombatStat>();
            if (!CombatStatOverridePatch.Owns(stat)) return;
            UnityEngine.UI.Text nameText = fields.Field("_nameText").GetValue<UnityEngine.UI.Text>();
            string displayName = GameplaySession.CombatDisplayName;
            if (nameText != null && !string.IsNullOrWhiteSpace(displayName))
            {
                nameText.text = displayName;
                LuaManagerPatch.Log?.LogInfo("Combat 顶部姓名已绑定为所选人物：" + displayName);
            }
        }
    }

    [HarmonyPatch(typeof(CombatManager), "OnDisable")]
    internal static class CombatRuntimeCleanupPatch
    {
        private static void Postfix()
        {
            CombatBackgroundOverridePatch.Clear();
            CombatRuntimeLevelPatch.Clear();
            CombatCustomAvatarPatch.Clear();
            CombatStatOverridePatch.Clear();
        }
    }

    /// <summary>
    /// 原版 InitNpcList 会把 _specialNPCs 额外压入队列。v3 必须让具名角色计入
    /// total，因此先禁用额外队列，再在生成后的固定长度队列中替换前 N 个普通席位。
    /// </summary>
    internal static class BattleNamedCharacterPatch
    {
        internal static bool Ready { get; set; }

        private static void Prefix(
            NpcSpawner __instance, GameObject spawnerObject, out string __state)
        {
            __state = null;
            if (!GameplaySession.PendingBattle || __instance == null) return;
            string side = FindSide(spawnerObject);
            if (side == null) return;
            try
            {
                int total;
                if (!GameplaySession.TryConfigInt(side + "_people", 0, 10000, out total)) total = 0;
                List<string> ids = BattleCompositionPolicy.ParseCharacters(
                    GameplaySession.ConfigString(side + "_characters"), total);
                // 原版 specialNPC 始终是额外席位；v3 所有席位都必须包含在 total 内。
                Traverse.Create(__instance).Field("_specialNPCs").SetValue(new NpcSpawnPreset[0]);
                __state = side;
            }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("战役具名角色", ex);
            }
        }

        private static string FindSide(GameObject spawnPoints)
        {
            if (spawnPoints == null || GameLevelManager.Instance == null) return null;
            Transform parent = spawnPoints.transform.parent;
            Traverse manager = Traverse.Create(GameLevelManager.Instance);
            if (ReferenceEquals(parent, manager.Field("_friendSpawnerPosition").GetValue<Transform>())) return "friend";
            if (ReferenceEquals(parent, manager.Field("_enemySpawnerPosition").GetValue<Transform>())) return "enemy";
            if (ReferenceEquals(parent, manager.Field("_neutralSpawnerPosition").GetValue<Transform>())) return "neutral";
            return null;
        }

        private static void Postfix(NpcSpawner __instance, string __state)
        {
            if (string.IsNullOrEmpty(__state) || __instance == null) return;
            try
            {
                int total;
                GameplaySession.TryConfigInt(__state + "_people", 0, 10000, out total);
                List<string> ids = BattleCompositionPolicy.ParseCharacters(
                    GameplaySession.ConfigString(__state + "_characters"), total);
                Queue<NpcSpawnPreset> queue = Traverse.Create(__instance)
                    .Field("_npcQueue").GetValue<Queue<NpcSpawnPreset>>();
                if (queue == null || queue.Count != total)
                    throw new InvalidOperationException("原版 NPC 队列人数与声明总人数不一致");
                if (ids.Count == 0) return;
                var replacements = new List<NpcSpawnPreset>();
                for (int i = 0; i < ids.Count; i++)
                {
                    NpcSpawnPreset replacement = TryResolveNamed(ids[i]);
                    replacements.Add(replacement);
                    if (replacement == null)
                        LuaManagerPatch.Log?.LogWarning(
                            "官方 BattleLevelConfig 没有可生成的 NPC preset：" + ids[i]
                            + "；该具名席位保留所选阵营的普通 NPC，战役继续");
                }
                var rows = queue.ToArray();
                queue.Clear();
                for (int i = 0; i < rows.Length; i++)
                    queue.Enqueue(i < replacements.Count && replacements[i] != null
                        ? replacements[i] : rows[i]);
                if (queue.Count != total)
                    throw new InvalidOperationException("替换具名角色后 NPC 队列人数发生变化");
            }
            catch (Exception ex) { GameplayOverrideFailure.Abort("战役具名角色", ex); }
        }

        private static NpcSpawnPreset TryResolveNamed(string id)
        {
            if (!BattleCompositionPolicy.HasNpcPrefabAsset(id)) return null;
            BattleLevelConfig config = Traverse.Create(GameLevelManager.Instance)
                .Field("_levelConfig").GetValue<BattleLevelConfig>();
            NpcSpawnPreset found = null;
            if (config != null && config.List != null)
            {
                for (int i = 0; i < config.List.Count; i++)
                {
                    BattleLevel level = config.List[i];
                    if (level == null) continue;
                    string[] fields = { "_friendSpawnerPrefab", "_enemySpawnerPrefab", "_neutralSpawnerPrefab" };
                    for (int f = 0; f < fields.Length; f++)
                    {
                        GameObject owner = Traverse.Create(level).Field(fields[f]).GetValue<GameObject>();
                        NpcSpawner source = owner != null ? owner.GetComponent<NpcSpawner>() : null;
                        if (source == null) continue;
                        NpcSpawnPreset[] special = Traverse.Create(source)
                            .Field("_specialNPCs").GetValue<NpcSpawnPreset[]>();
                        found = MatchPreset(source.NpcPresets, special, id, found);
                    }
                }
            }
            // BattleLevelConfig.List 是原版 Setup 唯一读取的关卡/NPC preset 配置库。
            // Addressables 中 brother1/2/4、girl4/9、sister1 对应的是玩家战场技能
            // Animator/切入图，不是带 NpcCharacter/NpcStat 的可生成 prefab；不得把
            // 技能动画控制器猜成 NPC。配置库确无 preset 时返回 null，由调用方保留
            // 阵营普通席位，避免在 Battle Setup 中抛异常并中断整场战役。
            return found;
        }

        private static NpcSpawnPreset MatchPreset(
            IList<NpcSpawnPreset> normal, IList<NpcSpawnPreset> special,
            string id, NpcSpawnPreset current)
        {
            current = MatchList(normal, id, current);
            return MatchList(special, id, current);
        }

        private static NpcSpawnPreset MatchList(
            IList<NpcSpawnPreset> rows, string id, NpcSpawnPreset current)
        {
            if (rows == null) return current;
            for (int i = 0; i < rows.Count; i++)
            {
                NpcSpawnPreset preset = rows[i];
                if (!MatchesOfficialPreset(preset, id)) continue;
                if (current != null && !ReferenceEquals(current.Prefab, preset.Prefab))
                    throw new InvalidOperationException("官方人物资源匹配不唯一：" + id);
                current = preset;
            }
            return current;
        }

        private static bool MatchesOfficialPreset(NpcSpawnPreset preset, string id)
        {
            if (preset == null || preset.Prefab == null) return false;
            if (BattleCompositionPolicy.IsVerifiedAssetIdentity(id, preset.Prefab.name)) return true;
            Animator[] animators = preset.Prefab.GetComponentsInChildren<Animator>(true);
            for (int i = 0; i < animators.Length; i++)
            {
                RuntimeAnimatorController controller = animators[i] != null
                    ? animators[i].runtimeAnimatorController : null;
                if (controller == null) continue;
                if (BattleCompositionPolicy.IsVerifiedAssetIdentity(id, controller.name)) return true;
                AnimatorOverrideController overrides = controller as AnimatorOverrideController;
                if (overrides == null) continue;
                var clips = new List<KeyValuePair<AnimationClip, AnimationClip>>();
                overrides.GetOverrides(clips);
                for (int c = 0; c < clips.Count; c++)
                {
                    AnimationClip clip = clips[c].Value;
                    if (clip != null && BattleCompositionPolicy.IsVerifiedAssetIdentity(id, clip.name))
                        return true;
                }
            }
            return false;
        }
    }

}
