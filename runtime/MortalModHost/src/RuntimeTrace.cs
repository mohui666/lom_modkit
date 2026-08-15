using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>Bounded development trace. It is active only for the fixed F5 preview package.</summary>
    internal static class RuntimeTrace
    {
        internal const int Capacity = 256;

        internal sealed class Entry
        {
            public long Sequence;
            public DateTime TimestampUtc;
            public string EventType;
            public string ModId;
            public string StoryId;
            public string NodeId;
            public string Detail;
        }

        private static readonly object Gate = new object();
        private static readonly Queue<Entry> Entries = new Queue<Entry>(Capacity);
        private static long _sequence;
        private static bool _active;
        private static string _modId = "";
        private static string _storyId = "";
        private static string _nodeId = "";
        private static string _nodeType = "";
        private static readonly Dictionary<string, string> Variables = new Dictionary<string, string>(StringComparer.Ordinal);
        private static readonly Dictionary<string, string> Flags = new Dictionary<string, string>(StringComparer.Ordinal);

        internal static bool Active { get { lock (Gate) return _active; } }
        internal static string CurrentMod { get { lock (Gate) return _modId; } }
        internal static string CurrentStory { get { lock (Gate) return _storyId; } }
        internal static string CurrentNode { get { lock (Gate) return _nodeId; } }

        internal static void BeginScript(ModPackage package, string registeredName)
        {
            bool development = IsDevelopmentPackage(package);
            lock (Gate)
            {
                bool continuation = development && _active && package != null
                    && string.Equals(_modId, package.Id, StringComparison.Ordinal);
                RuntimeDebugControl.Begin(development, continuation);
                _active = development;
                _nodeId = "";
                _nodeType = "";
                if (!development) return;
                if (!continuation) Entries.Clear();
                if (!continuation) { Variables.Clear(); Flags.Clear(); }
                _modId = package.Id ?? "";
                string prefix = "MOD_" + _modId + "_";
                _storyId = registeredName != null && registeredName.StartsWith(prefix, StringComparison.Ordinal)
                    ? registeredName.Substring(prefix.Length) : package.Entry ?? "";
                if (!continuation) AddLocked("mod_enter", "", package.Name ?? "");
                AddLocked("story_enter", "", _storyId);
            }
        }

        internal static bool IsDevelopmentPackage(ModPackage package)
        {
            return ModDisclosurePolicy.IsDevelopmentPreviewPackage(package);
        }

        internal static void NodeEnter(string nodeId, string nodeType)
        {
            lock (Gate)
            {
                if (!_active) return;
                string next = nodeId ?? "";
                if (_nodeId.Length > 0 && !string.Equals(_nodeId, next, StringComparison.Ordinal))
                {
                    if (_nodeType == "choice") AddLocked("choice", _nodeId, "target=" + next);
                    else if (_nodeType == "branch") AddLocked("condition_result", _nodeId, "target=" + next);
                    AddLocked("goto", _nodeId, next);
                }
                _nodeId = next;
                _nodeType = nodeType ?? "";
                AddLocked("node_enter", _nodeId, nodeType ?? "");
                if (nodeType == "end") AddLocked("end", _nodeId, "");
                else if (nodeType == "death") AddLocked("death", _nodeId, "");
            }
        }

        internal static void Choice(string nodeId, int selected, string target)
        {
            Record("choice", nodeId, "option=" + selected + " target=" + (target ?? ""));
        }

        internal static void Condition(string nodeId, string result, string target)
        {
            Record("condition_result", nodeId, (result ?? "") + " target=" + (target ?? ""));
        }

        internal static void RuntimeError(string detail)
        {
            Record("runtime_error", CurrentNode, detail ?? "");
        }

        /// <summary>
        /// Marks the boundary between two F5 runs while retaining the bounded history.
        /// Runtime variables and flags belong to the discarded Lua interpreter and must
        /// never leak into the freshly compiled preview.
        /// </summary>
        internal static void PrepareHotReload(string restartNodeId)
        {
            lock (Gate)
            {
                if (!_active) return;
                AddLocked("hot_reload", _nodeId,
                    "restart=" + (restartNodeId ?? ""));
                Variables.Clear();
                Flags.Clear();
                _nodeId = "";
                _nodeType = "";
                RuntimeDebugControl.Continue();
            }
        }

        internal static void Record(string eventType, string nodeId, string detail)
        {
            lock (Gate)
            {
                if (!_active) return;
                AddLocked(eventType ?? "unknown", nodeId ?? "", detail ?? "");
            }
        }

        internal static List<Entry> Snapshot()
        {
            lock (Gate) return new List<Entry>(Entries);
        }

        internal static Dictionary<string, string> VariablesSnapshot()
        {
            lock (Gate) return new Dictionary<string, string>(Variables, StringComparer.Ordinal);
        }

        internal static Dictionary<string, string> FlagsSnapshot()
        {
            lock (Gate) return new Dictionary<string, string>(Flags, StringComparer.Ordinal);
        }

        internal static void ReplaceVariables(IDictionary<string, string> values)
        {
            lock (Gate)
            {
                if (!_active) return;
                Variables.Clear();
                if (values != null) foreach (var pair in values) Variables[pair.Key] = pair.Value;
            }
        }

        internal static void ReplaceFlags(IDictionary<string, string> values)
        {
            lock (Gate)
            {
                if (!_active) return;
                Flags.Clear();
                if (values != null) foreach (var pair in values) Flags[pair.Key] = pair.Value;
            }
        }

        internal static void Reset()
        {
            lock (Gate)
            {
                Entries.Clear();
                Variables.Clear();
                Flags.Clear();
                RuntimeDebugControl.Reset();
                _active = false;
                _modId = _storyId = _nodeId = _nodeType = "";
            }
        }

        private static void AddLocked(string eventType, string nodeId, string detail)
        {
            while (Entries.Count >= Capacity) Entries.Dequeue();
            Entries.Enqueue(new Entry
            {
                Sequence = ++_sequence,
                TimestampUtc = DateTime.UtcNow,
                EventType = eventType,
                ModId = _modId,
                StoryId = _storyId,
                NodeId = nodeId,
                Detail = detail
            });
        }
    }
}
