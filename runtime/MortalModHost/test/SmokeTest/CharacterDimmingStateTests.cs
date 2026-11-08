using System;
using System.Linq;

namespace MortalModHost
{
    internal static class CharacterDimmingStateTests
    {
        internal static void Run()
        {
            var state = new CharacterDimmingState();
            bool value;
            state.Set("manual-dark", true, "manual");
            state.Set("manual-light", false, "manual");
            state.Set("temporary", false, "until_dialogue");
            if (!state.TryGet("temporary", out value) || value) throw new Exception("temporary highlight missing");
            state.BeginDialogue();
            if (state.TryGet("temporary", out value) || !state.Automatic.Contains("temporary"))
                throw new Exception("temporary highlight did not return to automatic");
            if (!state.TryGet("manual-dark", out value) || !value || !state.TryGet("manual-light", out value) || value)
                throw new Exception("legacy manual dimming changed");
            state.Set("manual-dark", false, "auto");
            if (state.TryGet("manual-dark", out value) || !state.Automatic.Contains("manual-dark"))
                throw new Exception("cannot restore automatic after persistent dimming");
            state.Set("temporary", true, "until_dialogue");
            state.Set("temporary", false, "manual");
            state.BeginDialogue();
            if (!state.TryGet("temporary", out value) || value || state.Automatic.Contains("temporary"))
                throw new Exception("a later manual step was expired by an older temporary step");
            state.Clear();
            if (state.Automatic.Any() || state.TryGet("temporary", out value)) throw new Exception("stage dimming was not cleared");
            Console.WriteLine("PASS temporary and manual character dimming state (offline)");
        }
    }
}
