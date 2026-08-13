using System;
using HarmonyLib;
using Lean.Localization;

namespace MortalModHost
{
    /// <summary>
    /// 契约 §A 防 wipe：LeanLocalization.UpdateTranslations（源码签名
    /// <c>public static void UpdateTranslations(bool forceUpdate = true)</c>，可选参数编译后即
    /// <c>UpdateTranslations(bool)</c>，用参数类型锁定匹配）会遍历 CurrentTranslations 逐个
    /// Clear()（LeanTranslation.Clear 把 Data 置 null）、CurrentTranslations.Clear()，然后只重建
    /// 各实例 RegisterAndBuild() 自带的序列化条目——运行时注册的 mod 台词被清空且不再恢复
    /// （ilspycmd 反编译实证）。切语言、LeanLocalization 实例 OnEnable/OnDisable、以及每帧
    /// Update(forceUpdate:false) 都会调到它，故 postfix 里幂等重跑 ReadTextRegistry.Apply()
    /// 兜底（百条以内的字典写入，开销可忽略）。目标是静态方法，postfix 不带 __instance。
    /// </summary>
    [HarmonyPatch(typeof(LeanLocalization), "UpdateTranslations", new Type[] { typeof(bool) })]
    internal static class ReadTextPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入（patch 类是静态的，拿不到插件实例 Logger）。</summary>
        internal static BepInEx.Logging.ManualLogSource Log;

        /// <summary>
        /// 幂等重注册全部 mod 台词。postfix 异常绝不能逃逸进 LeanLocalization（会导致官方
        /// 本地化链路崩溃），一律兜底吞掉只留警告。
        /// </summary>
        private static void Postfix()
        {
            try
            {
                ReadTextRegistry.Apply();
            }
            catch (Exception ex)
            {
                if (Log != null)
                    Log.LogWarning("UpdateTranslations 后已读文本重注册失败（mod 台词将退化为裸文本）：" + ex.Message);
            }
        }
    }
}
