using System;
using System.Collections;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Reach the real event scheduler from an isolated editor campaign. This does
    // not force a condition result or replay the preceding official chapter.
    internal static class EventTestRuntime
    {
        private static ModPackage _package;
        private static PreviewRequest _request;

        internal static void Arm(ModPackage package, PreviewRequest request)
        {
            if (!RuntimeTrace.IsDevelopmentPackage(package) || package.Id != "lom_modkit_preview"
                || package.Scenario == null || !request.FreshCampaign)
                throw new InvalidOperationException("事件测试仅允许编辑器临时包");
            if (!package.Scenario.Events.Exists(e => e.Id == request.EventId))
                throw new InvalidOperationException("测试事件不存在：" + request.EventId);
            if (request.Target != null && Resources.Load<TextAsset>("Story/ChineseTraditional/" + request.Target) == null)
                throw new InvalidOperationException("当前游戏缺少测试原版章节：" + request.Target);
            _package = package;
            _request = request;
        }

        internal static void Clear() { _package = null; _request = null; }

        internal static bool TryContinue(LuaManager manager)
        {
            if (_package == null || _request == null || manager == null
                || manager.ScriptName != _package.GetRegisteredScriptName(_package.Entry)) return false;
            var package = _package;
            var request = _request;
            Clear();
            var engine = ScenarioRuntime.Open(package);
            var selected = package.Scenario.Events.Find(e => e.Id == request.EventId);
            ScenarioRuntime.Log?.LogInfo("事件测试条件：" + request.EventId + " = " + engine.Test(selected.When)
                + "；随后沿真实调度，仍受概率、优先级和原版任务影响。");
            if (request.Target == null) return false;
            manager.StartCoroutine(EnterOriginal(manager, request));
            return true;
        }

        private static IEnumerator EnterOriginal(LuaManager manager, PreviewRequest request)
        {
            yield return null;
            try
            {
                if (manager == null || SceneController.Instance == null || SceneController.Instance.CurrentScene != "Story")
                    throw new InvalidOperationException("事件测试的 Story 场景已不可用");
                if (request.From != null) manager.SetCurrentStoryScript(request.From);
                manager.SetNextScript(request.Target);
                manager.Init();
            }
            catch (Exception error)
            {
                LuaManagerPatch.AbortActivePlayback("原版入口事件测试失败", manager, error, "event_test");
            }
        }
    }
}
