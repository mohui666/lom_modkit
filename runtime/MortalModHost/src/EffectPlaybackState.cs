using System;

namespace MortalModHost
{
    // The identity and playback generation of one Host-owned effect instance.
    internal sealed class EffectPlaybackState
    {
        internal readonly string Name;
        internal readonly string Tag;
        internal readonly bool Reusable;
        private long _generation;

        internal EffectPlaybackState(string name, string tag, bool reusable)
        {
            Name = name;
            Tag = EffectiveTag(name, tag);
            Reusable = reusable;
        }

        internal static string EffectiveTag(string name, string tag)
        {
            return string.IsNullOrEmpty(tag) ? name : tag;
        }

        internal bool CanReuse(string name, string tag)
        {
            return Reusable && string.Equals(Tag, EffectiveTag(name, tag), StringComparison.Ordinal);
        }

        internal bool MatchesStop(string name, string tag)
        {
            return string.IsNullOrEmpty(tag)
                ? string.Equals(Name, name, StringComparison.Ordinal)
                : string.Equals(Tag, tag, StringComparison.Ordinal);
        }

        internal long NextPlayback() { return ++_generation; }
        internal bool IsCurrent(long generation) { return generation == _generation; }
    }
}
