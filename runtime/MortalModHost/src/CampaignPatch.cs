using System;
using System.Collections.Generic;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using Mortal.Free;

namespace MortalModHost
{
    /// <summary>
    /// Harmony postfix：<c>SaveSystem.NewGameData()</c>（契约 §6.4）。
    /// 官方方法硬编码首脚本 ch1_1（Mortal.Core.decompiled.cs:4807）；点击"开始新战役"时
    /// Plugin 先把 mod 记入 <see cref="PendingCampaign"/>，这里在官方初始化完成后把首个剧情脚本
    /// 替换为该 mod 的入口注册名，再调一次 <c>SaveGameData()</c> 把修正后的状态重写进隔离槽存档
    /// （官方在 postfix 之前落盘的那份存档里首脚本仍是 ch1_1，不重写会导致读档被拉回官方序章），
    /// 最后清空挂起状态。
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
            if (mod == null) return;
            PendingCampaign = null;
            try
            {
                PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                if (stat == null)
                {
                    Log.LogError("新战役开局失败：PlayerStatManagerData 单例未就绪（mod " + mod.Id + "）");
                    return;
                }
                string registered = mod.GetRegisteredScriptName(mod.Entry);
                stat.SetStoryScript(registered);
                stat.SetStartScript(registered);
                Log.LogInfo("新战役首脚本已替换：" + registered + "（mod " + mod.Id + "）");

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
    /// 官方主线/支线优先是官方调用链天然行为：PositionController 只在无任务占用该位置时才走到本方法。
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
                if (positionId == null) return;

                List<string> storyKeys = null;
                PlayerStatManagerData stat = PlayerStatManagerData.Instance;
                if (stat != null) storyKeys = stat.StoryKeyList;

                foreach (ModPackage mod in Plugin.LoadedMods)
                {
                    if (mod.Campaign == null) continue;
                    foreach (CampaignTrigger trigger in mod.Campaign.Triggers)
                    {
                        if (!string.Equals(trigger.Position, positionId, StringComparison.Ordinal)) continue;
                        if (!trigger.IsConditionMet(storyKeys)) continue;
                        string registered = mod.GetRegisteredScriptName(trigger.Script);
                        __result = registered;
                        Log.LogInfo("位置触发器命中：" + positionId + " → " + registered);
                        return; // 先加载的 mod 优先（与注册表冲突策略一致）
                    }
                }
            }
            catch (Exception ex)
            {
                Log.LogError("位置触发器判定异常：" + ex);
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
