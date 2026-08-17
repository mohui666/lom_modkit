using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using HarmonyLib;
using Mortal.Combat;
using Mortal.Core;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;
using UnityEngine.UI;
using UnityObject = UnityEngine.Object;

namespace MortalModHost
{
    /// <summary>
    /// 为每次 MOD 决斗即时创建隔离的 CombatLevel。原版 CL 不再作为本场人物、
    /// 背景、数值、事件或结果的配置来源；只从一个可运行条目保留引擎必须的音效/
    /// 音乐对象引用。官方人物身份通过 EnemyStat.Name 的真实关系独立绑定。
    /// </summary>
    [HarmonyPatch(typeof(CombatLevelConfig), "Get")]
    internal static class CombatRuntimeLevelPatch
    {
        internal const string RuntimeLevelName = "CL_MORTALMODHOST_RUNTIME";
        private static CombatLevel _runtimeLevel;
        private static CombatStat _runtimeStat;
        private static CombatTalkingDialog _emptyDialog;

        private static bool Prefix(CombatLevelConfig __instance, string name, ref CombatLevel __result)
        {
            if (!GameplaySession.PendingCombat
                || !string.Equals(name, RuntimeLevelName, StringComparison.Ordinal))
                return true;
            try
            {
                Clear();
                __result = Build(__instance);
                return false;
            }
            catch (Exception ex)
            {
                GameplayOverrideFailure.Abort("决斗运行配置", ex);
                __result = null;
                return false;
            }
        }

        private static CombatLevel Build(CombatLevelConfig config)
        {
            if (config == null || config.List == null)
                throw new InvalidOperationException("CombatLevelConfig 不可用");
            string character = GameplaySession.ConfigString("character");
            CombatLevel source = null;
            for (int i = 0; i < config.List.Count; i++)
            {
                CombatLevel candidate = config.List[i];
                if (candidate == null || candidate.EnemyStat == null
                    || candidate.MusicData == null) continue;
                if (!CombatCharacterPolicy.IsUserCharacter(character)
                    && string.Equals(candidate.EnemyStat.Name, character, StringComparison.Ordinal)
                    && candidate.EnemyStat.CombatAvatar != null)
                {
                    source = candidate;
                    break;
                }
                if (source == null) source = candidate;
            }
            if (source == null)
                throw new InvalidOperationException("找不到可用于创建临时 CombatLevel 的原版依赖");

            _runtimeLevel = UnityObject.Instantiate(source);
            _runtimeLevel.name = RuntimeLevelName;
            _runtimeStat = UnityObject.Instantiate(source.EnemyStat);
            _runtimeStat.name = RuntimeLevelName + "_STAT";
            ResetAuthorFacingState(_runtimeStat);
            _emptyDialog = ScriptableObject.CreateInstance<CombatTalkingDialog>();
            _emptyDialog.NormalTalk = EmptyStrings();
            _emptyDialog.GoodTalk = EmptyStrings();
            _emptyDialog.BadTalk = EmptyStrings();
            _emptyDialog.StopAction = EmptyStrings();
            _emptyDialog.StopTalk = EmptyStrings();
            _emptyDialog.TalkAttacked = EmptyStrings();
            _emptyDialog.AutoAttack = EmptyStrings();
            _emptyDialog.NoAction = EmptyStrings();

            Traverse fields = Traverse.Create(_runtimeLevel);
            fields.Field("_desc").SetValue("MortalModHost runtime combat");
            // 正常路径会在同一加载协程内换成作者选择的独立背景。保留这个值只作
            // Addressables/内嵌映射损坏时的无崩溃兜底，不参与正常配置选择。
            fields.Field("_deadEnd").SetValue(false);
            fields.Field("_deadLibrary").SetValue(null);
            fields.Field("_enemyStat").SetValue(_runtimeStat);
            fields.Field("_enemyTalking").SetValue(_emptyDialog);
            fields.Field("_playerTalking").SetValue(_emptyDialog);
            fields.Field("_talkingEvent").SetValue(null);
            fields.Field("_winResult").SetValue(new CombatResultFlag[0]);
            fields.Field("_loseResult").SetValue(new CombatResultFlag[0]);
            _runtimeLevel.Events = new List<CombatEventData>();
            LuaManagerPatch.Log?.LogInfo(
                "已即时创建隔离 CombatLevel；未继承任何固定 CL 的背景、事件、结果或作者数值");
            return _runtimeLevel;
        }

        private static void ResetAuthorFacingState(CombatStat stat)
        {
            stat.MaxHealth = 100;
            stat.DefaultHealth = 100;
            stat.MaxStamina = 100;
            stat.DefaultStamina = 100;
            stat.Strength = 0;
            stat.Stamina = 0;
            stat.Dexterity = 0;
            stat.Talking = 0;
            stat.Defence = 0;
            stat.Sword = 0;
            stat.Fist = 0;
            stat.MartialWeapon = 0;
            stat.Internal = 0;
            stat.Mental = 0;
            stat.Disposition = 0;
            stat.Training = 0;
            // 原版以 50 为道德中性分界；未填写时不能把对手默认为恶人。
            stat.Karma = 50;
            stat.Behaviour = 0;
            stat.PoisonResist = 0;
            stat.ParalyzedResist = 0;
            stat.WeaponPoisonValue = 0;
            stat.WeaponParalyzedValue = 0;
            stat.Confucianism = 0;
            stat.Buddhism = 0;
            stat.Taoism = 0;
            stat.Xingyi = 0;
            stat.StrategyLevel = 0;
            stat.WeaponHitAddition = 0;
            stat.WeaponDamageAddition = 0;
            stat.WeaponDiceAddition = 0;
            stat.AttackDamageAddition = 0;
            stat.AttackDiceAddition = 0;
            stat.HealthAddition = 0;
            stat.BlockAbsorbAddition = 0f;
            stat.BlockDodgeAddition = 0f;
            stat.BlockParryAddition = 0f;
            stat.AttackParryAddition = 0f;
            // GetUltimateDamage 最后直接乘此倍率，0 会让旧 Story 的绝招恒为零伤害。
            stat.UltimateDamageRate = 1f;
            stat.DefenceAddition = 0;
            stat.IgnoreNormalMode = false;
            stat.TalentItems = new List<CombatTalentItem>();
            stat.TalkRate = 0f;
            stat.AttackRate = 1f;
            stat.WeaponkRate = 0f;
            stat.UltimateRate = 0f;
            stat.BlockRate = 0f;
            stat.Reset();
        }

        private static string[] EmptyStrings() { return new string[0]; }

        internal static void Clear()
        {
            if (_runtimeLevel != null) UnityObject.Destroy(_runtimeLevel);
            if (_runtimeStat != null) UnityObject.Destroy(_runtimeStat);
            if (_emptyDialog != null) UnityObject.Destroy(_emptyDialog);
            _runtimeLevel = null;
            _runtimeStat = null;
            _emptyDialog = null;
        }
    }

    /// <summary>
    /// 从构建时提取并内嵌的官方 view 映射解析 Addressables 地址。不能依赖
    /// StoryViewImage 实例：章节首节点/试玩直达时它可能尚未创建。
    /// </summary>
    [HarmonyPatch(typeof(CombatManager), "LoadCombatLevelAsset")]
    internal static class CombatBackgroundOverridePatch
    {
        private static string _address;
        private static AsyncOperationHandle<Sprite> _handle;
        private static bool _hasHandle;
        private static Dictionary<string, string> _addresses;

        internal static void Capture()
        {
            _address = null;
            try
            {
                string background = GameplaySession.ConfigString("background");
                if (string.IsNullOrWhiteSpace(background))
                    throw new InvalidOperationException("Combat 缺少独立 background");
                EnsureAddressMap();
                if (!_addresses.TryGetValue(background, out _address)
                    || string.IsNullOrEmpty(_address))
                    throw new InvalidOperationException("找不到官方背景：" + background);
                LuaManagerPatch.Log?.LogInfo(
                    "Combat 独立背景已绑定：" + background + " / " + _address);
            }
            catch (Exception ex)
            {
                // 背景是显示层；映射/Addressables 故障不得再让决斗节点抛出 Lua
                // 异常或把玩家踢回 Free。Combat 会保留临时关卡的可用背景。
                _address = null;
                LuaManagerPatch.Log?.LogError(
                    "Combat 独立背景解析失败；保留可用背景继续决斗：" + ex);
            }
        }

        private static void EnsureAddressMap()
        {
            if (_addresses != null) return;
            var parsed = new Dictionary<string, string>(StringComparer.Ordinal);
            Assembly assembly = Assembly.GetExecutingAssembly();
            using (Stream stream = assembly.GetManifestResourceStream("MortalModHost.view_map.json"))
            {
                if (stream == null) throw new InvalidOperationException("内嵌官方 view 映射缺失");
                using (var reader = new StreamReader(stream))
                {
                    var root = MiniJson.Parse(reader.ReadToEnd()) as Dictionary<string, object>;
                    if (root == null) throw new InvalidOperationException("内嵌官方 view 映射格式错误");
                    foreach (KeyValuePair<string, object> pair in root)
                    {
                        var item = pair.Value as Dictionary<string, object>;
                        object raw;
                        string address = item != null && item.TryGetValue("address", out raw)
                            ? raw as string : null;
                        if (!CombatBackgroundAddressPolicy.IsOfficialImageAddress(address))
                            throw new InvalidOperationException("官方 view 地址越界：" + pair.Key);
                        parsed.Add(pair.Key, address);
                    }
                }
            }
            if (parsed.Count == 0) throw new InvalidOperationException("内嵌官方 view 映射为空");
            _addresses = parsed;
        }

        private static void Postfix(CombatManager __instance, ref IEnumerator __result)
        {
            if (!GameplaySession.PendingCombat || __instance == null) return;
            __result = Wrap(__instance, __result);
        }

        private static IEnumerator Wrap(CombatManager manager, IEnumerator original)
        {
            while (original != null && original.MoveNext()) yield return original.Current;
            if (string.IsNullOrEmpty(_address))
            {
                LuaManagerPatch.Log?.LogWarning("Combat 独立背景不可用；保留可用背景继续决斗");
                yield break;
            }
            _handle = Addressables.LoadAssetAsync<Sprite>(_address);
            _hasHandle = true;
            if (!_handle.IsDone) yield return _handle;
            if (_handle.Status != AsyncOperationStatus.Succeeded || _handle.Result == null)
            {
                LuaManagerPatch.Log?.LogError(
                    "Combat 背景载入失败；保留可用背景继续决斗：" + _address);
                yield break;
            }
            Traverse fields = Traverse.Create(manager);
            SpriteRenderer backSprite = fields.Field("_backSprite").GetValue<SpriteRenderer>();
            Image backImage = fields.Field("_backImage").GetValue<Image>();
            if (backSprite == null || backImage == null)
            {
                LuaManagerPatch.Log?.LogError("Combat 背景渲染器不可用；继续决斗");
                yield break;
            }
            backSprite.sprite = _handle.Result;
            backImage.sprite = _handle.Result;
            LuaManagerPatch.Log?.LogInfo("Combat 已应用独立背景：" + _address);
        }

        internal static void Clear()
        {
            if (_hasHandle) Addressables.Release(_handle);
            _hasHandle = false;
            _address = null;
        }
    }
}
