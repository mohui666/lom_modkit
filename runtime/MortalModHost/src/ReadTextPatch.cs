using System;
using System.Reflection;
using HarmonyLib;
using Lean.Localization;

namespace MortalModHost
{
    /// <summary>
    /// 契约 §6.11 防 wipe：LeanLocalization.UpdateTranslations（源码签名
    /// <c>public static void UpdateTranslations(bool forceUpdate = true)</c>，可选参数编译后即
    /// <c>UpdateTranslations(bool)</c>，用参数类型锁定匹配）会遍历 CurrentTranslations 逐个
    /// Clear()（LeanTranslation.Clear 把 Data 置 null）、CurrentTranslations.Clear()，然后只重建
    /// 各实例 RegisterAndBuild() 自带的序列化条目——运行时注册的 mod 台词被清空且不再恢复
    /// （ilspycmd 反编译实证）。每帧 Update(false) 在没有 pendingUpdates 时立即返回；
    /// Prefix 在原方法清除该字段前记录是否真的刷新，Postfix 只在真实刷新后恢复台词。
    /// </summary>
    [HarmonyPatch(typeof(LeanLocalization), "UpdateTranslations", new Type[] { typeof(bool) })]
    internal static class ReadTextPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入（patch 类是静态的，拿不到插件实例 Logger）。</summary>
        internal static BepInEx.Logging.ManualLogSource Log;

        // Cache the exact installed LeanLocalization bool field as a direct
        // getter. FieldInfo.GetValue would box a bool on every idle frame.
        private static readonly AccessTools.FieldRef<bool> PendingUpdates = CreatePendingReader();
        private static AccessTools.FieldRef<bool> CreatePendingReader()
        {
            FieldInfo field = typeof(LeanLocalization).GetField("pendingUpdates", BindingFlags.NonPublic | BindingFlags.Static);
            if (field == null || field.FieldType != typeof(bool))
                throw new MissingFieldException(typeof(LeanLocalization).FullName, "pendingUpdates: bool");
            return AccessTools.StaticFieldRefAccess<bool>(field);
        }

        private static void Prefix(bool forceUpdate, out bool __state)
        {
            __state = forceUpdate || PendingUpdates();
        }

        /// <summary>
        /// 幂等重注册全部 mod 台词。postfix 异常绝不能逃逸进 LeanLocalization（会导致官方
        /// 本地化链路崩溃），一律兜底吞掉只留警告。
        /// </summary>
        private static void Postfix(bool __state)
        {
            if (!__state) return;
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
