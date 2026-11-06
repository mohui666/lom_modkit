using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.Loader;
using MoonSharp.Interpreter;
using MortalModHost;

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
        VanillaImportLuaTests.Run(root);
        string source = File.ReadAllText(Path.Combine(root,"original.lua"));
        string wrapper = File.ReadAllText(Path.Combine(root,"compiled.lua"));
        int passed = 0;
        foreach (bool outer in new[] {true,false})
        foreach (bool enabled in new[] {true,false})
        foreach (int value in new[] {0,2})
        foreach (int choice in new[] {1,2})
        {
            Script script = new Script(CoreModules.Preset_Complete);
            script.Globals["outer"] = outer;
            script.Globals["enabled"] = enabled;
            script.Globals["flag_value"] = value;
            script.Globals["choice_value"] = choice;
            script.DoString(@"
events = {}
checkpointmanager = {Condition=function(key) if key=='OUTER' then return outer else return enabled end end}
luamanager = {GetStoryText=function(key) return key end, GetFlagData=function(key) return tostring(flag_value) end}
function say(text) table.insert(events,text); coroutine.yield() end
function choose(options) table.insert(events,'choice:'..options[1]); coroutine.yield(); return choice_value end
function mark_progress(value) table.insert(events,'mark:'..value) end
");
            script.Globals["mod_load_vanilla_story"] = (Func<string,string,DynValue>)((target,json) =>
            {
                if (target != "DEMO") throw new Exception("Wrong original chapter");
                string patched = VanillaStoryPatch.Apply(source,json);
                return DynValue.NewClosure(script.LoadString(patched).Function);
            });
            DynValue coroutine = script.CreateCoroutine(script.LoadString(wrapper));
            int frame = 0;
            while (coroutine.Coroutine.State != CoroutineState.Dead && frame++ < 40)
                coroutine.Coroutine.Resume();
            if (coroutine.Coroutine.State != CoroutineState.Dead) throw new Exception("Patched original script did not finish");
            var expected = new List<string>();
            if (outer)
            {
                if (enabled) expected.Add("extra before");
                expected.Add("D_DEMO");
                if (value >= 2) expected.Add("extra after");
            }
            else expected.Add("alternate");
            expected.Add("mark:"+(outer ? 42 : 41));
            expected.Add("choice:renamed choice");
            expected.Add("mark:"+(choice==1 ? 100 : 200));
            expected.Add("ending");
            string[] actual = script.Globals.Get("events").Table.Values.Select(v=>v.String).ToArray();
            if (!actual.SequenceEqual(expected))
                throw new Exception("Flag insertion changed original flow: "+string.Join(" | ",actual));
            passed++;
        }
        Console.WriteLine("PASS "+passed+" MoonSharp flag/branch/choice/coroutine cases; Unity/game execution: false");
        CheckMessageControls(root);
        return 0;
    }
    private static void CheckMessageControls(string root)
    {
        var script=new Script(CoreModules.Preset_Complete);
        var hints=new List<string>();
        script.Globals["record_hint"]=(Action<string>)(message=>{if(!SystemMessageScope.Suppressed)hints.Add(message);});
        script.Globals["mod_without_system_messages"]=(Func<DynValue,DynValue>)(change=>SystemMessageScope.Run(()=>script.Call(change)));
        script.DoString(@"
stats={lover=0,morale=0}; affinity_total=0; talent_total=0; item_total=0; enemy_total=0; wait_count=0
function mod_set_mood() end
function mod_hide_all() end
function mod_stop_voice() end
function wait() wait_count=wait_count+1 end
luamanager={ChangeScene=function() end}
mainui={DisplayMessageText=function(message) record_hint('message:'..message) end}
statmodifymanager={
 UpdateSetPlayerStat=function(key,value) record_hint('set:'..key) end,
 SetPlayer=function(key,value) stats[key]=value end,
 Player=function(key,value,mode,display) if display==1 then record_hint('stat:'..key) end; stats[key]=stats[key]+value end,
 GetDisplayTime=function() return 1 end,
 Character=function(key,value,display) affinity_total=affinity_total+value; if display==1 then record_hint('affinity:'..key) end end,
 AddTalent=function(key,value) talent_total=talent_total+value; record_hint('talent:'..key) end,
 AddMisc=function(key,value) item_total=item_total+value end,
 ModifyEnemyTeam=function(key,value,display) enemy_total=enemy_total+value; if display==1 then record_hint('enemy:'..key) end end
}
function mod_change_item(category,key,value,show) item_total=item_total+value; if show and value~=0 then record_hint('item:'..key) end end
");
        var routine=script.CreateCoroutine(script.LoadString(File.ReadAllText(Path.Combine(root,"message-controls.lua"))));
        routine.Coroutine.Resume();
        if(routine.Coroutine.State!=CoroutineState.Dead)throw new Exception("Quiet mutations unexpectedly yielded inside their synchronous scope");
        if(script.Globals.Get("stats").Table.Get("lover").Number!=2
            ||script.Globals.Get("stats").Table.Get("morale").Number!=7
            ||script.Globals.Get("affinity_total").Number!=8
            ||script.Globals.Get("talent_total").Number!=3
            ||script.Globals.Get("item_total").Number!=4
            ||script.Globals.Get("enemy_total").Number!=3)
            throw new Exception("Shown or quiet mutations lost their actual values");
        if(hints.Count!=6 || hints.Count(hint=>hint.StartsWith("message:"))!=1
            ||script.Globals.Get("wait_count").Number!=0 ||SystemMessageScope.Suppressed)
            throw new Exception("System messages, explicit result text or display-only waits did not follow their switches: "+string.Join(" | ",hints));
        Console.WriteLine("PASS value writes, legacy displayed sets, quiet stat/affinity/item/talent/reward/enemy changes and explicit result text (MoonSharp offline)");
    }
}
