using System;
using System.Collections.Generic;

namespace MortalModHost
{
    /// <summary>
    /// Full bounded development trace plus a smaller node-only diagnostic breadcrumb
    /// queue. The debugger/full trace remains active only for the fixed F5 package;
    /// ordinary Mods never expose variables or flags to the breadcrumb queue.
    /// </summary>
    internal static class RuntimeTrace
    {
        internal const int Capacity = 256;
        internal const int DiagnosticCapacity = 32;

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
        private static readonly Queue<Entry> DiagnosticEntries = new Queue<Entry>(DiagnosticCapacity);
        private static long _sequence;
        private static bool _active;
        private static string _modId = "";
        private static string _modName = "";
        private static string _version = "";
        private static string _storyId = "";
        private static string _nodeId = "";
        private static string _nodeType = "";
        private static readonly Dictionary<string, string> Variables = new Dictionary<string, string>(StringComparer.Ordinal);
        private static readonly Dictionary<string, string> Flags = new Dictionary<string, string>(StringComparer.Ordinal);

        internal static bool Active { get { lock (Gate) return _active; } }
        internal static string CurrentMod { get { lock (Gate) return _modId; } }
        internal static string CurrentModName { get { lock (Gate) return _modName; } }
        internal static string CurrentVersion { get { lock (Gate) return _version; } }
        internal static string CurrentStory { get { lock (Gate) return _storyId; } }
        internal static string CurrentNode { get { lock (Gate) return _nodeId; } }

        internal static void BeginScript(ModPackage package, string registeredName)
        {
            bool development = IsDevelopmentPackage(package);
            lock (Gate)
            {
                string nextModId = package != null ? package.Id ?? "" : "";
                bool samePackage = package != null
                    && string.Equals(_modId, nextModId, StringComparison.Ordinal);
                bool continuation = development && _active && package != null
                    && samePackage;
                RuntimeDebugControl.Begin(development, continuation);
                _active = development;
                _nodeId = "";
                _nodeType = "";
                if (!samePackage) DiagnosticEntries.Clear();
                _modId = nextModId;
                _modName = package != null ? package.Name ?? "" : "";
                _version = package != null ? package.Version ?? "" : "";
                string prefix = "MOD_" + _modId + "_";
                _storyId = package != null && registeredName != null
                    && registeredName.StartsWith(prefix, StringComparison.Ordinal)
                    ? registeredName.Substring(prefix.Length)
                    : package != null ? package.Entry ?? "" : "";
                AddDiagnosticLocked("story_enter", "", _storyId);
                if (!development) return;
                if (!continuation) Entries.Clear();
                if (!continuation) { Variables.Clear(); Flags.Clear(); }
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
                string next = nodeId ?? "";
                if (_nodeId.Length > 0 && !string.Equals(_nodeId, next, StringComparison.Ordinal))
                {
                    if (_nodeType == "choice") AddBothLocked("choice", _nodeId, "target=" + next);
                    else if (_nodeType == "branch") AddBothLocked("condition_result", _nodeId, "target=" + next);
                    AddBothLocked("goto", _nodeId, next);
                }
                _nodeId = next;
                _nodeType = nodeType ?? "";
                AddBothLocked("node_enter", _nodeId, nodeType ?? "");
                if (nodeType == "end") AddBothLocked("end", _nodeId, "");
                else if (nodeType == "death") AddBothLocked("death", _nodeId, "");
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
                AddBothLocked("hot_reload", _nodeId,
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
                AddBothLocked(eventType ?? "unknown", nodeId ?? "", detail ?? "");
            }
        }

        internal static List<Entry> Snapshot()
        {
            lock (Gate) return new List<Entry>(Entries);
        }

        internal static List<Entry> DiagnosticSnapshot(int limit)
        {
            lock (Gate)
            {
                Entry[] all = DiagnosticEntries.ToArray();
                int count = limit <= 0 ? 0 : Math.Min(limit, all.Length);
                var result = new List<Entry>(count);
                for (int i = all.Length - count; i < all.Length; i++)
                    result.Add(all[i]);
                return result;
            }
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
                DiagnosticEntries.Clear();
                Variables.Clear();
                Flags.Clear();
                RuntimeDebugControl.Reset();
                _active = false;
                _modId = _modName = _version = _storyId = _nodeId = _nodeType = "";
            }
        }

        private static void AddBothLocked(string eventType, string nodeId, string detail)
        {
            AddDiagnosticLocked(eventType, nodeId, detail);
            if (_active) AddLocked(eventType, nodeId, detail);
        }

        private static void AddDiagnosticLocked(string eventType, string nodeId, string detail)
        {
            while (DiagnosticEntries.Count >= DiagnosticCapacity) DiagnosticEntries.Dequeue();
            DiagnosticEntries.Enqueue(new Entry
            {
                Sequence = ++_sequence,
                TimestampUtc = DateTime.UtcNow,
                EventType = eventType ?? "unknown",
                ModId = _modId,
                StoryId = _storyId,
                NodeId = nodeId ?? "",
                Detail = detail ?? ""
            });
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
