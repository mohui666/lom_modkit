using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 注册名（MOD_&lt;modid&gt;_&lt;scriptid&gt;）→ Lua 源码 的查找表（纯静态、无 BepInEx/Unity 依赖，可离线单测）。
    /// 扫描完成后由 Plugin 重建一次，Harmony patch 按名命中，契约见 docs/mod_format.md §6.3。
    /// </summary>
    internal static class ModRegistry
    {
        private static readonly Dictionary<string, string> _luaByRegisteredName = new Dictionary<string, string>();

        /// <summary>已注册脚本总数（日志/自检用）。</summary>
        public static int Count
        {
            get { return _luaByRegisteredName.Count; }
        }

        /// <summary>用扫描到的 mod 包重建查找表。注册名冲突时保留先加载者（加载顺序=文件名序）并告警。</summary>
        public static void Rebuild(IEnumerable<ModPackage> mods, Action<string> logWarn = null)
        {
            _luaByRegisteredName.Clear();
            foreach (var mod in mods)
            {
                foreach (var pair in mod.LuaScripts)
                {
                    string registeredName = mod.GetRegisteredScriptName(pair.Key);
                    if (_luaByRegisteredName.ContainsKey(registeredName))
                    {
                        if (logWarn != null)
                            logWarn("注册名冲突：" + registeredName + "，已保留先加载的包，忽略 " + mod.Id);
                        continue;
                    }
                    _luaByRegisteredName[registeredName] = pair.Value;
                }
            }
        }

        /// <summary>按注册名查 Lua 源码；未命中返回 false。</summary>
        public static bool TryGetLuaByRegisteredName(string registeredName, out string lua)
        {
            if (string.IsNullOrEmpty(registeredName))
            {
                lua = null;
                return false;
            }
            return _luaByRegisteredName.TryGetValue(registeredName, out lua);
        }
    }
}
