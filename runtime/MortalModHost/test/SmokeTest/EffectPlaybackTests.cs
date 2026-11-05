using System;
using System.Linq;

namespace MortalModHost
{
    internal static class EffectPlaybackTests
    {
        private static void Check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }

        internal static void Run()
        {
            var defaultHit = new EffectPlaybackState("Hit_001", "", true);
            var namedHit = new EffectPlaybackState("Hit_001", "impact", true);
            var stackedHit = new EffectPlaybackState("Hit_001", "impact", false);
            var stackedBlood = new EffectPlaybackState("Blood_002", "impact", false);
            var otherHit = new EffectPlaybackState("Hit_001", "other", false);
            var owned = new[] { defaultHit, namedHit, stackedHit, stackedBlood, otherHit };

            Check(defaultHit.CanReuse("Hit_001", ""), "Empty tags must use the effect name as their reuse key");
            Check(!defaultHit.CanReuse("Blood_002", ""), "Empty tags must not reuse another named effect");
            Check(owned.Count(item => item.CanReuse("Hit_001", "impact")) == 1,
                "Reuse must select the tagged singleton without absorbing stacked instances");
            var replace = owned.Single(item => item.CanReuse("Sword_001", "impact"));
            Check(replace == namedHit && replace.Name != "Sword_001",
                "Changing a reused tag's name must identify the old singleton for replacement");
            Check(owned.Count(item => item.MatchesStop("Hit_001", "")) == 4,
                "An untagged stop must include same-name effects carrying other tags");
            Check(owned.Count(item => item.MatchesStop("ignored", "impact")) == 3,
                "A tagged stop must include stacked and reused effects of different names");
            Check(owned.Count(item => item.MatchesStop("ignored", "Hit_001")) == 1,
                "An explicit tag must also match the effective tag of an untagged playback");
            Check(!owned.Any(item => item.MatchesStop("missing", "")), "Stopping an absent effect must not select other instances");

            long first = namedHit.NextPlayback();
            long stacked = stackedHit.NextPlayback();
            Check(namedHit.IsCurrent(first), "A fresh playback generation must be active");
            long replay = namedHit.NextPlayback();
            Check(!namedHit.IsCurrent(first) && namedHit.IsCurrent(replay),
                "A timer from earlier playback must not stop the replayed singleton");
            Check(stackedHit.IsCurrent(stacked), "Replaying one instance must not invalidate another instance's timer");
            namedHit.NextPlayback();
            Check(!namedHit.IsCurrent(replay), "Explicit stop or cleanup must invalidate the pending timer");
            Console.WriteLine("PASS effects: tagged reuse/stack isolation, name/tag stops and stale timer generations (Unity execution=false)");
        }
    }
}
