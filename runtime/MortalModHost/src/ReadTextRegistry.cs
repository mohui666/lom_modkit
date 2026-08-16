using System;
using System.Collections.Generic;
using Lean.Localization;

namespace MortalModHost
{
    /// <summary>
    /// 已读文本注册（契约 §6.8）：把全部 mod 包的 texts.json 条目注册进 LeanLocalization，
    /// 解析名为 "Story/" + key。官方链路（ilspycmd 反编译实证）：
    /// LuaManager.GetStoryText(key) → Mortal.Core.LeanLocalizationResolver.GetStoryText(key)
    /// → LeanLocalization.GetTranslationText("Story/" + key)，查不到时回退显示 key 本身。
    /// 注册 API：LeanLocalization.RegisterTranslation(name) 返回 LeanTranslation，其 public Data 字段
    /// 塞字符串即被 GetTranslationText 识别（Data is string 才认）。
    ///
    /// 注意：LeanLocalization.UpdateTranslations（切语言 / 实例 OnEnable/OnDisable 等）会清空
    /// CurrentTranslations 并重建，把运行时塞进去的条目抹掉。因此除 Plugin.Awake 扫描后立即注册外，
    /// LuaManagerPatch 每次演出前还会再 Apply 一次兜底；Apply 幂等（同 key 覆盖 Data）。
    /// </summary>
    internal static class ReadTextRegistry
    {
        /// <summary>LeanLocalization 解析名前缀：官方 GetStoryText 拼 "Story/" + key。</summary>
        private const string Prefix = "Story/";

        /// <summary>texts.json 原始 key → 台词文本（不含 Story/ 前缀，Apply 时拼上）。</summary>
        private static readonly Dictionary<string, string> _texts = new Dictionary<string, string>();
        private static readonly List<ModPackage> _mods = new List<ModPackage>();

        /// <summary>已收集文本条目数（日志/自检用）。</summary>
        public static int Count
        {
            get { return _texts.Count; }
        }

        /// <summary>用扫描到的 mod 包重建文本表（纯数据，不碰 Unity）。key 冲突保留先加载者（加载顺序=文件名序）。</summary>
        public static void Rebuild(IEnumerable<ModPackage> mods, Action<string> logWarn = null)
        {
            _mods.Clear();
            foreach (var mod in mods) _mods.Add(mod);
            SelectLocale(I18n.StoryLocale, logWarn);
        }

        private static void SelectLocale(string locale, Action<string> logWarn = null)
        {
            _texts.Clear();
            foreach (var mod in _mods)
            {
                foreach (var pair in mod.GetTexts(locale))
                {
                    if (_texts.ContainsKey(pair.Key))
                    {
                        if (logWarn != null)
                            logWarn("已读文本 key 冲突：" + pair.Key + "，保留先加载的包");
                        continue;
                    }
                    _texts[pair.Key] = pair.Value;
                }
            }
        }

        /// <summary>
        /// 把全部条目写入 LeanLocalization（幂等：key 已存在时覆盖 Data）。
        /// 静态方法只碰静态字典，无需场景里有 LeanLocalization 实例，任意时机可调。
        /// </summary>
        public static void Apply()
        {
            // LeanLocalization invokes this after a language switch, so select the
            // locale on every Apply rather than requiring a mod rescan.
            SelectLocale(I18n.StoryLocale);
            foreach (var pair in _texts)
            {
                LeanTranslation translation = LeanLocalization.RegisterTranslation(Prefix + pair.Key);
                if (translation != null)
                    translation.Data = pair.Value;
            }
        }

        /// <summary>
        /// 契约 §6.8 自检（Plugin.Awake 首次注册后调一次）：报告注册条数，并取第一条文本做样本
        /// 校验——GetTranslationText("Story/"+key) 与原文不一致说明注册未生效（台词会退化显示 key
        /// 本身）。只取第一条避免逐条刷日志；replaceTokens=false 保证与原文逐字符比对。
        /// </summary>
        public static void SelfCheck(Action<string> logInfo, Action<string> logWarn)
        {
            if (logInfo != null)
                logInfo("已读文本已注册：" + Count + " 条");
            if (_texts.Count == 0) return;
            using (var enumerator = _texts.GetEnumerator())
            {
                enumerator.MoveNext();
                KeyValuePair<string, string> first = enumerator.Current;
                string actual;
                try
                {
                    actual = LeanLocalization.GetTranslationText(Prefix + first.Key, null, false);
                }
                catch (Exception ex)
                {
                    if (logWarn != null)
                        logWarn("已读文本注册自检失败：" + first.Key + "（GetTranslationText 抛异常：" + ex.Message + "）");
                    return;
                }
                if (actual != first.Value)
                {
                    if (logWarn != null)
                        logWarn("已读文本注册自检失败：" + first.Key + " 解析为 " + (actual == null ? "(null)" : actual));
                }
            }
        }
    }
}
