using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    /// <summary>Package-owned active moves, executed by the native duel action and damage pipeline.</summary>
    internal static class CustomCombatSkillRuntime
    {
        internal sealed class Skill
        {
            internal ModPackage Package;
            internal UserContent Content;
            internal CombatActionController Actor;
            internal int NextRound;
            internal bool EffectsApplied;
            internal CustomCombatSkillDefinition Data { get { return Content.Gameplay.Skill; } }
        }

        private static readonly Dictionary<CombatActionController, List<Skill>> Equipped = new Dictionary<CombatActionController, List<Skill>>();
        private static readonly Dictionary<CombatActionController, Skill> Active = new Dictionary<CombatActionController, Skill>();
        private static Skill _pending;
        private static bool _roundRunning;
        private static CustomCombatSkillButtons _buttons;
        internal static bool RoundRunning { get { return _roundRunning; } }

        internal static void Validate(ModPackage package)
        {
            if (!GameplaySession.PendingCombat) return;
            foreach (string field in new[] { "player_skills", "enemy_skills" }) Resolve(package, field).ToList();
        }

        private static IEnumerable<UserContent> Resolve(ModPackage package, string field)
        {
            string rawList = GameplaySession.ConfigString(field);
            if (string.IsNullOrEmpty(rawList)) yield break;
            string[] refs = rawList.Split(',');
            if (refs.Length > 8) throw new InvalidOperationException("决斗每方至多装备8个自创招式");
            var seen = new HashSet<string>(StringComparer.Ordinal);
            foreach (string raw in refs)
            {
                UserContent content = CustomGameplayRuntime.ResolveContent(package, raw);
                if (content.Type != "skill" || content.Gameplay == null || content.Gameplay.Skill == null || !seen.Add(content.Id))
                    throw new InvalidOperationException("决斗招式未定义、类型错误或重复：" + raw);
                CustomCombatAnimationRuntime.Validate(content);
                yield return content;
            }
        }

        internal static void Attach(CombatActionController actor)
        {
            if (!GameplaySession.PendingCombat || actor == null) return;
            CombatManager manager = CombatManager.Instance;
            ModPackage package = CustomGameplayRuntime.CurrentPackage();
            string field = actor == manager.PlayerAction ? "player_skills" : "enemy_skills";
            var skills = new List<Skill>();
            foreach (UserContent content in Resolve(package, field))
            {
                CustomCombatAnimationRuntime.Preload(content);
                skills.Add(new Skill { Package = package, Content = content, Actor = actor });
            }
            Equipped[actor] = skills;
        }

        internal static void Clear()
        {
            if (_buttons != null) UObject.Destroy(_buttons.gameObject);
            _buttons = null;
            _pending = null;
            _roundRunning = false;
            Active.Clear();
            Equipped.Clear();
            CustomCombatAnimationRuntime.Clear();
        }

        internal static bool TryActive(CombatActionController actor, out Skill skill)
        {
            skill = null;
            return actor != null && Active.TryGetValue(actor, out skill);
        }

        internal static CombatActionType BaseAction(Skill skill)
        {
            if (skill.Data.Action == "weapon") return CombatActionType.暗器;
            if (skill.Data.Action == "ultimate") return skill.Actor.GetCurrentUltimateLevel();
            return CombatActionType.捅人;
        }

        private static bool Matches(Skill skill, CombatActionType action)
        {
            return skill.Data.Action == "attack" ? action == CombatActionType.捅人
                : skill.Data.Action == "weapon" ? action == CombatActionType.暗器
                : action == CombatActionType.絕招1 || action == CombatActionType.絕招2 || action == CombatActionType.絕招3;
        }

        internal static bool PanelReady(CombatActionButtonPanel panel)
        {
            var group = panel != null ? panel.GetComponent<CanvasGroup>() : null;
            return group != null && group.interactable && panel.gameObject.activeInHierarchy
                && (bool)AccessTools.Field(typeof(CombatActionButtonPanel), "_activeNav").GetValue(panel);
        }

        internal static bool CanUse(Skill skill)
        {
            if (skill.Actor == null || skill.Actor.Stat == null || !skill.Actor.Stat.CanAction || skill.Actor.IsDead
                || CombatManager.Instance.CurrentRounds < skill.NextRound || !ModPlayModeSession.Allows(skill.Package)) return false;
            Skill previous;
            bool had = Active.TryGetValue(skill.Actor, out previous);
            Active[skill.Actor] = skill;
            try
            {
                return skill.Data.Action == "weapon" ? skill.Actor.CanUseWeaponAction()
                    : skill.Data.Action == "ultimate" ? skill.Actor.CanUseUltimateAction() : skill.Actor.CanUseAttackAction();
            }
            finally { if (had) Active[skill.Actor] = previous; else Active.Remove(skill.Actor); }
        }

        internal static void Select(Skill skill, CombatActionButtonPanel panel)
        {
            if (_roundRunning || CustomConsumableRuntime.Busy || !PanelReady(panel) || !CanUse(skill)) return;
            _pending = skill;
            CombatManager.Instance.Action(BaseAction(skill));
        }

        internal static void ShowButtons(CombatActionButtonPanel panel)
        {
            CombatManager manager = CombatManager.Instance;
            List<Skill> skills;
            if (manager == null || !Equipped.TryGetValue(manager.PlayerAction, out skills) || skills.Count == 0) return;
            if (_buttons == null)
            {
                var root = new GameObject("ModActiveSkills", typeof(RectTransform));
                root.transform.SetParent(panel.transform, false);
                _buttons = root.AddComponent<CustomCombatSkillButtons>();
                _buttons.Setup(panel, skills);
            }
            _buttons.Show();
        }

        private static IEnumerator Round(IEnumerator native, CombatActionResult player, CombatActionResult enemy)
        {
            CombatManager manager = CombatManager.Instance;
            _roundRunning = true;
            try
            {
                if (_pending != null && _pending.Actor == manager.PlayerAction && CanUse(_pending))
                    Activate(_pending, player);
                _pending = null;
                List<Skill> candidates;
                if (Equipped.TryGetValue(manager.EnemyAction, out candidates))
                {
                    var ready = candidates.Where(s => CanUse(s) && UnityEngine.Random.Range(0, 100) < s.Data.AiChance).ToList();
                    if (ready.Count > 0) Activate(ready[UnityEngine.Random.Range(0, ready.Count)], enemy);
                }
                while (native.MoveNext()) yield return native.Current;
            }
            finally
            {
                var disposable = native as IDisposable;
                if (disposable != null) disposable.Dispose();
                Active.Clear();
                _pending = null;
                _roundRunning = false;
            }
        }

        private static void Activate(Skill skill, CombatActionResult result)
        {
            Active[skill.Actor] = skill;
            result.Action = BaseAction(skill);
            result.ResultType = CombatActionResultType.正常;
            skill.NextRound = CombatManager.Instance.CurrentRounds + skill.Data.Cooldown + 1;
            skill.EffectsApplied = false;
            Debug.Log("[CustomSkill] action=" + skill.Content.Id + " side="
                + (skill.Actor == CombatManager.Instance.PlayerAction ? "player" : "enemy")
                + " round=" + CombatManager.Instance.CurrentRounds);
        }

        internal static IEnumerator AfterDamage(CombatCharacterController character, IEnumerator native)
        {
            while (native.MoveNext()) yield return native.Current;
            var actor = (CombatActionController)AccessTools.Field(typeof(CombatCharacterController), "_sourceAction").GetValue(character);
            Skill skill;
            bool hit = (bool)AccessTools.Field(typeof(CombatCharacterController), "_isHit").GetValue(character);
            if (!hit || !TryActive(actor, out skill) || skill.EffectsApplied) yield break;
            skill.EffectsApplied = true;
            var enemy = actor == CombatManager.Instance.PlayerAction ? CombatManager.Instance.EnemyAction : CombatManager.Instance.PlayerAction;
            foreach (CustomSupportAction effect in skill.Data.Effects)
            {
                var target = effect.Target == "self" ? actor : enemy;
                if (target == null || target.IsDead) continue;
                int value = effect.Type == "damage" ? -effect.Value : effect.Value;
                if (effect.Type == "stamina")
                {
                    yield return CombatManager.Instance.DisplayStamina(value, target);
                    yield return target.ModifyStamina(value);
                }
                else
                {
                    yield return CombatManager.Instance.DisplayHealth(value, target);
                    yield return target.ModifyHealth(value);
                }
            }
            Debug.Log("[CustomSkill] hit=" + skill.Content.Id + " effects=" + skill.Data.Effects.Count);
        }

        [HarmonyPatch(typeof(CombatManager), "ActionRoundHandle")]
        private static class RoundPatch
        {
            private static void Postfix(CombatActionResult playerActionResult, CombatActionResult enemyActionResult, ref IEnumerator __result)
            { if (Equipped.Count > 0) __result = Round(__result, playerActionResult, enemyActionResult); }
        }

        [HarmonyPatch(typeof(CombatActionController), nameof(CombatActionController.GetActionCost))]
        private static class CostPatch
        {
            private static void Postfix(CombatActionController __instance, CombatActionType action, ref int __result)
            { Skill skill; if (TryActive(__instance, out skill) && Matches(skill, action)) __result = -skill.Data.StaminaCost; }
        }

        [HarmonyPatch(typeof(CombatActionController), nameof(CombatActionController.CanUseAnyAction))]
        private static class AvailablePatch
        {
            private static void Postfix(CombatActionController __instance, ref bool __result)
            {
                List<Skill> skills;
                if (!__result && Equipped.TryGetValue(__instance, out skills)) __result = skills.Any(CanUse);
            }
        }

        [HarmonyPatch(typeof(CombatActionController), nameof(CombatActionController.SetCurrentDamageResult))]
        private static class DamagePatch
        {
            private static void Postfix(CombatActionController __instance, CombatActionType action)
            {
                Skill skill;
                if (!TryActive(__instance, out skill) || !Matches(skill, action) || __instance.CurrentDamageResult == null) return;
                __instance.CurrentDamageResult.Item.SetBaseValue(skill.Data.Power);
                __instance.CurrentDamageResult.Item.UpdateFinalValue();
                __instance.CurrentDamageResult.DamageCount = 1;
            }
        }

        [HarmonyPatch(typeof(CombatActionController), nameof(CombatActionController.UpdateCurrentActionName))]
        private static class NamePatch
        {
            private static void Postfix(CombatActionController __instance)
            { Skill skill; if (TryActive(__instance, out skill) && Matches(skill, __instance.CurrentActionResult.Action)) __instance.CurrentActionResult.Name = skill.Content.Name; }
        }

        [HarmonyPatch]
        private static class AccuracyPatch
        {
            private static IEnumerable<MethodBase> TargetMethods()
            {
                yield return AccessTools.Method(typeof(CombatActionController), "GetAttackHitRate");
                yield return AccessTools.Method(typeof(CombatActionController), "GetWeaponHitRate");
            }
            private static void Postfix(CombatActionController __instance, ref float __result)
            { Skill skill; if (TryActive(__instance, out skill)) __result = skill.Data.Accuracy / 100f; }
        }

        [HarmonyPatch(typeof(CombatCharacterController), nameof(CombatCharacterController.HandleDamage))]
        private static class HitPatch
        {
            private static void Postfix(CombatCharacterController __instance, ref IEnumerator __result)
            { if (Active.Count > 0) __result = AfterDamage(__instance, __result); }
        }

        [HarmonyPatch(typeof(CombatActionButtonPanel), nameof(CombatActionButtonPanel.Show))]
        private static class ShowPatch
        {
            private static void Postfix(CombatActionButtonPanel __instance, ref IEnumerator __result)
            { __result = Show(__instance, __result); }
            private static IEnumerator Show(CombatActionButtonPanel panel, IEnumerator native)
            { while (native.MoveNext()) yield return native.Current; ShowButtons(panel); }
        }
    }

    internal sealed class CustomCombatSkillButtons : MonoBehaviour
    {
        private CombatActionButtonPanel _panel;
        private readonly List<KeyValuePair<CustomCombatSkillRuntime.Skill, CombatActionButton>> _rows = new List<KeyValuePair<CustomCombatSkillRuntime.Skill, CombatActionButton>>();

        internal void Setup(CombatActionButtonPanel panel, List<CustomCombatSkillRuntime.Skill> skills)
        {
            _panel = panel;
            var root = (RectTransform)transform;
            root.anchorMin = root.anchorMax = new Vector2(.5f, 1);
            root.pivot = new Vector2(.5f, 0);
            root.anchoredPosition = new Vector2(0, 20);
            root.sizeDelta = new Vector2(760, 190);
            var template = (CombatActionButton)AccessTools.Field(typeof(CombatActionButtonPanel), "_attackButton").GetValue(panel);
            for (int i = 0; i < skills.Count; i++)
            {
                var skill = skills[i];
                CombatActionButton button = UObject.Instantiate(template, transform, false);
                button.gameObject.name = "ModSkill_" + skill.Content.Id;
                var rect = (RectTransform)button.transform;
                rect.anchorMin = rect.anchorMax = new Vector2(.5f, 1);
                rect.pivot = new Vector2(.5f, .5f);
                rect.sizeDelta = new Vector2(280, 140);
                rect.localScale = Vector3.one * .6f;
                rect.anchoredPosition = new Vector2((i % 4 - 1.5f) * 190, -48 - (i / 4) * 94);
                CustomGameplayRuntime.Set(button, "_hotKey", "");
                CustomGameplayRuntime.Set(button, "_focusOffsetY", 8f);
                CustomGameplayRuntime.Set(button, "OnButtonClick", (Action)(() => CustomCombatSkillRuntime.Select(skill, _panel)));
                CustomGameplayRuntime.Set(button, "OnButtonEnter", (Action)(() => { }));
                CustomGameplayRuntime.Set(button, "OnButtonExit", (Action)(() => { }));
                var move = (MovePanel)AccessTools.Field(typeof(CombatActionButton), "_movePanel").GetValue(button);
                CustomGameplayRuntime.Set(move, "_originAnchoredPosition", (Vector3)((RectTransform)move.transform).anchoredPosition);
                button.SetName(skill.Content.Name);
                button.SetCost(-skill.Data.StaminaCost);
                button.SetTipText(skill.Content.Gameplay.Description + "\n基础威力 " + skill.Data.Power + " · 命中 " + skill.Data.Accuracy
                    + "% · 冷却 " + skill.Data.Cooldown + " 回合\n命中后附加效果：" + skill.Data.Effects.Count + " 项");
                button.HideTip();
                button.gameObject.SetActive(true);
                _rows.Add(new KeyValuePair<CustomCombatSkillRuntime.Skill, CombatActionButton>(skill, button));
            }
        }

        internal void Show() { gameObject.SetActive(true); Refresh(); }
        private void Update() { Refresh(); }
        private void Refresh()
        {
            bool ready = CustomCombatSkillRuntime.PanelReady(_panel) && !CustomCombatSkillRuntime.RoundRunning && !CustomConsumableRuntime.Busy;
            foreach (var row in _rows)
            {
                bool active = ready && CustomCombatSkillRuntime.CanUse(row.Key);
                row.Value.Active(active);
                int remaining = Math.Max(0, row.Key.NextRound - CombatManager.Instance.CurrentRounds);
                row.Value.SetInfoText(remaining > 0 ? "冷却 " + remaining : "自创招式");
            }
        }
    }
}
