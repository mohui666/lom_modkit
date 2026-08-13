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
                // 官方脚本：复位心情气泡硬控状态（契约 §6.10，官方演出不受 mod 影响）；
                // 清空 mod 死亡/结局文本覆盖（契约 §6.13，官方结局不受影响）
                MoodControl.Disabled = false;
                ModOverlay.Clear();
                CharacterIntroSupport.Clear();
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

                // 契约 §3.1：结局卡背景图按"当前演出 mod"解析——开演前把包写入 ModOverlay.CurrentPackage
                ModPackage package;
                if (ModRegistry.TryGetPackageByRegisteredName(scriptName, out package))
                {
                    ModOverlay.CurrentPackage = package;
                }
                CharacterIntroSupport.Clear();

                // 契约 §6.9/§6.10：演出前把 mod_hide_mood / mod_set_mood 注册进共享 MoonSharp 环境（幂等重设；官方脚本不调用它们，无副作用）
                RegisterModGlobals(env);
                // 契约 §6.11 兜底：LeanLocalization 切语言/OnEnable 会清空 CurrentTranslations，每次演出前重注册一遍
                try
                {
                    ReadTextRegistry.Apply();
                }
                catch (Exception ex)
                {
                    Log.LogWarning("演出前已读文本重注册失败（mod 台词将退化为裸文本）：" + ex);
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

        /// <summary>
        /// 契约 §6.9/§6.10/§6.13：把 mod 全局函数注册进共享 Lua 环境的全局表（编译器在 mod Lua 里发射裸全局调用）。
        /// mod_hide_mood()：即时隐藏全部圆形情绪面板（实现收敛在 MoodControl.HideAllMoodPanels）。
        /// mod_set_mood(bool)：把心情气泡硬控状态写入 MoodControl.Disabled = !value；
        /// 缺参按 false（禁用气泡）；非布尔参数按契约同样视为缺省 false。
        /// mod_set_death_text(title, desc)：死亡文本两段式覆盖（ModOverlay，短标题 + 多行描述，
        /// GameOver 画面官方同款布局显示）；单参调用按旧契约当 desc、title 留空（标题栏保持官方清空态）。
        /// mod_set_ending_text(title, desc)：结局卡片覆盖（ModOverlay，End 画面显示）。
        /// </summary>
        private static void RegisterModGlobals(LuaEnvironment env)
        {
            try
            {
                Script script = env.Interpreter;
                if (script == null)
                {
                    Log.LogWarning("LuaEnvironment.Interpreter 为 null，跳过 mod 全局函数注册");
                    return;
                }
                script.Globals["mod_hide_mood"] = new CallbackFunction((ctx, args) =>
                {
                    MoodControl.HideAllMoodPanels();
                    return DynValue.Nil;
                }, "mod_hide_mood");
                script.Globals["mod_set_mood"] = new CallbackFunction((ctx, args) =>
                {
                    bool show = false;
                    try
                    {
                        if (args.Count > 0) show = args[0].CastToBool();
                    }
                    catch
                    {
                        show = false; // 非布尔参数按契约缺省处理：禁用气泡
                    }
                    MoodControl.Disabled = !show;
                    return DynValue.Nil;
                }, "mod_set_mood");
                // 契约 §6.13：死亡文本（2 参：短标题 + 多行描述，GameOver 画面两段式显示）。
                // 单参调用兼容（旧编译器/老 mod 包）：参数当描述、标题留空。
                script.Globals["mod_set_death_text"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        string title = "";
                        string desc = "";
                        if (args.Count == 1)
                        {
                            desc = args[0].CastToString();
                        }
                        else if (args.Count >= 2)
                        {
                            title = args[0].CastToString();
                            desc = args[1].CastToString();
                        }
                        ModOverlay.SetDeathText(title, desc);
                    }
                    catch (Exception ex)
                    {
                        Log.LogWarning("mod_set_death_text 参数转换失败（按空文本处理）：" + ex.Message);
                        ModOverlay.SetDeathText("", "");
                    }
                    return DynValue.Nil;
                }, "mod_set_death_text");
                // 契约 §6.13/§3.1：结局卡片（2/3 参：标题 + 描述 [+ 包内图片路径]，End 画面显示）；参数转换异常吞掉
                script.Globals["mod_set_ending_text"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        string title = args.Count > 0 ? args[0].CastToString() : "";
                        string desc = args.Count > 1 ? args[1].CastToString() : "";
                        string image = args.Count > 2 ? args[2].CastToString() : "";
                        ModOverlay.SetEnding(title, desc, image);
                    }
                    catch (Exception ex)
                    {
                        Log.LogWarning("mod_set_ending_text 参数转换失败（本次调用忽略）：" + ex.Message);
                    }
                    return DynValue.Nil;
                }, "mod_set_ending_text");
                // 自定义人物介绍卡：下一次特殊 key 的 intropanel.Show 由
                // CharacterIntroPanelPatch 接管；官方人物节点不调用本函数。
                script.Globals["mod_prepare_character_intro"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        string title = args.Count > 0 ? args[0].CastToString() : "";
                        string name = args.Count > 1 ? args[1].CastToString() : "";
                        string intro = args.Count > 2 ? args[2].CastToString() : "";
                        string image = args.Count > 3 ? args[3].CastToString() : "";
                        float imageScale = args.Count > 4 && args[4].Type == DataType.Number
                            ? (float)args[4].Number : 100f;
                        float imageX = args.Count > 5 && args[5].Type == DataType.Number
                            ? (float)args[5].Number : 0f;
                        float imageY = args.Count > 6 && args[6].Type == DataType.Number
                            ? (float)args[6].Number : 0f;
                        CharacterIntroSupport.Prepare(
                            title, name, intro, image, imageScale, imageX, imageY);
                    }
                    catch (Exception ex)
                    {
                        Log.LogWarning("mod_prepare_character_intro 参数转换失败：" + ex.Message);
                        CharacterIntroSupport.Clear();
                    }
                    return DynValue.Nil;
                }, "mod_prepare_character_intro");
            }
            catch (Exception ex)
            {
                Log.LogWarning("注册 mod 全局函数失败：" + ex);
            }
        }
    }
}
