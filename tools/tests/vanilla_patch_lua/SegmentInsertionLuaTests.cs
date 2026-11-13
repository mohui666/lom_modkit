using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using MoonSharp.Interpreter;
using MortalModHost;

internal static class SegmentInsertionLuaTests
{
    internal static void Run(string root)
    {
        string path = Path.Combine(root, "import-segment.json");
        if (!File.Exists(path)) return;
        using var catalog = JsonDocument.Parse(File.ReadAllText(Path.Combine(root, "import-segment-texts.json")));
        var texts = catalog.RootElement.EnumerateObject().ToDictionary(value => value.Name, value => value.Value.GetString());
        texts["N_IMPORT_A"] = "原版甲";
        texts["N_IMPORT_B"] = "原版乙";
        texts["N_IMPORT_DONE"] = "原版尾句";
        string lua = File.ReadAllText(Path.Combine(root, "import-segment.lua"));
        const string prefix = "MOD_showcase3_original_story_import_";
        foreach (bool outer in new[] { true, false })
        foreach (int nativeChoice in new[] { 1, 2 })
        foreach (int segmentChoice in new[] { 1, 2 })
        {
            var source = VanillaImportSource.Parse(File.ReadAllText(path));
            source.VerifyOriginal(source.OriginalSource);
            var script = new Script(CoreModules.Preset_Complete);
            script.Globals["outer"] = outer;
            script.Globals["native_choice"] = nativeChoice;
            script.Globals["segment_choice"] = segmentChoice;
            script.Globals["lookup_text"] = (Func<string, string>)(key => texts.TryGetValue(key, out var text) ? text : key);
            script.Globals["mod_begin_vanilla_import"] = (Action<string>)(id => source.Begin());
            script.Globals["mod_vanilla_source_node"] = (Action<string>)(id => source.Node(id));
            script.Globals["mod_vanilla_source_continue"] = new CallbackFunction((ctx, args) =>
            {
                source.ContinueOriginal();
                return DynValue.Nil;
            }, "mod_vanilla_source_continue");
            script.DoString(@"
events = {}
sayoptions = {waitforinput=true,fadewhendone=true}
saydialogs = {narrative='narrative'}
menudialogs = {native={}, Options={SetActive=function(value) end}}
narrative = 'narrator'
function setsaydialog(value) dialog=value end
function setcharacter(value) speaker=value end
function setmenudialog(value) menu=value end
function mod_narrative_left(value) end
function mod_stop_voice() end
function say(text) table.insert(events,text); coroutine.yield() end
function choose(options) coroutine.yield(); if options[1]=='O_ONE' then return native_choice else return segment_choice end end
function mark_progress(value) table.insert(events,'progress:'..tostring(value)) end
checkpointmanager = {Condition=function(key) return outer end}
luamanager = {GetStoryText=function(key) return lookup_text(key) end}
");
            var routine = script.CreateCoroutine(script.LoadString(lua));
            int frame = 0;
            while (routine.Coroutine.State != CoroutineState.Dead && ++frame < 40) routine.Coroutine.Resume();
            var expected = new[] {
                outer ? texts[prefix + "original_4"] : "原版乙",
                "progress:" + ((outer ? 7 : 10) + nativeChoice),
                texts[prefix + "inserted"], texts[prefix + "segment1_0"],
                texts[prefix + (segmentChoice == 1 ? "segment1_2" : "segment1_3")], "原版尾句"
            };
            var actual = script.Globals.Get("events").Table.Values.Select(value => value.String).ToArray();
            if (routine.Coroutine.State != CoroutineState.Dead || !actual.SequenceEqual(expected))
                throw new Exception("Whole segment lost a branch, replayed dialogue or changed native locals: " + string.Join(";", actual));
            if (script.Globals.Get("menu").Table != script.Globals.Get("menudialogs").Table.Get("native").Table
                || script.Globals.Get("sayoptions").Table.Get("waitforinput").Boolean)
                throw new Exception("Whole segment failed to restore the native menu or dialogue timing");
        }
        Console.WriteLine("PASS 8 whole-segment coroutine cases: both native branches, independent choices, native locals and continuation (MoonSharp offline)");
    }
}
