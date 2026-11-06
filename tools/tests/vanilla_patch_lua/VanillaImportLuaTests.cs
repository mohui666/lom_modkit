using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using MoonSharp.Interpreter;
using MortalModHost;

internal static class VanillaImportLuaTests
{
    internal static void Run(string root)
    {
        string original = File.ReadAllText(Path.Combine(root,"import-original.lua"));
        int passed = 0;
        foreach (bool outer in new[]{true,false})
        foreach (int choice in new[]{1,2})
        foreach (string variant in new[]{"baseline","edited","speaker","branch","choice"})
        {
            var source = VanillaImportSource.Parse(File.ReadAllText(Path.Combine(root,"import-"+variant+".json")));
            source.VerifyOriginal(original);
            bool native = true;
            var states = new List<bool>();
            var script = new Script(CoreModules.Preset_Complete);
            script.Globals["outer"] = outer;
            script.Globals["chosen"] = choice;
            script.Globals["capture_source"] = (Action)(()=>states.Add(native));
            script.Globals["mod_begin_vanilla_import"] = (Action<string>)(id=>native=source.Begin());
            script.Globals["mod_vanilla_source_node"] = (Action<string>)(id=>native=source.Node(id));
            script.Globals["mod_vanilla_source_continue"] = new CallbackFunction((ctx,args)=>
            {
                native=source.ContinueOriginal();
                return DynValue.Nil;
            }, "mod_vanilla_source_continue");
            script.DoString(@"
events = {}
dialog, speaker = 'unset', 'unset'
sayoptions = {waitforinput=true,fadewhendone=true}
saydialogs = {narrative='narrative', center='center'}
menudialogs = {native={},Options={SetActive=function(value) end}}
narrative = 'narrator'
function setmenudialog(value) menu=value end
function setsaydialog(value) dialog=value end
function setcharacter(value) speaker=value end
function mod_stop_voice() end
function mod_narrative_left(value) end
function say(text)
    table.insert(events,text..'|'..dialog..'|'..speaker..'|'..tostring(sayoptions.waitforinput))
    capture_source()
    coroutine.yield()
end
function choose(options) coroutine.yield(); return chosen end
function mark_progress(value) table.insert(events,'progress:'..tostring(value)) end
checkpointmanager = {Condition=function(key) return outer end}
luamanager = {GetStoryText=function(key)
    if key=='N_IMPORT_A' then return '原版甲' end
    if key=='N_IMPORT_B' then return '原版乙' end
    if key=='N_IMPORT_DONE' then return '原版尾句' end
    if string.find(key,'inserted') then return '新增对白' end
    return '改写甲'
end}
");
            var routine=script.CreateCoroutine(script.LoadString(File.ReadAllText(Path.Combine(root,"import-"+variant+".lua"))));
            int frames=0;
            while(routine.Coroutine.State!=CoroutineState.Dead && ++frames<40)routine.Coroutine.Resume();
            if(routine.Coroutine.State!=CoroutineState.Dead)throw new Exception("Imported native coroutine did not complete");
            var events=script.Globals.Get("events").Table.Values.Select(v=>v.String).ToArray();
            string first=outer && variant=="edited" ? "改写甲" : outer?"原版甲":"原版乙";
            string nativeLine="|narrative|narrator|false";
            if(events[0]!=first+nativeLine || events[1]!="progress:"+((outer?7:10)+choice)
                || events.Last()!="原版尾句"+nativeLine)
                throw new Exception("Imported scope, branch, choice, timing or speaker changed: "+variant+" "+string.Join(";",events));
            if(variant=="baseline" && states.Any(value=>!value))throw new Exception("Original lines were marked as MOD");
            if(variant=="edited" && (states[0]!=(outer?false:true)||states[1]||!states[2]))
                throw new Exception("Text-only edits and inserted lines did not restore original provenance");
            if(variant=="speaker" && (states[1]||states[2]))throw new Exception("Persistent presentation edits lost MOD attribution");
            if(variant=="branch" && !states.Last())throw new Exception("A read-only flag check failed to restore original provenance");
            if(script.Globals.Get("menu").Table!=script.Globals.Get("menudialogs").Table.Get("native").Table)
                throw new Exception("Inserted choices did not restore the native menu dialog");
            if(variant=="choice" && !states.Last())throw new Exception("An inserted choice failed to restore original provenance");
            passed++;
        }
        string current=Path.Combine(root,"current-import.lua");
        if(File.Exists(current))
        {
            new Script(CoreModules.Preset_Complete).LoadString(File.ReadAllText(current));
            Console.WriteLine("PASS current installed chapter import syntax (MoonSharp offline)");
        }
        Console.WriteLine("PASS "+passed+" full-import coroutine cases: native scopes, both branches, choices, speaker/options restoration and source attribution (MoonSharp offline)");
        RunNestedTerminal(root);
    }

    private static void RunNestedTerminal(string root)
    {
        var source=VanillaImportSource.Parse(File.ReadAllText(Path.Combine(root,"import-terminal.json")));
        source.VerifyOriginal(source.OriginalSource);
        var script=new Script(CoreModules.Preset_Complete);
        script.Globals["mod_begin_vanilla_import"]=(Action<string>)(id=>source.Begin());
        script.Globals["mod_vanilla_source_node"]=(Action<string>)(id=>source.Node(id));
        script.Globals["mod_vanilla_source_continue"]=new CallbackFunction((ctx,args)=>
        {
            source.ContinueOriginal();
            return DynValue.Nil;
        }, "mod_vanilla_source_continue");
        script.DoString(@"
events={}
sayoptions={}
saydialogs={narrative={}}
menudialogs={native={}}
narrative={}
function setsaydialog(value) end
function setcharacter(value) end
function setmenudialog(value) end
function mod_stop_voice() end
function mod_hide_all() end
function mark_progress(value) table.insert(events,'unwanted:'..value) end
checkpointmanager={Condition=function(key)return true end}
luamanager={ChangeScene=function(scene,a,b)table.insert(events,'scene:'..scene) end}
");
        var routine=script.CreateCoroutine(script.LoadString(File.ReadAllText(Path.Combine(root,"import-terminal.lua"))));
        bool finished=false;
        for(int frame=0;frame<20 && routine.Coroutine.State!=CoroutineState.Dead;++frame)
        {
            DynValue yielded=routine.Coroutine.Resume();
            if(yielded.Type==DataType.String && yielded.String==VanillaImportSource.CompletionSignal){finished=true;break;}
        }
        var events=script.Globals.Get("events").Table.Values.Select(value=>value.String).ToArray();
        if(!finished || events.Length!=1 || events[0]!="scene:Free")
            throw new Exception("A custom end inside a native function continued the original chapter");
        Console.WriteLine("PASS terminal step inside a native local function stops the owning coroutine (MoonSharp offline)");
    }
}
