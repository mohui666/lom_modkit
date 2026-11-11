using System;
using System.Collections;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    // Inserts an event before a real original chapter begins. The original map
    // has already charged its action; continuation enters the target directly.
    internal static class StoryEntryRuntime
    {
        private static string _transitionFrom, _transitionTarget;
        private static LuaManager _resumeManager;
        private static string _resumeTarget;
        private static LuaManager _returningManager;

        internal static void Transition(string from,string target)
        {
            _transitionFrom=from;
            _transitionTarget=target;
        }

        internal static void Enter(LuaManager manager)
        {
            string target=manager.ScriptName;
            string from=_transitionTarget==target ? _transitionFrom : null;
            _transitionFrom=null;
            _transitionTarget=null;
            if (_resumeManager==manager && _resumeTarget==target) {
                _resumeManager=null;
                _resumeTarget=null;
                return;
            }
            if (!VanillaStoryPatch.ValidTarget(target) || !ScenarioRuntime.HasStoryEntry(target,from)) return;
            // This is the same local resource used by LuaManager.ExecuteLuaScript.
            // A miss is left to the game's own missing-script diagnostic.
            if (Resources.Load<TextAsset>("Story/ChineseTraditional/"+target)==null) return;
            var menu=Traverse.Create(StoryManager.Instance).Field("_menuPanelController").GetValue<MenuPanel>();
            if(menu==null)throw new InvalidOperationException("原版章节入口缺少 Story 存档菜单");
            var save=Traverse.Create(menu).Field("_saveButton").GetValue<Button>();
            if(save==null)throw new InvalidOperationException("原版章节入口缺少存档按钮");
            var continuation=new StoryEntryReturn {Target=target,From=from,
                Start=Traverse.Create(PlayerStatManagerData.Instance).Field("_startStoryScript").GetValue<string>()??"",
                SaveEnabled=save.interactable,
                Travel=manager.GetCurrentTravelScript()??"",Temp1=manager.GetTempScript(1)??"",
                Temp2=manager.GetTempScript(2)??"",Temp3=manager.GetTempScript(3)??""};
            string script;
            if (!ScenarioRuntime.TryDispatch("story_entry",null,target,from,continuation,out script)) return;
            // The existing MOD loader now owns this invocation and its interpreter.
            manager.SetCurrentStoryScript(script);
            // An original source may already allow saving. Point that inherited
            // checkpoint at the pending event even when it never toggles saving.
            PlayerStatManagerData.Instance.SetStartScript(script);
        }

        internal static bool TryResume(LuaManager manager)
        {
            StoryEntryReturn continuation=ScenarioRuntime.GetStoryReturn(manager);
            if(continuation==null) return false;
            if(_returningManager==manager) return true;
            _returningManager=manager;
            manager.StartCoroutine(Resume(manager,continuation));
            return true;
        }

        private static IEnumerator Resume(LuaManager manager,StoryEntryReturn continuation)
        {
            // Let the event's ChangeScene/end call unwind before the loader stops
            // the MOD coroutine and rebuilds the original Lua environment.
            yield return null;
            float deadline=Time.realtimeSinceStartup+30f;
            while(SceneController.Instance!=null && !Plugin.IsSceneReadyForModNavigation(SceneController.Instance)) {
                if(Time.realtimeSinceStartup>=deadline) {
                    _returningManager=null;
                    LuaManagerPatch.AbortActivePlayback("等待原版章节续接就绪超时",manager,null,"story_entry_resume");
                    yield break;
                }
                yield return null;
            }
            try {
                if(manager==null || SceneController.Instance==null || SceneController.Instance.CurrentScene!="Story")
                    throw new InvalidOperationException("原版章节续接的 Story 场景已不可用");
                if(Resources.Load<TextAsset>("Story/ChineseTraditional/"+continuation.Target)==null)
                    throw new InvalidOperationException("当前游戏缺少续接原版章节："+continuation.Target);
                ScenarioRuntime.Complete(manager);
                manager.SetCurrentTravelScript(continuation.Travel);
                manager.SetTempScript(1,continuation.Temp1);
                manager.SetTempScript(2,continuation.Temp2);
                manager.SetTempScript(3,continuation.Temp3);
                PlayerStatManagerData.Instance.SetStartScript(continuation.Start);
                StoryManager.Instance.ToggleSaveButton(continuation.SaveEnabled);
                _resumeManager=manager;
                _resumeTarget=continuation.Target;
                manager.SetNextScript(continuation.Target);
                ScenarioRuntime.Log?.LogInfo("事件后继续原版章节："+continuation.Target);
                manager.Init();
            }
            catch(Exception error) {
                _resumeManager=null;
                _resumeTarget=null;
                LuaManagerPatch.AbortActivePlayback("原版章节续接失败",manager,error,"story_entry_resume");
            }
            finally {_returningManager=null;}
        }

        internal static void Clear()
        {
            _transitionFrom=null;
            _transitionTarget=null;
            _resumeManager=null;
            _resumeTarget=null;
            _returningManager=null;
        }
    }

    [HarmonyPatch(typeof(LuaManager),"SetNextScript")]
    internal static class StoryEntryTransitionPatch
    {
        private static void Prefix(LuaManager __instance,out string __state) {__state=__instance.ScriptName;}
        private static void Postfix(LuaManager __instance,string script,string __state)
        {
            if(__instance.ScriptName==script)StoryEntryRuntime.Transition(__state,script);
        }
    }

    [HarmonyPatch(typeof(LuaManager),"ExecuteLuaScript")]
    internal static class StoryEntryExecutePatch
    {
        [HarmonyPriority(Priority.First)]
        private static bool Prefix(LuaManager __instance)
        {
            try {StoryEntryRuntime.Enter(__instance);return true;}
            catch(Exception error) {
                LuaManagerPatch.AbortActivePlayback("原版章节入口事件调度失败",__instance,error,"story_entry_dispatch");
                return false;
            }
        }
    }

    [HarmonyPatch(typeof(PlayerStatManagerData),"ResetData")]
    internal static class StoryEntryResetPatch
    {
        private static void Postfix() {StoryEntryRuntime.Clear();}
    }
}
