using System;
using System.Collections.Generic;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 战前只把作者填的赵活基准值写入官方 GameStat.Value / Talent.Setup，
    /// 让 SetPlayerStat 按 FinalValue 和 _playerTotalHealth 换算血量。
    /// 不写 SaveSystem。战后把改过的字段写回快照。
    /// </summary>
    internal static class CombatPlayerStatSession
    {
        private static readonly Dictionary<GameStatType, GameStatSnapshot> StatSnap =
            new Dictionary<GameStatType, GameStatSnapshot>();
        private static readonly Dictionary<string, int> TalentSnap =
            new Dictionary<string, int>(StringComparer.Ordinal);
        private static bool _active;
        private static bool _hasOfficialVitality;
        private static int _vitalityControllerId;
        private static int _vitalityDataId;
        private static int _officialMaxHealth;
        private static int _officialMaxStamina;

        private struct GameStatSnapshot
        {
            public int Value;
            public int Max;
        }

        internal static bool Active { get { return _active; } }

        /// <summary>
        /// 原版存档必须看到战前官方 GameStat，不能把本场 player_* 临时覆盖写进
        /// GameSave；否则下一次读档再次应用覆盖时会把血量/气力重复加上去。
        /// </summary>
        internal static bool SuspendForSave()
        {
            bool resume = _active;
            if (resume) Restore();
            return resume;
        }

        internal static void ResumeAfterSave(bool resume)
        {
            if (!resume || !GameplaySession.PendingCombat) return;
            CaptureAndApply();
        }

        internal static void CaptureAndApply()
        {
            Restore();
            if (!GameplaySession.PendingCombat) return;
            if (!CombatPlayerOverridePolicy.HasAny(GameplaySession.HasConfig)) return;
            PlayerStatManagerData stats = PlayerStatManagerData.Instance;
            if (stats == null)
                throw new InvalidOperationException("PlayerStatManagerData 尚未就绪");
            ApplyGameStatBases(stats);
            ApplyTalentBases(stats);
            _active = StatSnap.Count > 0 || TalentSnap.Count > 0;
            if (_active)
                LuaManagerPatch.Log?.LogInfo(
                    "赵活决斗基准已写入官方 GameStat/Talent，字段="
                    + (StatSnap.Count + TalentSnap.Count));
        }

        internal static void Restore()
        {
            if (StatSnap.Count == 0 && TalentSnap.Count == 0)
            {
                ClearVitalityBaseline();
                _active = false;
                return;
            }
            PlayerStatManagerData stats = PlayerStatManagerData.Instance;
            if (stats != null)
            {
                foreach (KeyValuePair<GameStatType, GameStatSnapshot> pair in StatSnap)
                    {
                        GameStat stat = stats.Stats.Get(pair.Key);
                        if (stat != null)
                        {
                            GameStatSnapshot snap = pair.Value;
                            stat.SetMax(snap.Max);
                            stats.Stats.Set(pair.Key, snap.Value);
                        }
                    }
                foreach (KeyValuePair<string, int> pair in TalentSnap)
                    stats.Talents.Set(pair.Key, pair.Value);
            }
            StatSnap.Clear();
            TalentSnap.Clear();
            ClearVitalityBaseline();
            _active = false;
            LuaManagerPatch.Log?.LogInfo("赵活决斗基准已从内存快照写回，未写官方存档槽");
        }

        private static void ClearVitalityBaseline()
        {
            _hasOfficialVitality = false;
            _vitalityControllerId = 0;
            _vitalityDataId = 0;
            _officialMaxHealth = 0;
            _officialMaxStamina = 0;
        }

        internal static void SyncVitalityItems(CombatStatController controller)
        {
            if (controller == null || controller.Data == null) return;
            CombatStat data = controller.Data;
            // SetStat creates these items from CombatStat fields. Replacing an
            // existing item later drops original AddModify buffs, so rebase the
            // same item and retain its ModifyList/FinalValue calculation.
            controller.MaxHealth = RebaseVitalityItem(controller.MaxHealth, data.MaxHealth);
            controller.MaxStamina = RebaseVitalityItem(controller.MaxStamina, data.MaxStamina);
        }

        private static CombatStatItem RebaseVitalityItem(CombatStatItem item, int value)
        {
            if (item == null) return new CombatStatItem(value);
            item.SetBaseValue(value);
            item.UpdateFinalValue();
            return item;
        }

        internal static void ApplyExplicitVitality(CombatActionController player)
        {
            if (player == null || player.Stat == null || player.Stat.Data == null) return;
            CombatStatController controller = player.Stat;
            CombatStat data = controller.Data;
            CaptureOfficialVitality(controller);
            if (!CombatPlayerOverridePolicy.TouchesVitality(GameplaySession.HasConfig))
            {
                SyncVitalityItems(controller);
                return;
            }
            int extraHealth;
            int extraStamina;
            int currentHealth;
            int currentStamina;
            int maxHealth = _officialMaxHealth;
            int maxStamina = _officialMaxStamina;
            if (GameplaySession.TryConfigInt("player_max_health", 1, 10000000, out extraHealth))
                maxHealth = CombatVitalPolicy.AddHealthBaseBonus(_officialMaxHealth, extraHealth);
            if (GameplaySession.TryConfigInt("player_max_stamina", 0, 100000, out extraStamina))
                maxStamina = extraStamina;
            data.MaxHealth = maxHealth;
            data.MaxStamina = maxStamina;
            data.DefaultHealth = maxHealth;
            data.DefaultStamina = maxStamina;
            if (GameplaySession.TryConfigInt("player_health", 0, 10000000, out currentHealth))
                data.DefaultHealth = currentHealth;
            if (GameplaySession.TryConfigInt("player_stamina", 0, 100000, out currentStamina))
                data.DefaultStamina = currentStamina;
            if (data.MaxHealth > 0)
                data.DefaultHealth = Mathf.Clamp(data.DefaultHealth, 0, data.MaxHealth);
            if (data.MaxStamina > 0)
                data.DefaultStamina = Mathf.Clamp(data.DefaultStamina, 0, data.MaxStamina);
            data.Reset();
            SyncVitalityItems(controller);
            LuaManagerPatch.Log?.LogInfo(
                "Combat 赵活生命基准已应用：official=" + _officialMaxHealth
                + "；extra=" + (GameplaySession.HasConfig("player_max_health")
                    ? GameplaySession.ConfigString("player_max_health") : "0")
                + "；base=" + controller.MaxHealth.BaseValue
                + "；final=" + controller.MaxHealth.FinalValue
                + "；current=" + data.CurrentHealth);
        }

        /// <summary>
        /// SetPlayerStat 之后的 CombatStat 才是原版体力、被动和武学都结算完成的
        /// 基准。场景切换期间会残留上一次 Combat 的托管对象，故必须同时绑定
        /// 控制器和 CombatStat 的 InstanceID，不能用一个跨场静态 bool 判断。
        /// </summary>
        internal static void CaptureOfficialVitality(CombatStatController controller)
        {
            if (controller == null || controller.Data == null) return;
            int controllerId = controller.GetInstanceID();
            int dataId = controller.Data.GetInstanceID();
            if (_hasOfficialVitality
                && _vitalityControllerId == controllerId
                && _vitalityDataId == dataId)
                return;
            _hasOfficialVitality = true;
            _vitalityControllerId = controllerId;
            _vitalityDataId = dataId;
            _officialMaxHealth = controller.Data.MaxHealth;
            _officialMaxStamina = controller.Data.MaxStamina;
            LuaManagerPatch.Log?.LogInfo(
                "Combat 赵活原版生命基准已捕获：health=" + _officialMaxHealth
                + "；stamina=" + _officialMaxStamina
                + "；controller=" + controllerId
                + "；stat=" + dataId);
        }

        private static void ApplyGameStatBases(PlayerStatManagerData stats)
        {
            string[] fields = CombatPlayerOverridePolicy.StatFields;
            for (int i = 0; i < fields.Length; i++)
            {
                string official;
                if (!CombatPlayerOverridePolicy.TryOfficialGameStatType(fields[i], out official))
                    continue;
                string key = CombatPlayerOverridePolicy.Key(fields[i]);
                int value;
                if (!GameplaySession.TryConfigInt(key, 0, 10000, out value)) continue;
                GameStatType type;
                if (!Enum.TryParse(official, out type))
                    throw new InvalidOperationException("官方 GameStatType 不可用：" + official);
                GameStat stat = stats.Stats.Get(type);
                if (stat == null)
                    throw new InvalidOperationException("官方 GameStat 不存在：" + official);
                if (!StatSnap.ContainsKey(type))
                    StatSnap.Add(type, new GameStatSnapshot { Value = stat.Value, Max = stat.Max });
                if (value > stat.Max)
                    stat.SetMax(value);
                stats.Stats.Set(type, value);
            }
        }

        private static void ApplyTalentBases(PlayerStatManagerData stats)
        {
            if (!GameplaySession.HasConfig(CombatPlayerOverridePolicy.TalentsKey)) return;
            if (stats.Talents == null || stats.Talents.List == null)
                throw new InvalidOperationException("官方 Talents 不可用");
            string encoded = GameplaySession.ConfigString(CombatPlayerOverridePolicy.TalentsKey);
            if (string.IsNullOrEmpty(encoded)) return;
            foreach (string row in encoded.Split(','))
            {
                string[] columns = row.Split(':');
                int level;
                if (columns.Length != 2 || !int.TryParse(columns[1], out level))
                    throw new InvalidOperationException("赵活决斗技能配置格式错误");
                PlayerTalentData talent = stats.Talents.Get(columns[0]);
                if (talent == null || !talent.CombatSkill)
                    throw new InvalidOperationException("不是有效的原版决斗技能：" + columns[0]);
                if (!TalentSnap.ContainsKey(columns[0]))
                    TalentSnap.Add(columns[0], talent.Level);
                stats.Talents.Set(columns[0], level);
            }
        }
    }
}
