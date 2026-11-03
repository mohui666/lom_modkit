using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using OBB.Framework.Utils;
using UnityEngine;

namespace MortalModHost
{
    internal static class CustomConsumableRuntime
    {
        internal sealed class Entry
        {
            internal ModPackage Package;
            internal UserContent Content;
            internal Consumables Item;
            internal string Reference { get { return "user:" + Content.Id; } }
        }
        private static readonly Dictionary<int, Entry> Items = new Dictionary<int, Entry>();
        private static bool _busy;
        private static int _managerId, _usedRound = -1;
        internal static bool Busy { get { return _busy; } }
        internal static IEnumerable<Entry> Entries { get { return Items.Values.Where(e => e.Item != null && ModPlayModeSession.Allows(e.Package)); } }

        internal static Consumables Create(ModPackage package, UserContent content)
        {
            var item = CustomGameplayRuntime.Own(ScriptableObject.CreateInstance<Consumables>());
            var effects = new List<ItemEffect>();
            foreach (CustomConsumableEffect definition in content.Gameplay.ConsumableEffects)
            {
                if (definition.Type != "stat") continue;
                GameStatType type = StatType(definition.Stat);
                var effect = new ItemEffect();
                CustomGameplayRuntime.Set(effect, "_statType", type);
                CustomGameplayRuntime.Set(effect, "_value", definition.Value);
                effects.Add(effect);
            }
            CustomGameplayRuntime.Set(item, "_effects", effects.ToArray());
            Items.Add(item.GetInstanceID(), new Entry { Package = package, Content = content, Item = item });
            return item;
        }

        internal static void Unregister(ItemData item)
        {
            if (item != null) Items.Remove(item.GetInstanceID());
        }
        internal static bool TryGet(ItemData item, out Entry entry)
        {
            entry = null;
            return item != null && Items.TryGetValue(item.GetInstanceID(), out entry);
        }
        internal static void Clear() { Items.Clear(); ResetCombat(); }
        internal static void ResetCombat() { _busy = false; _managerId = 0; _usedRound = -1; }

        internal static void Persist(ItemData item)
        {
            Entry entry;
            if (!TryGet(item, out entry)) return;
            var stats = PlayerStatManagerData.Instance;
            if (stats == null) throw new InvalidOperationException("玩家物品存档尚未就绪");
            CustomConsumableState.Write(stats.StoryKeyList, entry.Package.Id, entry.Content.Id, item.CurrentCount);
        }
        internal static void RestoreCounts()
        {
            var stats = PlayerStatManagerData.Instance;
            if (stats == null) throw new InvalidOperationException("玩家物品存档尚未就绪");
            foreach (Entry entry in Items.Values)
            {
                if (entry.Item == null) continue;
                entry.Item.SetCount(CustomConsumableState.Read(stats.StoryKeyList, entry.Package.Id, entry.Content.Id));
            }
        }

        internal static Entry Resolve(ModPackage package, string reference)
        {
            if (reference == null || !reference.StartsWith("user:", StringComparison.Ordinal))
                throw new ArgumentException("主动消耗品必须引用本包 user: 内容");
            string id = CustomGameplayRuntime.ResolveItemId(package, "consumable", reference);
            ItemData item = ItemDatabase.Instance.GetItem(GameItemType.消耗品, id) as ItemData;
            Entry entry;
            if (!TryGet(item, out entry) || entry.Package.Id != package.Id)
                throw new InvalidOperationException("自定义消耗品没有注册：" + reference);
            return entry;
        }

        internal static string FieldUnavailable(Entry entry)
        {
            string common = CommonUnavailable(entry);
            if (common != null) return common;
            if (entry.Content.Gameplay.UseIn != "field") return "仅可在决斗中使用";
            var scenes = SceneController.Instance;
            if (scenes == null || (scenes.CurrentScene != "Story" && scenes.CurrentScene != "Free"))
                return "仅可在剧情或养成中使用";
            if (_busy) return "正在使用道具";
            return null;
        }

        internal static void UseField(ModPackage package, string reference)
        {
            Entry entry = Resolve(package, reference);
            string reason = FieldUnavailable(entry);
            if (reason != null) throw new InvalidOperationException(reason);
            var changes = new List<KeyValuePair<GameStatType, int>>();
            var planned = new Dictionary<GameStatType, long>();
            PlayerStatData stats = PlayerStatManagerData.Instance.Stats;
            CustomConsumableState.Read(PlayerStatManagerData.Instance.StoryKeyList, entry.Package.Id, entry.Content.Id);
            foreach (CustomConsumableEffect effect in entry.Content.Gameplay.ConsumableEffects)
            {
                if (effect.Type != "stat") throw new InvalidOperationException("剧情道具含有非属性效果");
                GameStatType type = StatType(effect.Stat);
                GameStat stat = stats.Get(type);
                long previous;
                if (!planned.TryGetValue(type, out previous)) previous = stat.Value;
                long next = previous + effect.Value;
                if (next < int.MinValue || next > int.MaxValue) throw new InvalidOperationException("属性变化超过整数范围");
                planned[type] = next;
                changes.Add(new KeyValuePair<GameStatType, int>(type, effect.Value));
            }
            foreach (var change in changes) stats.Add(change.Key, change.Value);
            entry.Item.AddCount(-1);
        }

        internal static string CombatUnavailable(Entry entry)
        {
            string common = CommonUnavailable(entry);
            if (common != null) return common;
            if (entry.Content.Gameplay.UseIn != "combat") return "仅可在剧情或养成中使用";
            string phase = CombatPhaseUnavailable();
            if (phase != null) return phase;
            CombatManager manager = CombatManager.Instance;
            CombatActionController player = manager.GetActionController(CombatCharacterType.我方);
            bool useful = entry.Content.Gameplay.ConsumableEffects.Any(e =>
                (e.Type == "heal" && player.Stat.CurrentHealth < player.Stat.MaxHealthValue)
                || (e.Type == "stamina" && player.Stat.CurrentStamina < player.Stat.MaxStamina.FinalValue));
            return useful ? null : "气血和气力无需恢复";
        }

        internal static string CombatPhaseUnavailable()
        {
            CombatManager manager = CombatManager.Instance;
            var scenes = SceneController.Instance;
            if (manager == null || scenes == null || scenes.CurrentScene != "Combat") return "仅可在决斗中使用";
            if (_busy) return "正在使用道具";
            if (scenes.IsLoading || scenes.IsPrepare || !manager.EnableHotKey) return "等待当前行动结束";
            CombatUIManager ui = CombatUIManager.Instance;
            CombatActionButtonPanel actions = ui != null ? Traverse.Create(ui).Field("_actionPanel").GetValue<CombatActionButtonPanel>() : null;
            CanvasGroup group = actions != null ? actions.GetComponent<CanvasGroup>() : null;
            if (actions == null || !actions.gameObject.activeInHierarchy || group == null || !group.interactable)
                return "等待当前行动结束";
            if (!Traverse.Create(actions).Field("_activeNav").GetValue<bool>()) return "关闭菜单后再使用道具";
            if (_managerId == manager.GetInstanceID() && _usedRound == manager.CurrentRounds) return "本回合已使用道具";
            CombatActionController player = manager.GetActionController(CombatCharacterType.我方);
            if (player == null || player.Stat == null || player.IsDead) return "当前无法使用道具";
            return null;
        }

        internal static bool HasCombatChoice(CombatActionController actor)
        {
            CombatManager manager = CombatManager.Instance;
            if (_busy || manager == null || actor == null || actor.Stat == null || actor.IsDead
                || !actor.Stat.CanAction || actor.IsAutoAction()
                || manager.GetActionController(CombatCharacterType.我方) != actor
                || (_managerId == manager.GetInstanceID() && _usedRound == manager.CurrentRounds)) return false;
            return Entries.Any(entry => entry.Item.CurrentCount > 0 && entry.Content.Gameplay.UseIn == "combat"
                && entry.Content.Gameplay.ConsumableEffects.Any(effect =>
                    (effect.Type == "heal" && actor.Stat.CurrentHealth < actor.Stat.MaxHealthValue)
                    || (effect.Type == "stamina" && actor.Stat.CurrentStamina < actor.Stat.MaxStamina.FinalValue)));
        }

        internal static IEnumerator UseCombat(ModPackage package, string reference)
        {
            Entry entry = Resolve(package, reference);
            string reason = CombatUnavailable(entry);
            if (reason != null) throw new InvalidOperationException(reason);
            CombatManager manager = CombatManager.Instance;
            CustomConsumableState.Read(PlayerStatManagerData.Instance.StoryKeyList, entry.Package.Id, entry.Content.Id);
            CombatActionController player = manager.GetActionController(CombatCharacterType.我方);
            _busy = true;
            _managerId = manager.GetInstanceID();
            _usedRound = manager.CurrentRounds;
            CombatUIManager.Instance.HideAction();
            try
            {
                manager.AddGameLog("使用道具：" + entry.Content.Name);
                foreach (CustomConsumableEffect effect in entry.Content.Gameplay.ConsumableEffects)
                {
                    if (effect.Type == "heal")
                    {
                        yield return manager.DisplayHealth(effect.Value, player);
                        yield return player.ModifyHealth(effect.Value);
                    }
                    else if (effect.Type == "stamina")
                    {
                        yield return manager.DisplayStamina(effect.Value, player);
                        yield return player.ModifyStamina(effect.Value);
                    }
                    else throw new InvalidOperationException("决斗道具含有非恢复效果");
                }
                entry.Item.AddCount(-1);
            }
            finally { _busy = false; }
            // The real original action pipeline resolves the enemy and advances the
            // round. No-action also retains the game's standard stamina recovery.
            manager.Action(CombatActionType.無動作);
        }

        private static string CommonUnavailable(Entry entry)
        {
            if (entry == null || entry.Item == null || !ModPlayModeSession.Allows(entry.Package)) return "当前玩法未启用此道具";
            return entry.Item.CurrentCount > 0 ? null : "道具数量不足";
        }
        private static GameStatType StatType(string key)
        {
            GameStatType type;
            if (!EnumUtils.TryParseByStringValue<GameStatType>(key, out type)
                || PlayerStatManagerData.Instance == null || PlayerStatManagerData.Instance.Stats.Get(type) == null)
                throw new InvalidOperationException("道具属性未定义：" + key);
            return type;
        }
    }

    [HarmonyPatch(typeof(ItemData), "AddCount")]
    internal static class CustomConsumableCountPatch
    {
        private static void Postfix(ItemData __instance) { CustomConsumableRuntime.Persist(__instance); }
    }
    [HarmonyPatch(typeof(ItemDatabase), "Load")]
    internal static class CustomConsumableLoadPatch
    {
        private static void Postfix(ItemDatabase __instance)
        {
            if (__instance == CustomGameplayRuntime.Database) CustomConsumableRuntime.RestoreCounts();
        }
    }
    [HarmonyPatch(typeof(CombatManager), "Action")]
    internal static class CustomConsumableActionLockPatch
    {
        private static bool Prefix() { return !CustomConsumableRuntime.Busy; }
    }
    [HarmonyPatch(typeof(CombatActionController), "CanUseAnyAction")]
    internal static class CustomConsumableChoicePatch
    {
        private static void Postfix(CombatActionController __instance, ref bool __result)
        {
            if (!__result && CustomConsumableRuntime.HasCombatChoice(__instance)) __result = true;
        }
    }
}
