using System;
using BepInEx.Logging;
using Fungus;
using HarmonyLib;
using MoonSharp.Interpreter;
using Mortal.Story;

namespace MortalModHost
{
    /// <summary>
    /// Harmony prefix：拦截 <c>LuaManager.ExecuteLuaScript()</c>（private，Story 场景中由 Init() 调用）。
    /// 当前脚本名命中 mod 注册名（MOD_&lt;modid&gt;_&lt;scriptid&gt;）时，从内存中的 mod 包取 Lua 文本直接演出，
    /// 并跳过原方法——原方法会去 Resources 找官方脚本，MOD_ 前缀的脚本必然找不到，还会刷一条误导性报错。
    /// 官方脚本名一律放行（return true），游戏行为不受影响。
    /// </summary>
    [HarmonyPatch(typeof(LuaManager), "ExecuteLuaScript")]
    internal static class LuaManagerPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入（patch 类是静态的，拿不到插件实例的 Logger）。</summary>
        internal static ManualLogSource Log;

        private static bool Prefix(LuaManager __instance)
        {
            string scriptName = __instance.ScriptName;
            string lua;
            if (!ModRegistry.TryGetLuaByRegisteredName(scriptName, out lua))
            {
                // 契约 §6.6 兜底：MOD_ 前缀但注册表查不到（对应 mod 已删除）时，
                // 不执行原方法（去了也找不到 Resources 脚本，会报错误锁），直接回 Free 场景防软锁
                if (scriptName != null && scriptName.StartsWith("MOD_", StringComparison.Ordinal))
                {
                    Log.LogWarning("mod 脚本 " + scriptName + " 未注册（对应 mod 可能已删除），跳过演出并返回 Free 场景");
                    try
                    {
                        __instance.ChangeScene("Free", "", "");
                    }
                    catch (Exception ex)
                    {
                        Log.LogError("防软锁 ChangeScene 失败：" + ex);
                    }
                    return false;
                }
                return true; // 官方脚本，走原方法
            }

            try
            {
                // _luaEnvironment 是 private 字段（[SerializeField]），用 Harmony Traverse 取
                var env = Traverse.Create(__instance).Field("_luaEnvironment").GetValue<LuaEnvironment>();
                if (env == null)
                {
                    Log.LogError("mod 脚本 " + scriptName + " 无法演出：LuaManager._luaEnvironment 为 null");
                    return false;
                }
                // 与原方法保持一致：friendlyName = 脚本名 + ".LuaScript"，协程方式运行
                Closure fn = env.LoadLuaFunction(lua, scriptName + ".LuaScript");
                env.RunLuaFunction(fn, true);
                Log.LogInfo("mod 脚本开始演出：" + scriptName);
            }
            catch (Exception ex)
            {
                Log.LogError("mod 脚本 " + scriptName + " 演出失败：" + ex);
            }
            return false; // 该脚本名归 mod 管，成败都跳过原方法
        }
    }
}
