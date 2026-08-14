using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 一个已解析的 .lommod 包：manifest 元信息 + lua/ 目录全部脚本（内存常驻，不落盘）。
    /// 契约见 docs/mod_format.md §1/§2。
    /// </summary>
    internal sealed class ModPackage
    {
        /// <summary>mod 唯一 id（manifest.id），注册脚本时用作前缀防冲突。</summary>
        public string Id;

        /// <summary>显示名（manifest.name）。</summary>
        public string Name;

        /// <summary>版本号（manifest.version）。</summary>
        public string Version;

        /// <summary>作者（manifest.author，可空）。</summary>
        public string Author;

        /// <summary>简介（manifest.description，可空）。</summary>
        public string Description;

        /// <summary>入口剧情脚本 id（manifest.entry），必须存在于 <see cref="LuaScripts"/>。</summary>
        public string Entry;

        /// <summary>lua/ 目录全部脚本：键 = 脚本 id（文件名去 .lua），值 = Lua 源码文本。</summary>
        public readonly Dictionary<string, string> LuaScripts = new Dictionary<string, string>();

        /// <summary>
        /// texts.json（可选，契约 §1）：键 = "MOD_&lt;modid&gt;_&lt;scriptid&gt;_&lt;nodeid&gt;"，值 = 台词文本。
        /// 运行时注册进 LeanLocalization（解析名 "Story/" + key），让 mod 台词获得官方已读变黄/可快进能力。
        /// </summary>
        public readonly Dictionary<string, string> Texts = new Dictionary<string, string>();

        /// <summary>战役模式配置（manifest.campaign，契约 §2）；null 表示本包无战役模式。</summary>
        public CampaignConfig Campaign;

        /// <summary>
        /// assets/ 目录下的图片（契约 §3.1）：键 = 包内相对路径（"assets/xxx.png"，正斜杠），
        /// 值 = 原始字节（仅 .png/.jpg/.jpeg，单张 ≤8MB，超限的加载时已警告跳过）。
        /// 运行时结局卡背景图按 Lua 传来的路径在此查表，Texture2D 解码由 Unity 侧完成。
        /// </summary>
        public readonly Dictionary<string, byte[]> Assets = new Dictionary<string, byte[]>();

        /// <summary>
        /// 包内用户内容（契约：assets/user/&lt;type&gt;/&lt;id&gt;/）。
        /// 键 = 裸内容 ID（如 mohui.boss_theme），只含本包资源。
        /// </summary>
        public readonly Dictionary<string, UserContent> UserContents = new Dictionary<string, UserContent>();

        /// <summary>.lommod 文件完整路径（仅用于日志定位，内容已全部读入内存）。</summary>
        public string PackagePath;

        /// <summary>只解析本包内的用户内容；找不到返回 false。绝不回读编辑器仓库。</summary>
        public bool TryGetUserContent(string contentId, out UserContent content)
        {
            if (string.IsNullOrEmpty(contentId))
            {
                content = null;
                return false;
            }
            return UserContents.TryGetValue(contentId, out content);
        }

        /// <summary>
        /// 按契约 §6.1 生成注册到游戏 LuaManager 的脚本名：MOD_&lt;modid&gt;_&lt;scriptid&gt;。
        /// </summary>
        public string GetRegisteredScriptName(string scriptId)
        {
            return "MOD_" + Id + "_" + scriptId;
        }
    }
}
