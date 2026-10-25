using System;
using System.IO;
using System.Linq;
using System.Runtime.Loader;
using MoonSharp.Interpreter;

internal static class Program
{
    private static int Main(string[] args)
    {
        if (args.Length != 2) throw new ArgumentException("Fungus.dll and fixture directory required");
        AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.GetFullPath(args[0]));
        return Run(args[1]);
    }

    private static int Run(string root)
    {
        int passed = 0;
        foreach (string name in new[] { "joined", "instant", "failure", "portraits", "transition" })
        {
            Script script = new Script(CoreModules.Preset_Complete);
            script.DoString(File.ReadAllText(Path.Combine(root, "prelude.lua")));
            DynValue co = script.CreateCoroutine(script.LoadString(File.ReadAllText(Path.Combine(root, name + ".lua"))));
            Exception failure = null;
            int frame = 0;
            for (; frame < 1000 && co.Coroutine.State != CoroutineState.Dead; frame++)
            {
                script.Globals["clock"] = frame * 0.05;
                try { co.Coroutine.Resume(); }
                catch (InterpreterException error) { failure = error; break; }
            }
            if (frame >= 1000) throw new Exception(name + " failed to join");
            var events = script.Globals.Get("events").Table.Values.Select(v => v.Table).ToArray();
            var starts = events.Where(e => e.Get("kind").String == "start").ToArray();
            var next = events.Where(e => e.Get("kind").String == "next").ToArray();
            if (name == "failure")
            {
                Require(failure != null && failure.Message.Contains("fixture branch failure"), "child errors must reach the parent");
                Require(next.Length == 0, "failed groups must not continue");
            }
            else
            {
                Require(failure == null, name + ": " + failure);
                Require(next.Length == 1, "parent must continue once");
                Require(starts.Length >= 2, "multiple branches must start");
                double started = starts[0].Get("time").Number;
                Require(starts.All(e => Math.Abs(e.Get("time").Number - started) < 1e-8), "actions must start on the same frame");
                if (name == "joined")
                {
                    double elapsed = next[0].Get("time").Number - started;
                    Require(elapsed >= 0.9 - 1e-8 && elapsed <= 1.0, "join must wait for the longest action, not the sum");
                    var loaded = events.Where(e => e.Get("kind").String == "loaded").ToArray();
                    Require(loaded.Length == 2 && loaded.All(e => e.Get("time").Number <= started), "cold official characters must load before launch");
                }
                if (name == "portraits")
                    Require(next[0].Get("time").Number - started >= 0.8 - 1e-8, "show/hide fades must finish before continuing");
                if (name == "instant")
                    Require(next[0].Get("time").Number == started, "instant groups must not invent a wait");
                if (name == "transition")
                    Require(next[0].Get("time").Number - started >= 0.7 - 1e-8, "Unity task waits must be joined");
            }
            Console.WriteLine("PASS " + name);
            passed++;
        }
        Console.WriteLine(passed + " MoonSharp offline scenarios passed; Unity/game execution: false");
        return 0;
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }
}
