using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// 注册名（MOD_&lt;modid&gt;_&lt;scriptid&gt;）→ Lua 源码 的查找表（纯静态、无 BepInEx/Unity 依赖，可离线单测）。
    /// 扫描完成后由 Plugin 重建一次，Harmony patch 按名命中，契约见 docs/zh_CN/mod_format.md §6.2。
    /// </summary>
    internal static class ModRegistry
    {
        private static readonly Dictionary<string, string> _luaByRegisteredName = new Dictionary<string, string>();

        private static readonly Dictionary<string, ModPackage> _packageByRegisteredName = new Dictionary<string, ModPackage>();
        private static readonly Dictionary<string, string> _scriptIdByRegisteredName = new Dictionary<string, string>();

        /// <summary>已注册脚本总数（日志/自检用）。</summary>
        public static int Count
        {
            get { return _luaByRegisteredName.Count; }
        }

        /// <summary>
        /// 用扫描到的 mod 包重建查找表。重复 id 或任一注册名冲突时按整包拒绝后加载者，
        /// 避免一个包只注册一半，或菜单选择 B 却实际执行 A 的脚本。
        /// </summary>
        public static void Rebuild(IEnumerable<ModPackage> mods, Action<string> logWarn = null)
        {
            _luaByRegisteredName.Clear();
            _packageByRegisteredName.Clear();
            _scriptIdByRegisteredName.Clear();
            var registeredIds = new HashSet<string>(StringComparer.Ordinal);
            foreach (var mod in mods)
            {
                if (mod == null || string.IsNullOrEmpty(mod.Id)) continue;
                string conflict = null;
                if (registeredIds.Contains(mod.Id))
                {
                    conflict = "重复 mod id：" + mod.Id;
                }
                else
                {
                    foreach (var pair in mod.LuaScripts)
                    {
                        string registeredName = mod.GetRegisteredScriptName(pair.Key);
                        if (_luaByRegisteredName.ContainsKey(registeredName))
                        {
                            conflict = "注册名冲突：" + registeredName;
                            break;
                        }
                    }
                }
                if (conflict != null)
                {
                    if (logWarn != null)
                        logWarn(conflict + "，已保留先加载的包，整包忽略 " + mod.Id);
                    continue;
                }

                foreach (var pair in mod.LuaScripts)
                {
                    string registeredName = mod.GetRegisteredScriptName(pair.Key);
                    _luaByRegisteredName[registeredName] = pair.Value;
                    _packageByRegisteredName[registeredName] = mod;
                    _scriptIdByRegisteredName[registeredName] = pair.Key;
                }
                registeredIds.Add(mod.Id);
            }
        }

        public static bool IsPackageFullyRegistered(ModPackage package)
        {
            if (package == null || package.LuaScripts.Count == 0) return false;
            foreach (var pair in package.LuaScripts)
            {
                ModPackage owner;
                if (!_packageByRegisteredName.TryGetValue(package.GetRegisteredScriptName(pair.Key), out owner)
                    || !object.ReferenceEquals(owner, package))
                    return false;
            }
            return true;
        }

        /// <summary>按注册名查 Lua 源码；未命中返回 false。</summary>
        public static bool TryGetLuaByRegisteredName(string registeredName, out string lua)
        {
            if (string.IsNullOrEmpty(registeredName))
            {
                lua = null;
                return false;
            }
            ModPackage package;
            string scriptId;
            if (_packageByRegisteredName.TryGetValue(registeredName, out package) &&
                _scriptIdByRegisteredName.TryGetValue(registeredName, out scriptId))
            {
                lua = package.GetLuaScript(scriptId, I18n.StoryLocale);
                return lua != null;
            }
            return _luaByRegisteredName.TryGetValue(registeredName, out lua);
        }

        /// <summary>
        /// 按注册名查所属 mod 包（契约 §3.1 结局卡背景图：mod_set_ending_text 的 image
        /// 参数按"当前演出 mod"的包内 assets 解析）。未命中返回 false。
        /// </summary>
        public static bool TryGetPackageByRegisteredName(string registeredName, out ModPackage package)
        {
            if (string.IsNullOrEmpty(registeredName))
            {
                package = null;
                return false;
            }
            return _packageByRegisteredName.TryGetValue(registeredName, out package);
        }
    }
}
