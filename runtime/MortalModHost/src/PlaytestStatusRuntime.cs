using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace MortalModHost
{
    // Optional acknowledgements for SDK requests. Old F5 and video requests
    // never create status files. These are execution observations, not visual
    // inspection or completion of an entire gameplay route.
    internal static class PlaytestStatusRuntime
    {
        private sealed class Session
        {
            internal PreviewRequest Request;
            internal string Path, State, Message;
            internal bool Dispatched;
            internal bool PredecessorEntered, EventSelected, EventEntered, EventNodeEntered, OriginalTargetEntered;
            internal bool SelectedEventMatches;
            internal string SelectedModId, SelectedEventId;
            internal bool EventTracking { get { return Request.EventScript != null; } }
            internal bool Finished { get { return State == "failed" || State == "event_not_triggered"
                || (EventTracking ? EventNodeEntered : State == "node_entered"); } }
        }
        private static Session _active;
        internal static Action<string> LogError = Console.Error.WriteLine;

        internal static void Observe(PreviewRequest request, string pluginDirectory)
        {
            if (_active != null && _active.Request.RequestId == request.RequestId) return;
            if (_active != null)
            {
                if (!_active.Finished)
                    Fail("试玩请求已被后续请求取代", null, null);
                _active = null;
            }
            if (request.RequestId == null) return;
            string directory = System.IO.Path.Combine(pluginDirectory, "playtest-status");
            _active = new Session { Request = request, Path = System.IO.Path.Combine(directory, request.RequestId + ".json") };
            Write("game_started", "游戏 Host 已接收试玩请求，正在等待目标章节", "host_request", null, null);
        }

        internal static bool Tracked { get { return _active != null; } }
        internal static void Dispatch() { if (_active != null) _active.Dispatched = true; }

        internal static void Waiting(string message)
        {
            if (_active != null && _active.State == "game_started")
                Write("game_started", message, "host_request", null, null);
        }

        internal static void ScriptStarted(string modId, string scriptId)
        {
            if (!ProgressAllowed()) return;
            if (modId != "lom_modkit_preview") { Detach("试玩已被其他作品演出取代"); return; }
            if (_active.EventTracking)
            {
                if (!_active.SelectedEventMatches || scriptId != _active.Request.EventScript || _active.EventEntered) return;
                _active.EventEntered = true;
                Write("chapter_entered", "真实调度的目标事件已开始执行，正在等待目标步骤", "lua_start", scriptId, null);
                return;
            }
            if (_active.Request.ScriptId == scriptId && _active.State != "node_entered")
                Write("chapter_entered", "游戏已开始执行目标章节，正在等待目标步骤", "lua_start", scriptId, null);
        }

        internal static void NodeEntered(string modId, string scriptId, string nodeId)
        {
            if (!ProgressAllowed()) return;
            if (_active.EventTracking)
            {
                if (_active.SelectedEventMatches && _active.EventEntered && !_active.EventNodeEntered
                    && modId == "lom_modkit_preview" && scriptId == _active.Request.EventScript && nodeId == _active.Request.EventNode)
                {
                    _active.EventNodeEntered = true;
                    Write("node_entered", "游戏已执行真实调度的目标事件及目标步骤", "node_trace", scriptId, nodeId);
                }
                return;
            }
            if (_active.State == "node_entered") return;
            if (modId == "lom_modkit_preview" && _active.Request.ScriptId == scriptId && _active.Request.NodeId == nodeId)
                Write("node_entered", "游戏已进入目标章节和步骤", "node_trace", scriptId, nodeId);
        }

        internal static void EventSelected(string modId, string eventId)
        {
            if (!ProgressAllowed() || !_active.EventTracking) return;
            _active.SelectedModId = modId;
            _active.SelectedEventId = eventId;
            _active.SelectedEventMatches = modId == "lom_modkit_preview" && eventId == _active.Request.EventId;
            if (_active.SelectedEventMatches)
            {
                _active.EventSelected = true;
                if (!_active.EventEntered)
                    Write(_active.State, "目标事件已被真实调度选中，等待事件演出执行", "event_dispatch", null, null);
            }
            else if (!_active.EventEntered)
                Write(_active.State, "真实调度选中了其他事件：" + modId + "/" + eventId + "；尚未进入目标事件", "event_dispatch", null, null);
        }

        internal static void OriginalScriptStarted(string scriptId)
        {
            if (_active == null || !_active.Dispatched || _active.State == "failed" || !_active.EventTracking) return;
            bool predecessor = scriptId == _active.Request.SourceScript && !_active.PredecessorEntered;
            bool target = scriptId == _active.Request.Target && !_active.OriginalTargetEntered;
            if (!predecessor && !target) return;
            if (predecessor) _active.PredecessorEntered = true;
            if (target) _active.OriginalTargetEntered = true;
            if (_active.State == "event_not_triggered")
            {
                if (predecessor) Write(_active.State, _active.Message, "original_lua_start", scriptId, null, true);
                return;
            }
            if (target)
            {
                if (!_active.EventEntered)
                    Write("event_not_triggered", "已实际进入原版目标章节，但目标事件未进入；请检查真实条件、概率、优先级及来源匹配", "original_lua_start", scriptId, null);
                else
                    Write("original_target_entered", "目标事件进入后，原版目标章节已实际开始执行", "original_lua_start", scriptId, null);
            }
            else if (_active.EventEntered || _active.OriginalTargetEntered)
                Write(_active.State, _active.Message, "original_lua_start", scriptId, null, true);
            else
                Write("predecessor_entered", "原版前置章节已实际开始执行，等待其自然进入目标章节并调度事件", "original_lua_start", scriptId, null);
        }

        private static bool ProgressAllowed()
        {
            return _active != null && _active.Dispatched && _active.State != "failed" && _active.State != "event_not_triggered";
        }

        internal static void Fail(string reason, string scriptId, string nodeId)
        {
            if (_active != null && _active.State != "failed")
                Write("failed", reason, "host_error", scriptId, nodeId);
        }

        internal static void RuntimeFailed(string reason, string scriptId, string nodeId)
        {
            if (_active != null && _active.Dispatched) Fail(reason, scriptId, nodeId);
        }

        private static void Detach(string reason)
        {
            if (_active == null) return;
            if (!_active.Finished) Fail(reason, null, null);
            _active = null;
        }

        internal static void Shutdown() { Detach("游戏在进入目标步骤前退出或卸载 Host"); }

        private static void Write(string state, string message, string evidence, string scriptId, string nodeId, bool changedContext = false)
        {
            if (!changedContext && _active.State == state && _active.Message == message) return;
            PreviewRequest request = _active.Request;
            string contextMode = !request.FreshCampaign ? "presentation"
                : request.SourceScript != null ? "original_predecessor"
                : request.Target != null ? "original_boundary" : "event_runtime";
            var value = new Dictionary<string, object>
            {
                {"format",1}, {"request_id",_active.Request.RequestId}, {"state",state},
                {"script_id",_active.Request.ScriptId}, {"node_id",_active.Request.NodeId},
                {"current_script",scriptId}, {"current_node",nodeId},
                {"message",message}, {"evidence",evidence}, {"visual_checked",false},
                {"context_mode",contextMode}, {"event_id",request.EventId},
                {"source_script",request.SourceScript}, {"target",request.Target},
                {"event_script",request.EventScript}, {"event_node",request.EventNode},
                {"predecessor_entered",_active.PredecessorEntered}, {"event_selected",_active.EventSelected},
                {"event_entered",_active.EventEntered}, {"event_node_entered",_active.EventNodeEntered},
                {"original_target_entered",_active.OriginalTargetEntered},
                {"selected_mod_id",_active.SelectedModId}, {"selected_event_id",_active.SelectedEventId},
                {"updated_at",DateTimeOffset.UtcNow.ToUnixTimeSeconds()}
            };
            // Status I/O must not prevent the existing gameplay abort/cleanup.
            // Stop this optional observer after a reported write failure; never
            // manufacture an acknowledgement in memory for an unwritten file.
            try
            {
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(_active.Path));
                string temporary = _active.Path + ".tmp";
                File.WriteAllText(temporary, ScenarioState.ToJson(value), new UTF8Encoding(false));
                if (File.Exists(_active.Path)) File.Replace(temporary, _active.Path, null);
                else File.Move(temporary, _active.Path);
                _active.State = state; _active.Message = message;
            }
            catch (IOException error) { WriteFailed(error); }
            catch (UnauthorizedAccessException error) { WriteFailed(error); }
        }

        private static void WriteFailed(Exception error)
        {
            string message = "无法写入试玩状态 " + _active.Path + "，后续执行状态无法确认：" + error.Message;
            _active = null;
            if (LogError != null) LogError(message);
            else Console.Error.WriteLine(message);
        }
    }
}
