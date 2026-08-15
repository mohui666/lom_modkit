using System;
using System.Collections;
using System.Collections.Generic;
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

        // Only the fixed F5 preview owns these references. They let a subsequent F5
        // request stop the old host coroutine before its Story scene is reloaded.
        private static LuaEnvironment _activeDevelopmentEnvironment;
        private static LuaManager _activeDevelopmentManager;

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
                CustomAudioPlayer.StopEverything();
                CustomCharacterRuntime.HideAllOnStage();
                CustomImageRuntime.ClearAll();
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
                RuntimeTrace.BeginScript(package, scriptName);
                CharacterIntroSupport.Clear();
                CustomAudioPlayer.StopEverything();
                CustomCharacterRuntime.HideAllOnStage();
                CustomImageRuntime.ClearAll();

                // LoadLuaFunction 会初始化共享 Interpreter；编译错误可能被 Fungus 吞掉并返回 null。
                Closure fn = env.LoadLuaFunction(lua, scriptName + ".LuaScript");
                if (fn == null)
                    throw new InvalidOperationException("Lua 编译失败，LoadLuaFunction 返回 null");

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

                // Fungus 会吞掉异步 InterpreterException；宿主自行推进，才能记录 runtime_error。
                if (RuntimeTrace.Active)
                {
                    _activeDevelopmentEnvironment = env;
                    _activeDevelopmentManager = __instance;
                }
                env.StartCoroutine(HostRunModLua(env, fn, scriptName));
                Log.LogInfo("mod 脚本开始演出：" + scriptName);
            }
            catch (Exception ex)
            {
                RuntimeTrace.RuntimeError(ex.ToString());
                Log.LogError("mod 脚本 " + scriptName + " 演出失败：" + ex);
            }
            return false; // 该脚本名归 mod 管，成败都跳过原方法
        }

        private static IEnumerator HostRunModLua(LuaEnvironment env, Closure fn, string scriptName)
        {
            DynValue coroutine = null;
            Exception failure = null;
            try
            {
                Script script = env != null ? env.Interpreter : null;
                if (script == null) throw new InvalidOperationException("LuaEnvironment.Interpreter 为 null");
                coroutine = script.CreateCoroutine(fn);
                if (coroutine == null || coroutine.Type != DataType.Thread || coroutine.Coroutine == null)
                    throw new InvalidOperationException("无法创建 MoonSharp 协程");
            }
            catch (Exception ex) { failure = ex; }
            if (failure != null)
            {
                RuntimeTrace.RuntimeError("启动失败：" + failure);
                Log.LogError("mod 脚本 " + scriptName + " 启动失败：" + failure);
                yield break;
            }
            while (coroutine.Coroutine.State != CoroutineState.Dead)
            {
                if (RuntimeDebugControl.Paused)
                {
                    yield return null;
                    continue;
                }
                failure = null;
                try { coroutine.Coroutine.Resume(); }
                catch (Exception ex) { failure = ex; }
                if (failure != null)
                {
                    RuntimeTrace.RuntimeError(failure.ToString());
                    Log.LogError("mod 脚本 " + scriptName + " 运行异常：" + failure);
                    yield break;
                }
                yield return null;
            }
        }

        /// <summary>
        /// Stops and forgets the current F5 execution. Resetting the protected Fungus
        /// fields is intentional: StopAllCoroutines alone leaves modflags, modvars and
        /// all host callbacks in the shared MoonSharp global table.
        /// </summary>
        internal static void StopActiveDevelopmentPlayback()
        {
            if (!RuntimeTrace.Active) return;
            RuntimeDebugControl.Continue();

            LuaEnvironment env = _activeDevelopmentEnvironment;
            LuaManager manager = _activeDevelopmentManager;
            _activeDevelopmentEnvironment = null;
            _activeDevelopmentManager = null;

            try
            {
                if (env != null) env.StopAllCoroutines();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("F5 热重载停止 LuaEnvironment 协程失败：" + ex.Message);
            }
            try
            {
                if (manager != null) manager.StopAllCoroutines();
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("F5 热重载停止 LuaManager 协程失败：" + ex.Message);
            }

            if (env == null) return;
            try
            {
                Traverse fields = Traverse.Create(env);
                fields.Field("interpreter").SetValue(null);
                fields.Field("initialised").SetValue(false);
            }
            catch (Exception ex)
            {
                if (Log != null) Log.LogWarning("F5 热重载清空 MoonSharp 环境失败：" + ex.Message);
            }
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
                script.Globals["mod_trace_node"] = new CallbackFunction((ctx, args) =>
                {
                    CaptureTraceState(script);
                    RuntimeTrace.NodeEnter(ArgString(args, 0), ArgString(args, 1));
                    if (RuntimeDebugControl.BeforeNode())
                        return DynValue.NewYieldReq(new DynValue[0]);
                    return DynValue.Nil;
                }, "mod_trace_node");
                script.Globals["mod_trace_choice"] = new CallbackFunction((ctx, args) =>
                {
                    int selected = args.Count > 1 && args[1].Type == DataType.Number ? (int)args[1].Number : 0;
                    RuntimeTrace.Choice(ArgString(args, 0), selected, ArgString(args, 2));
                    return DynValue.Nil;
                }, "mod_trace_choice");
                script.Globals["mod_trace_condition"] = new CallbackFunction((ctx, args) =>
                {
                    RuntimeTrace.Condition(ArgString(args, 0), ArgString(args, 1), ArgString(args, 2));
                    return DynValue.Nil;
                }, "mod_trace_condition");
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
                script.Globals["mod_play_voice"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        string voice = args.Count > 0 ? args[0].CastToString() : "";
                        CustomAudioPlayer.PlayVoice(voice);
                    }
                    catch (Exception ex)
                    {
                        Log.LogWarning("mod_play_voice 失败：" + ex.Message);
                    }
                    return DynValue.Nil;
                }, "mod_play_voice");
                script.Globals["mod_stop_voice"] = new CallbackFunction((ctx, args) =>
                {
                    CustomAudioPlayer.StopVoice();
                    return DynValue.Nil;
                }, "mod_stop_voice");
                script.Globals["mod_hide_all"] = new CallbackFunction((ctx, args) =>
                {
                    CustomCharacterRuntime.HideAllOnStage();
                    CustomImageRuntime.ClearAll();
                    return DynValue.Nil;
                }, "mod_hide_all");
                script.Globals["mod_background_show"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        CustomImageRuntime.ShowBackground(
                            ArgString(args, 0), ArgFloat(args, 1, 0f));
                    }
                    catch (Exception ex)
                    {
                        Log.LogWarning("mod_background_show 失败：" + ex.Message);
                    }
                    return DynValue.Nil;
                }, "mod_background_show");
                script.Globals["mod_background_clear"] = new CallbackFunction((ctx, args) =>
                {
                    try { CustomImageRuntime.ClearBackground(ArgFloat(args, 0, 0f)); }
                    catch (Exception ex) { Log.LogWarning("mod_background_clear 失败：" + ex.Message); }
                    return DynValue.Nil;
                }, "mod_background_clear");
                script.Globals["mod_cg_show"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        CustomImageRuntime.ShowCg(
                            ArgString(args, 0),
                            ArgFloat(args, 1, 0f),
                            ArgFloat(args, 2, 100f),
                            ArgFloat(args, 3, 0f),
                            ArgFloat(args, 4, 0f));
                    }
                    catch (Exception ex) { Log.LogWarning("mod_cg_show 失败：" + ex.Message); }
                    return DynValue.Nil;
                }, "mod_cg_show");
                script.Globals["mod_cg_hide"] = new CallbackFunction((ctx, args) =>
                {
                    try { CustomImageRuntime.HideCg(ArgFloat(args, 0, 0f)); }
                    catch (Exception ex) { Log.LogWarning("mod_cg_hide 失败：" + ex.Message); }
                    return DynValue.Nil;
                }, "mod_cg_hide");
                script.Globals["mod_overlay_show"] = new CallbackFunction((ctx, args) =>
                {
                    try
                    {
                        CustomImageRuntime.ShowOverlay(
                            ArgString(args, 0), ArgString(args, 1), ArgString(args, 2, "center"),
                            ArgFloat(args, 3, 100f), ArgFloat(args, 4, 100f),
                            ArgString(args, 5, "front"), ArgFloat(args, 6, 0f));
                    }
                    catch (Exception ex) { Log.LogWarning("mod_overlay_show 失败：" + ex.Message); }
                    return DynValue.Nil;
                }, "mod_overlay_show");
                script.Globals["mod_overlay_hide"] = new CallbackFunction((ctx, args) =>
                {
                    try { CustomImageRuntime.HideOverlay(ArgString(args, 0), ArgFloat(args, 1, 0f)); }
                    catch (Exception ex) { Log.LogWarning("mod_overlay_hide 失败：" + ex.Message); }
                    return DynValue.Nil;
                }, "mod_overlay_hide");
                RegisterCharacterGlobals(script);
            }
            catch (Exception ex)
            {
                Log.LogWarning("注册 mod 全局函数失败：" + ex);
            }
        }

        private static void RegisterCharacterGlobals(Script script)
        {
            script.Globals["mod_char_show"] = new CallbackFunction((ctx, args) =>
            {
                try
                {
                    CustomCharacterRuntime.Show(
                        ArgString(args, 0),
                        ArgString(args, 1, "normal"),
                        ArgString(args, 2, "M"),
                        ArgString(args, 3, "right"),
                        ArgFloat(args, 4, 0f),
                        ArgFloat(args, 5, 0f));
                }
                catch (Exception ex)
                {
                    Log.LogWarning("mod_char_show 失败：" + ex.Message);
                }
                return DynValue.Nil;
            }, "mod_char_show");
            script.Globals["mod_char_hide"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.Hide(ArgString(args, 0), ArgFloat(args, 1, 0f)); }
                catch (Exception ex) { Log.LogWarning("mod_char_hide 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_hide");
            script.Globals["mod_char_move"] = new CallbackFunction((ctx, args) =>
            {
                try
                {
                    CustomCharacterRuntime.Move(
                        ArgString(args, 0),
                        ArgString(args, 1),
                        ArgString(args, 2),
                        ArgFloat(args, 3, 1f));
                }
                catch (Exception ex) { Log.LogWarning("mod_char_move 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_move");
            script.Globals["mod_char_face"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.Face(ArgString(args, 0), ArgString(args, 1, "right")); }
                catch (Exception ex) { Log.LogWarning("mod_char_face 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_face");
            script.Globals["mod_char_focus"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.Focus(ArgString(args, 0)); }
                catch (Exception ex) { Log.LogWarning("mod_char_focus 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_focus");
            script.Globals["mod_char_offset"] = new CallbackFunction((ctx, args) =>
            {
                try
                {
                    CustomCharacterRuntime.Offset(
                        ArgString(args, 0),
                        ArgFloat(args, 1, 0f),
                        ArgFloat(args, 2, 0f),
                        ArgFloat(args, 3, 0.5f));
                }
                catch (Exception ex) { Log.LogWarning("mod_char_offset 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_offset");
            script.Globals["mod_char_shock"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.Shock(ArgString(args, 0), ArgFloat(args, 1, 0.5f)); }
                catch (Exception ex) { Log.LogWarning("mod_char_shock 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_shock");
            script.Globals["mod_char_dim"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.Dim(ArgString(args, 0), ArgBool(args, 1, true)); }
                catch (Exception ex) { Log.LogWarning("mod_char_dim 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_dim");
            script.Globals["mod_char_rotate"] = new CallbackFunction((ctx, args) =>
            {
                try
                {
                    CustomCharacterRuntime.Rotate(
                        ArgString(args, 0),
                        ArgFloat(args, 1, 180f),
                        ArgFloat(args, 2, 1f));
                }
                catch (Exception ex) { Log.LogWarning("mod_char_rotate 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_rotate");
            script.Globals["mod_char_portrait"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.SetPortrait(ArgString(args, 0), ArgString(args, 1, "normal")); }
                catch (Exception ex) { Log.LogWarning("mod_char_portrait 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_portrait");
            script.Globals["mod_char_set_speaker"] = new CallbackFunction((ctx, args) =>
            {
                try { CustomCharacterRuntime.SetSpeaker(ArgString(args, 0)); }
                catch (Exception ex) { Log.LogWarning("mod_char_set_speaker 失败：" + ex.Message); }
                return DynValue.Nil;
            }, "mod_char_set_speaker");
        }

        private static string ArgString(MoonSharp.Interpreter.CallbackArguments args, int index, string fallback = "")
        {
            try
            {
                if (args.Count > index)
                {
                    string value = args[index].CastToString();
                    if (!string.IsNullOrEmpty(value))
                        return value;
                }
            }
            catch { }
            return fallback;
        }

        private static float ArgFloat(MoonSharp.Interpreter.CallbackArguments args, int index, float fallback)
        {
            try
            {
                if (args.Count > index && args[index].Type == DataType.Number)
                    return (float)args[index].Number;
            }
            catch { }
            return fallback;
        }

        private static void CaptureTraceState(Script script)
        {
            if (!RuntimeTrace.Active || script == null) return;
            RuntimeTrace.ReplaceFlags(ReadDebugTable(script.Globals.Get("modflags")));
            RuntimeTrace.ReplaceVariables(ReadDebugTable(script.Globals.Get("modvars")));
        }

        private static Dictionary<string, string> ReadDebugTable(DynValue value)
        {
            var result = new Dictionary<string, string>(StringComparer.Ordinal);
            if (value == null || value.Type != DataType.Table || value.Table == null) return result;
            foreach (TablePair pair in value.Table.Pairs)
            {
                string key = pair.Key != null ? pair.Key.ToPrintString() : "";
                if (key.Length == 0) continue;
                result[key] = pair.Value != null ? pair.Value.ToPrintString() : "nil";
            }
            return result;
        }

        private static bool ArgBool(MoonSharp.Interpreter.CallbackArguments args, int index, bool fallback)
        {
            try
            {
                if (args.Count > index && args[index].Type == DataType.Boolean)
                    return args[index].Boolean;
            }
            catch { }
            return fallback;
        }
    }
}
