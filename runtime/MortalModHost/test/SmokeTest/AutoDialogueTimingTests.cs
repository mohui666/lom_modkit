using System;

namespace MortalModHost
{
    internal static class AutoDialogueTimingTests
    {
        internal static void Run()
        {
            foreach (int count in new[] { 2, 10, 20, 40, 160 })
            {
                var clock = new AutoDialogueTiming(new string('字', count), false);
                float duration = Math.Max(1.2f, count / 8f);
                if (clock.Duration(AutoDialogueSpeed.Standard) != duration) throw new Exception("reading duration");
                if (clock.Tick(duration - 0.01f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("early advance");
                if (!clock.Tick(0.02f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("missing advance");
                if (clock.Tick(10f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("duplicate advance");
            }
            var text = new AutoDialogueTiming("<size=60>你</size> <color=#fff>好</color>\n😀", false);
            if (text.Characters != 3) throw new Exception("rich text, whitespace or surrogate counted");
            var voiced = new AutoDialogueTiming("你好", true);
            if (voiced.Tick(3f, true, true, AutoDialogueSpeed.Standard)) throw new Exception("voice cut off");
            if (voiced.Tick(10f, false, false, AutoDialogueSpeed.Standard)) throw new Exception("pause advanced");
            if (voiced.Tick(0.29f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("voice tail skipped");
            if (!voiced.Tick(0.02f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("voice tail stuck");
            var paused = new AutoDialogueTiming("你好", false);
            paused.Tick(0.6f, true, false, AutoDialogueSpeed.Standard);
            paused.Tick(20f, false, false, AutoDialogueSpeed.Standard);
            if (paused.Elapsed != 0.6f) throw new Exception("pause consumed reading time");
            if (!paused.Tick(0.61f, true, false, AutoDialogueSpeed.Standard)) throw new Exception("pause lost remaining time");
            if (!(text.Duration(AutoDialogueSpeed.Fast) < text.Duration(AutoDialogueSpeed.Standard)
                && text.Duration(AutoDialogueSpeed.Slow) > text.Duration(AutoDialogueSpeed.Standard))) throw new Exception("speed settings");
            Console.WriteLine("PASS smart autoplay reading time, voice tail, pause, speed and single advance (offline)");
        }
    }
}
