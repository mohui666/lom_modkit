using System.Collections;
using Mortal.Combat;
using UnityEngine;

namespace MortalModHost
{
    // This class is instantiated only for a package-owned CombatStateEffectScriptable.
    public sealed class CustomSupportEffectUnit : CombatStateEffectUnitScriptable
    {
        internal CustomGameplayDefinition Definition;
        private int _remaining, _lastRound = -1;
        private bool _casting;
        public override IEnumerator Init(CombatStateEffectParameter param)
        {
            _remaining = Definition.Uses;
            param.Effect.DisplayCount = _remaining;
            yield break;
        }
        public override IEnumerator ApplyBeforeAction(CombatStateEffectParameter param)
        {
            if (Definition.Trigger != "round_start" || param.CombatManager.CurrentRounds == _lastRound) yield break;
            _lastRound = param.CombatManager.CurrentRounds;
            yield return Cast(param);
        }
        public override IEnumerator ApplyAfterAction(CombatStateEffectParameter param)
        {
            if (Definition.Trigger == "after_action") yield return Cast(param);
        }
        public override bool CheckTriggerAfterAttacked(CombatStateEffectParameter param)
        {
            return Definition.Trigger == "after_hit" && _remaining > 0 && !_casting;
        }
        public override IEnumerator TriggerAfterAttacked(CombatStateEffectParameter param)
        {
            if (Definition.Trigger == "after_hit") yield return Cast(param);
        }
        private IEnumerator Cast(CombatStateEffectParameter param)
        {
            if (_remaining <= 0 || _casting || param.Source == null || param.Source.IsDead
                || Random.Range(0, 100) >= Definition.Chance) yield break;
            _casting = true;
            try
            {
                _remaining--;
                param.Effect.DisplayCount = _remaining;
                if (param.Effect.UI != null) yield return param.Effect.UI.Cast();
                foreach (CustomSupportAction action in Definition.Effects)
                {
                    CombatActionController target = action.Target == "self" ? param.Source : param.Target;
                    if (target == null || target.IsDead) continue;
                    if (action.Type == "stamina") yield return target.ModifyStamina(action.Value);
                    else yield return target.ModifyHealth(action.Type == "damage" ? -action.Value : action.Value);
                }
                if (param.Effect.UI != null)
                {
                    yield return param.Effect.UI.CastFinish();
                    param.Effect.UI.UpdateEffect();
                }
                if (_remaining == 0)
                {
                    param.Effect.SetDisable(true);
                    param.Effect.SetRemove(true);
                }
            }
            finally { _casting = false; }
        }
    }
}
