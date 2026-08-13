using System;
using System.Collections.Generic;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using Mortal.Free;
using OBB.Framework.Utils;

namespace MortalModHost
{
    /// <summary>
    /// Harmony postfix：<c>SaveSystem.NewGameData()</c>（契约 §6.4）。
    /// 官方方法硬编码首脚本 ch1_1（Mortal.Core.decompiled.cs:4807）；点击"开始新战役"时
    /// Plugin 先把 mod 记入 <see cref="PendingCampaign"/>，这里在官方初始化完成后把首个剧情脚本
    /// 替换为该 mod 的入口注册名，再调一次 <c>SaveGameData()</c> 把修正后的状态重写进隔离槽存档
    /// （官方在 postfix 之前落盘的那份存档里首脚本仍是 ch1_1，不重写会导致读档被拉回官方序章），
    /// 最后清空挂起状态。
    /// 无 mod 挂起（玩家用官方方式开局）时清除 <see cref="ModCampaignState"/>（契约 §2
    /// disable_official_events 的清除时机）；mod 战役开局失败（单例未就绪）同样回退为无战役态。
    /// 注意 LuaManagerPatch 官方脚本分支不重置该状态（战役期间可能穿插官方脚本演出）。
    /// </summary>
    [HarmonyPatch(typeof(SaveSystem), "NewGameData")]
    internal static class NewGameDataPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        /// <summary>待开局的战役 mod；null 表示本次 NewGameData 是官方正常开局，不干预。</summary>
        internal static ModPackage PendingCampaign;

        private static void Postfix(SaveSystem __instance)
        {
            ModPackage mod = PendingCampaign;
            if (mod == null)
            {
                // 官方方式开局（无 mod 挂起）：上一场 mod 战役结束，禁用原版事件状态随之清除。
                // （LuaManagerPatch 官方脚本分支不重置本状态——战役期间可能穿插官方脚本演出。）
                ModCampaignState.Clear();
                return;
            }
            PendingCampaign = null;
            try
            {
                PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                if (stat == null)
                {
                    Log.LogError("新战役开局失败：PlayerStatManagerData 单例未就绪（mod " + mod.Id + "）");
                    ModCampaignState.Clear(); // 战役未真正开始，回退为无 mod 战役态（不再抑制原版事件）
                    return;
                }
                string registered = mod.GetRegisteredScriptName(mod.Entry);
                stat.SetStoryScript(registered);
                stat.SetStartScript(registered);
                Log.LogInfo("新战役首脚本已替换：" + registered + "（mod " + mod.Id + "）");

                // 契约 §D：mod 新战役发放 2 点命运（GameStatType.命运，StringValue "fate"），
                // 保证全场景骰子 DiceMenuDialog.CheckRevolution（Stats.Get(命运).FinalValue > 0）
                // 在 mod 战役里也能走逆天流程（官方新游戏有命运点，mod 隔离存档初始为 0）。
                try
                {
                    GameStat fate = stat.Stats.Get(GameStatType.命運);
                    if (fate != null)
                    {
                        fate.AddValue(2);
                        Log.LogInfo("mod 新战役已发放 2 点命运（当前值 " + fate.Value + "）");
                    }
                    else
                    {
                        Log.LogWarning("mod 新战役发放命运点失败：Stats 中没有 命運 属性");
                    }
                }
                catch (Exception ex)
                {
                    Log.LogWarning("mod 新战役发放命运点失败：" + ex.Message);
                }

                // 存档污染修复：NewGameData 内部在本 postfix 之前已 CreateSaveData() 落盘，
                // 存档里 StartStoryScript 仍是官方 ch1_1（读档会被拉回官方序章）。
                // 这里重存一次当前槽（_currentSlot 已是 "mod_<modid>" 隔离槽）。
                // 无递归风险：SaveGameData 只做 CreateSaveData+ExecuteSaveData，不会回调 NewGameData；
                // 双写也只是同槽顺序覆盖（先官方 ch1_1 后本修正），无并发冲突。
                __instance.SaveGameData();
                Log.LogInfo("隔离槽存档已重写，StartStoryScript = " + registered);
            }
            catch (Exception ex)
            {
                Log.LogError("新战役首脚本替换异常（mod " + mod.Id + "）：" + ex);
            }
        }
    }

    /// <summary>
    /// Harmony postfix：<c>FreePositionData.GetExecuteScript(float)</c>（契约 §6.5）。
    /// 命中 manifest.triggers（position 匹配 + flag 条件满足）时把返回值替换为 mod 脚本注册名。
    /// 有活跃战役时只匹配当前战役 mod 的触发器（契约 §2「只保留本 mod 的位置触发器」）；无战役时
    /// 全部 mod、先加载者优先。禁用原版剧情时，PositionClickStorySuppressionPatch
    /// 会先让官方点击链跳过主线/支线，因此 mod 位置触发器仍能在本方法命中。
    /// mod 战役声明 disable_official_events（<see cref="ModCampaignState"/>）或 F7 全局临时开关
    /// （<see cref="VanillaStorySwitch"/>）且未命中任何 mod 触发器时，把返回值置 null 抑制官方默认
    /// 故事脚本。mod 触发器匹配在抑制判定前执行，因此命中仍优先；无 mod 命中时返回 null。
    /// 开关关闭后一切恢复原状，战役级开关语义不变。反编译结论（ilspycmd，Mortal.Free.dll）：唯一调用方
    /// PositionController.OnPositionClick 拿到返回值先 <c>string.IsNullOrEmpty(_scriptName)</c> 判断，
    /// 空值走 Debug.Log("自由模式：X, 无脚本") 分支——不扣行动点、不 SetStoryScript、不切场景，
    /// 是安全 no-op（官方 SetConditionFlag 在 GetExecuteScript 之前执行，属位置自身状态刷新，无剧情副作用）。
    /// </summary>
    [HarmonyPatch(typeof(FreePositionData), "GetExecuteScript")]
    internal static class FreePositionPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        /// <summary>FreePositionData 实例 → 契约 position id（Center 等）。懒加载缓存，miss 时重建。</summary>
        private static readonly Dictionary<FreePositionData, string> PositionIdByData =
            new Dictionary<FreePositionData, string>();

        private static void Postfix(FreePositionData __instance, ref string __result)
        {
            try
            {
                string positionId = GetPositionId(__instance);
                if (positionId != null)
                {
                    List<string> storyKeys = null;
                    PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                    if (stat != null) storyKeys = stat.StoryKeyList;

                    foreach (ModPackage mod in Plugin.LoadedMods)
                    {
                        if (mod.Campaign == null) continue;
                        // 契约 §2：有活跃战役时位置触发器只匹配当前战役 mod——否则其他已安装
                        // mod 的无条件触发器会跨战役抢占（实证：showcase 的无条件 Center 兜底
                        // 触发器永远抢在 snack_case 战役的 Center→clue 之前命中）。无战役时维持
                        // 全部 mod、先加载者优先（与注册表冲突策略一致）。
                        if (ModCampaignState.Active
                            && !string.Equals(mod.Id, ModCampaignState.ActiveModId, StringComparison.Ordinal))
                            continue;
                        foreach (CampaignTrigger trigger in mod.Campaign.Triggers)
                        {
                            if (!string.Equals(trigger.Position, positionId, StringComparison.Ordinal)) continue;
                            if (!trigger.IsConditionMet(storyKeys)) continue;
                            if (!IsTimeAndAffinityMet(trigger)) continue;
                            string registered = mod.GetRegisteredScriptName(trigger.Script);
                            __result = registered;
                            Log.LogInfo("位置触发器命中：" + positionId + " → " + registered);
                            return; // 先加载的 mod 优先（与注册表冲突策略一致）
                        }
                    }
                }

                // 契约 §2：mod 战役声明 disable_official_events，或 F7 全局临时开关
                // （VanillaStorySwitch.Enabled，会话级、不依赖战役）生效时，且没有任何 mod 触发器
                // 命中（触发器匹配在上方先执行，命中即提前 return），抑制该位置的官方默认故事脚本。
                // 全局开关语义：只压「无 mod 触发器命中时的官方默认故事脚本」，关闭后一切恢复原状。
                // 反编译结论（ilspycmd，Mortal.Free.dll PositionController.OnPositionClick）：
                // 调用方拿到返回值先判 string.IsNullOrEmpty，空值走 Debug.Log 分支——安全 no-op，
                // 不消耗行动点、不设置剧情脚本、不切场景。
                if (VanillaStorySwitch.ShouldSuppress)
                {
                    if (!string.IsNullOrEmpty(__result))
                    {
                        string source = VanillaStorySwitch.Enabled ? "F7 全局开关" : "战役开关";
                        Log.LogInfo(source + "禁用原版事件：位置 " + (positionId ?? "?") + " 的官方默认脚本已抑制");
                    }
                    __result = null;
                }
            }
            catch (Exception ex)
            {
                Log.LogError("位置触发器判定异常：" + ex);
            }
        }

        /// <summary>
        /// 触发器时间/好感条件（契约 §2.1，与 flag 条件全部 AND）：
        /// <list type="bullet">
        /// <item>when_month/when_stage：PlayerStatManagerData.GameTime（GameTime.Month 字段 +
        ///   Stage 枚举，MonthStageType 上旬=1/中旬=2/下旬=3，ilspycmd 实测与 ConvertToRounds 的 (int)Stage 一致）。</item>
        /// <item>when_affinity：EnumUtils.TryParseByStringValue&lt;RelationshipStatType&gt;(character)（匹配
        ///   StringValue 契约 id，如 brother4=四師兄）→ Relationships.Get(type).Value（RelationshipStat 当前值
        ///   属性，IGameStat 是空标记接口不提供取值）≥ min。</item>
        /// </list>
        /// API 拿不到/异常一律按条件不满足处理（返回 false），不抛异常。数组顺序即优先级：
        /// 外层循环按 manifest.triggers 数组顺序逐个判定，第一个条件全部命中的触发器生效。
        /// </summary>
        private static bool IsTimeAndAffinityMet(CampaignTrigger trigger)
        {
            if (trigger.WhenMonth == null && trigger.WhenStage == null && trigger.WhenAffinity == null)
                return true;
            try
            {
                PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                if (trigger.WhenMonth != null || trigger.WhenStage != null)
                {
                    if (stat == null) return false;
                    GameTime time = stat.GameTime;
                    if (time == null) return false;
                    if (trigger.WhenMonth != null && time.Month != trigger.WhenMonth.Value) return false;
                    if (trigger.WhenStage != null && (int)time.Stage != trigger.WhenStage.Value) return false;
                }
                if (trigger.WhenAffinity != null)
                {
                    if (stat == null) return false;
                    RelationshipStatType type;
                    if (!EnumUtils.TryParseByStringValue<RelationshipStatType>(trigger.WhenAffinity.Character, out type))
                        return false;
                    RelationshipStat relationship = stat.Relationships.Get(type);
                    if (relationship == null) return false;
                    if (relationship.Value < trigger.WhenAffinity.Min) return false;
                }
                return true;
            }
            catch (Exception ex)
            {
                Log.LogWarning("触发器时间/好感条件评估失败（按不满足处理）：" + ex.Message);
                return false;
            }
        }

        /// <summary>查缓存；miss 时重建整表（Free 场景 PositionController 集合随场景变化）。</summary>
        private static string GetPositionId(FreePositionData data)
        {
            string id;
            if (PositionIdByData.TryGetValue(data, out id))
                return id;
            RebuildMap();
            return PositionIdByData.TryGetValue(data, out id) ? id : null;
        }

        /// <summary>
        /// 扫描 Free 场景全部 PositionController，Traverse 读 private 字段
        /// _positionData（FreePositionData）与 _position（PositionType，成员为中文名）。
        /// </summary>
        private static void RebuildMap()
        {
            PositionIdByData.Clear();
            foreach (PositionController controller in UnityEngine.Object.FindObjectsOfType<PositionController>())
            {
                var traverse = Traverse.Create(controller);
                FreePositionData data = traverse.Field("_positionData").GetValue<FreePositionData>();
                if (data == null) continue;
                PositionType position = traverse.Field("_position").GetValue<PositionType>();
                string id = PositionNameMap.ToContractId(position.ToString());
                if (id != null)
                    PositionIdByData[data] = id;
            }
            if (Log != null)
                Log.LogInfo("位置触发器映射已重建：" + PositionIdByData.Count + " 个位置");
        }
    }
}
