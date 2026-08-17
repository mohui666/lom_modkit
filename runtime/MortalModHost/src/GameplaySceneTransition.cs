using System;
using System.Collections;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    /// <summary>
    /// 原版 LuaManager.Init() 只 StartCoroutine(UnloadLoading())，不会等待 Story 的
    /// Loading1 真正卸载就开始执行 Lua。Combat/Battle 若是章节首节点，直接调用
    /// ChangeScene 会与这次卸载并发，导致下一场永久停在“读取中”。Host 在这里等到
    /// 原版场景完全就绪，再调用原版 ChangeScene，保留其存档和场景字段语义。
    /// </summary>
    internal static class GameplaySceneTransition
    {
        private const float TimeoutSeconds = 30f;
        private static bool _pending;

        internal static void Begin(LuaManager manager, string kind)
        {
            if (manager == null) throw new ArgumentNullException("manager");
            bool combat = string.Equals(kind, "combat", StringComparison.Ordinal);
            bool battle = string.Equals(kind, "battle", StringComparison.Ordinal);
            if ((!combat || !GameplaySession.PendingCombat)
                && (!battle || !GameplaySession.PendingBattle))
                throw new InvalidOperationException("Gameplay 场景请求与已准备会话不一致：" + kind);
            if (_pending)
                throw new InvalidOperationException("Gameplay 场景切换已经在等待原版读取遮罩");
            _pending = true;
            manager.StartCoroutine(WaitAndChange(manager, kind));
        }

        internal static void Reset()
        {
            _pending = false;
        }

        private static IEnumerator WaitAndChange(LuaManager manager, string kind)
        {
            float deadline = Time.realtimeSinceStartup + TimeoutSeconds;
            try
            {
                while (true)
                {
                    SceneController scenes = SceneController.Instance;
                    if (scenes != null
                        && string.Equals(scenes.CurrentScene, "Story", StringComparison.Ordinal)
                        && Plugin.IsSceneReadyForModNavigation(scenes))
                        break;
                    if (Time.realtimeSinceStartup >= deadline)
                    {
                        LuaManagerPatch.AbortActivePlayback(
                            "等待原版 Story 读取遮罩结束超时，已取消 Gameplay 场景切换",
                            manager, null, "gameplay_scene_timeout");
                        yield break;
                    }
                    yield return null;
                }

                if (manager == null)
                {
                    LuaManagerPatch.AbortActivePlayback(
                        "Gameplay 场景切换前 LuaManager 已销毁",
                        null, null, "gameplay_scene_lifecycle");
                    yield break;
                }
                bool combat = string.Equals(kind, "combat", StringComparison.Ordinal);
                string scene = combat ? "Combat" : "Battle";
                // Combat 使用 Host 即时组装的隔离关卡，不再借用任何固定原版 CL。
                string key = combat ? "MORTALMODHOST_RUNTIME" : "0000";
                LuaManagerPatch.Log?.LogInfo(
                    "原版 Story 读取遮罩已结束，开始安全切换到 " + scene);
                manager.ChangeScene(scene, key, "Story");
            }
            finally
            {
                _pending = false;
            }
        }
    }
}
