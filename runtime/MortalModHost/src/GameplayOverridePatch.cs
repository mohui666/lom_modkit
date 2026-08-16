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
    /// 高层 Combat 只克隆并覆盖当前决斗的 CombatStat，不修改原版共享资产。
    /// 角色外观、动画与基础招式仍来自作者选择的原版 CL_ 模板。
    /// </summary>
    [HarmonyPatch(typeof(CombatActionController), "SetStat",
        new Type[] { typeof(CombatStat) })]
    internal static class CombatStatOverridePatch
    {
        private static CombatStat _lastClone;

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
                CombatActionController enemy = Traverse.Create(manager)
                    .Field("_enemyAction").GetValue<CombatActionController>();
                if (!ReferenceEquals(__instance, enemy)) return;

                Clear();
                CombatStat clone = UnityObject.Instantiate(data);
                clone.name = data.name + "__MortalModHost";
                BattleOverrideResolver.ApplyCombatCharacter(clone);
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

        internal static void ApplyCombatCharacter(CombatStat clone)
        {
            string character = GameplaySession.ConfigString("character");
            if (string.IsNullOrEmpty(character))
                throw new InvalidOperationException("Combat v3 缺少 character");
            if (CombatCharacterPolicy.IsUserCharacter(character)) return;
            CombatLevelConfig config = Traverse.Create(CombatManager.Instance)
                .Field("_levelConfig").GetValue<CombatLevelConfig>();
            if (config == null || config.List == null)
                throw new InvalidOperationException("CombatLevelConfig 不可用");
            CombatEnemyAvatar avatar = null;
            string token = NormalizeAssetName(CombatCharacterPolicy.OfficialAssetToken(character));
            for (int i = 0; i < config.List.Count; i++)
            {
                CombatLevel level = config.List[i];
                CombatEnemyAvatar candidate = level != null && level.EnemyStat != null
                    ? level.EnemyStat.CombatAvatar : null;
                string key = candidate != null ? NormalizeAssetName(candidate.NormalKey) : "";
                if (key.IndexOf(token, StringComparison.Ordinal) < 0) continue;
                if (avatar != null && !SameAvatar(avatar, candidate))
                    throw new InvalidOperationException("官方决斗人物动画匹配不唯一：" + character);
                avatar = candidate;
            }
            if (avatar == null)
                throw new InvalidOperationException("找不到官方决斗人物动画：" + character);
            clone.CombatAvatar = avatar;
        }

        private static bool SameAvatar(CombatEnemyAvatar left, CombatEnemyAvatar right)
        {
            return left == right || (left != null && right != null
                && left.NormalKey == right.NormalKey && left.AttackKey == right.AttackKey
                && left.HurtKey == right.HurtKey && left.DefenceKey == right.DefenceKey);
        }

        private static string NormalizeAssetName(string value)
        {
            var chars = new List<char>();
            foreach (char c in (value ?? "").ToLowerInvariant())
                if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) chars.Add(c);
            return new string(chars.ToArray());
        }

        internal static GameObject ResolveFactionSpawner(string side, string fieldName)
        {
            if (!GameplaySession.PendingBattle) return null;
            string faction = GameplaySession.ConfigString(side + "_faction");
            int total;
            if (!GameplaySession.TryConfigInt(side + "_people", 0, 10000, out total)) total = 0;
            BattleCompositionPolicy.ParseCharacters(
                GameplaySession.ConfigString(side + "_characters"), total);
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

    /// <summary>在原版 SetData 完成后，仅替换本场敌人的四个 SpriteRenderer；离场立即销毁。</summary>
    [HarmonyPatch(typeof(CombatEnemyController), "SetData")]
    internal static class CombatCustomAvatarPatch
    {
        private static readonly List<Sprite> Sprites = new List<Sprite>();
        private static readonly List<Texture2D> Textures = new List<Texture2D>();

        private static void Postfix(CombatEnemyController __instance, ref IEnumerator __result)
        {
            if (!GameplaySession.PendingCombat || __instance == null) return;
            string character = GameplaySession.ConfigString("character");
            if (!CombatCharacterPolicy.IsUserCharacter(character)) return;
            __result = Wrap(__instance, __result, character);
        }

        private static IEnumerator Wrap(
            CombatEnemyController controller, IEnumerator original, string raw)
        {
            while (original != null && original.MoveNext()) yield return original.Current;
            try { Apply(controller, raw); }
            catch (Exception ex) { GameplayOverrideFailure.Abort("自定义决斗动画", ex); }
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

    [HarmonyPatch(typeof(CombatEnemyController), "OnDestroy")]
    internal static class CombatCustomAvatarCleanupPatch
    {
        private static void Prefix()
        {
            CombatCustomAvatarPatch.Clear();
            CombatStatOverridePatch.Clear();
        }
    }

    /// <summary>
    /// 原版 InitNpcList 会把 _specialNPCs 额外压入队列。v3 必须让具名角色计入
    /// total，因此先禁用额外队列，再在生成后的固定长度队列中替换前 N 个普通席位。
    /// </summary>
    [HarmonyPatch(typeof(NpcSpawner), "Setup")]
    internal static class BattleNamedCharacterPatch
    {
        private static string Prefix(NpcSpawner __instance, GameObject spawnerObject)
        {
            if (!GameplaySession.PendingBattle || __instance == null) return null;
            string side = FindSide(spawnerObject);
            if (side == null) return null;
            try
            {
                int total;
                if (!GameplaySession.TryConfigInt(side + "_people", 0, 10000, out total)) total = 0;
                List<string> ids = BattleCompositionPolicy.ParseCharacters(
                    GameplaySession.ConfigString(side + "_characters"), total);
                // 原版 specialNPC 始终是额外席位；v3 所有席位都必须包含在 total 内。
                Traverse.Create(__instance).Field("_specialNPCs").SetValue(new NpcSpawnPreset[0]);
                return side;
            }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("战役具名角色", ex);
                return null;
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
                for (int i = 0; i < ids.Count; i++) replacements.Add(ResolveNamed(ids[i]));
                var rows = queue.ToArray();
                queue.Clear();
                for (int i = 0; i < rows.Length; i++)
                    queue.Enqueue(i < replacements.Count ? replacements[i] : rows[i]);
                if (queue.Count != total)
                    throw new InvalidOperationException("替换具名角色后 NPC 队列人数发生变化");
            }
            catch (Exception ex) { GameplayOverrideFailure.Abort("战役具名角色", ex); }
        }

        private static NpcSpawnPreset ResolveNamed(string id)
        {
            BattleLevelConfig config = Traverse.Create(GameLevelManager.Instance)
                .Field("_levelConfig").GetValue<BattleLevelConfig>();
            string token = BattleCompositionPolicy.AssetToken(id);
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
                        found = MatchPreset(source.NpcPresets, special, token, found);
                    }
                }
            }
            if (found == null)
                throw new InvalidOperationException("找不到已验证的官方战役人物资源：" + id);
            return found;
        }

        private static NpcSpawnPreset MatchPreset(
            IList<NpcSpawnPreset> normal, IList<NpcSpawnPreset> special,
            string token, NpcSpawnPreset current)
        {
            current = MatchList(normal, token, current);
            return MatchList(special, token, current);
        }

        private static NpcSpawnPreset MatchList(
            IList<NpcSpawnPreset> rows, string token, NpcSpawnPreset current)
        {
            if (rows == null) return current;
            for (int i = 0; i < rows.Count; i++)
            {
                NpcSpawnPreset preset = rows[i];
                string name = preset != null && preset.Prefab != null ? preset.Prefab.name : "";
                string normalized = NormalizeAssetName(name);
                if (normalized.IndexOf(token, StringComparison.Ordinal) < 0) continue;
                if (current != null && !ReferenceEquals(current.Prefab, preset.Prefab))
                    throw new InvalidOperationException("官方人物资源匹配不唯一：" + token);
                current = preset;
            }
            return current;
        }

        private static string NormalizeAssetName(string value)
        {
            var chars = new List<char>();
            foreach (char c in (value ?? "").ToLowerInvariant())
                if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) chars.Add(c);
            return new string(chars.ToArray());
        }
    }

}
