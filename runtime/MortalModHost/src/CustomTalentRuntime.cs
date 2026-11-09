using System;
using System.Collections.Generic;
using System.Linq;
using HarmonyLib;
using Mortal.Core;
using Mortal.Combat;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    // Only the collection is copied. Official talents retain their game-owned state;
    // new talents are independent objects and have no combat/battle effect keys.
    internal static class CustomTalentRuntime
    {
        private static PlayerTalentDataCollection _source, _collection;
        private static readonly Dictionary<string, PlayerTalentData> Talents = new Dictionary<string, PlayerTalentData>(StringComparer.Ordinal);

        internal static PlayerTalentData Register(string id)
        {
            if (_collection == null)
            {
                _source = PlayerStatManagerData.Instance.Talents;
                _collection = UObject.Instantiate(_source);
                AccessTools.Field(typeof(PlayerTalentDataCollection), "_list")
                    .SetValue(_collection, new List<PlayerTalentData>(_source.List));
            }
            if (_collection.Get(id) != null) throw new InvalidOperationException("自定义天赋 ID 重复：" + id);
            var talent = ScriptableObject.CreateInstance<PlayerTalentData>();
            talent.name = id;
            AccessTools.Field(typeof(PlayerTalentData), "_id").SetValue(talent, id);
            AccessTools.Field(typeof(PlayerTalentData), "_displayLevel").SetValue(talent, false);
            AccessTools.Field(typeof(PlayerTalentData), "_lowLevelTalent").SetValue(talent, new PlayerTalentData[0]);
            _collection.List.Add(talent);
            Talents.Add(id, talent);
            return talent;
        }

        internal static string ResolveId(ModPackage package, string raw)
        {
            if (raw == null || !raw.StartsWith("user:", StringComparison.Ordinal)) return raw;
            UserContent content = CustomGameplayRuntime.ResolveContent(package, raw);
            if (content.Type != "talent") throw new ArgumentException("不是自定义天赋：" + raw);
            CustomGameplayRuntime.EnsurePackage(package);
            return CustomGameplayRuntime.RuntimeId(package, content.Id);
        }

        internal static void Change(ModPackage package, string raw, int level)
        {
            if (level != 1 && level != -1) throw new ArgumentException("天赋增减只能为 +1 或 -1");
            if (package == null || !ModPlayModeSession.Allows(package)) throw new InvalidOperationException("当前 MOD 未启用");
            var modifier = UObject.FindObjectOfType<StatModifyManager>();
            if (modifier == null) throw new InvalidOperationException("当前场景不能授予天赋");
            modifier.AddTalent(ResolveId(package, raw), level);
        }

        internal static bool Redirect(PlayerTalentDataCollection source)
        {
            return _collection != null && ReferenceEquals(source, _source);
        }
        internal static PlayerTalentDataCollection Collection { get { return _collection; } }
        internal static IEnumerable<PlayerTalentData> Acquired { get { return Talents.Values.Where(t => t.Level > 0); } }

        internal static void Unregister(string id)
        {
            PlayerTalentData talent;
            if (!Talents.TryGetValue(id, out talent)) return;
            _collection.List.Remove(talent);
            Talents.Remove(id);
            UObject.Destroy(talent);
        }

        internal static void Clear()
        {
            foreach (PlayerTalentData talent in Talents.Values) UObject.Destroy(talent);
            Talents.Clear();
            if (_collection != null) UObject.Destroy(_collection);
            _source = null;
            _collection = null;
        }
    }

    [HarmonyPatch(typeof(PlayerTalentDataCollection), "Get")]
    internal static class CustomTalentLookupPatch
    {
        private static bool Prefix(PlayerTalentDataCollection __instance, string id, ref PlayerTalentData __result)
        {
            if (!CustomTalentRuntime.Redirect(__instance)) return true;
            __result = CustomTalentRuntime.Collection.Get(id);
            return false;
        }
    }
    [HarmonyPatch(typeof(PlayerTalentDataCollection), "Reset")]
    internal static class CustomTalentResetPatch
    {
        private static bool Prefix(PlayerTalentDataCollection __instance)
        {
            if (!CustomTalentRuntime.Redirect(__instance)) return true;
            CustomTalentRuntime.Collection.Reset();
            return false;
        }
    }
    [HarmonyPatch(typeof(PlayerStatManagerData), "get_Talents")]
    internal static class CustomTalentCollectionPatch
    {
        private static void Postfix(ref PlayerTalentDataCollection __result)
        {
            if (CustomTalentRuntime.Redirect(__result)) __result = CustomTalentRuntime.Collection;
        }
    }
    [HarmonyPatch(typeof(PlayerStatManagerData), "Save")]
    internal static class CustomTalentSavePatch
    {
        private static void Postfix(GameSave gameSave)
        {
            foreach (PlayerTalentData talent in CustomTalentRuntime.Acquired)
                gameSave.Talents.Add(new TalentSave { Id = talent.Id, Level = talent.Level });
        }
    }
    // Display-only rows: never add an empty effect key to CombatStat.TalentItems.
    [HarmonyPatch(typeof(CombatCharacterStatusUI), "UpdateSkill")]
    internal static class CustomTalentCombatListPatch
    {
        private static void Postfix(CombatActionController ____actionController,
            GameObject ____skillPrefab, ToggleGroup ____skillToggleGroup, GameObject ____emptySkillText)
        {
            if (CombatManager.Instance == null || ____actionController != CombatManager.Instance.PlayerAction) return;
            foreach (PlayerTalentData talent in CustomTalentRuntime.Acquired)
            {
                GameObject row = UObject.Instantiate(____skillPrefab, ____skillToggleGroup.transform);
                row.GetComponent<Toggle>().group = ____skillToggleGroup;
                row.GetComponent<CombatSkillTextPanel>().Setup(talent, talent.Level);
                ____emptySkillText.SetActive(false);
            }
        }
    }
}
