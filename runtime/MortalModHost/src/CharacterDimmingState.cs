using System;
using System.Collections.Generic;

namespace MortalModHost
{
    // Only explicit new modes expire; old manual choices remain persistent.
    internal sealed class CharacterDimmingState
    {
        private readonly Dictionary<string, bool> _overrides = new Dictionary<string, bool>(StringComparer.Ordinal);
        private readonly HashSet<string> _temporary = new HashSet<string>(StringComparer.Ordinal);
        private readonly HashSet<string> _automatic = new HashSet<string>(StringComparer.Ordinal);

        internal IEnumerable<string> Automatic { get { return _automatic; } }
        internal bool TryGet(string id, out bool dimmed) { return _overrides.TryGetValue(id, out dimmed); }
        internal void Set(string id, bool dimmed, string mode)
        {
            if (mode != "manual" && mode != "until_dialogue" && mode != "auto")
                throw new ArgumentException("未知压暗生效方式：" + mode);
            _temporary.Remove(id);
            _automatic.Remove(id);
            if (mode == "auto")
            {
                _overrides.Remove(id);
                _automatic.Add(id);
            }
            else
            {
                _overrides[id] = dimmed;
                if (mode == "until_dialogue") _temporary.Add(id);
            }
        }
        internal void BeginDialogue()
        {
            foreach (string id in _temporary)
            {
                _overrides.Remove(id);
                _automatic.Add(id);
            }
            _temporary.Clear();
        }
        internal void Clear() { _overrides.Clear(); _temporary.Clear(); _automatic.Clear(); }
    }
}
