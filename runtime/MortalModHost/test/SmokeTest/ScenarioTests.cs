using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    internal static class ScenarioTests
    {
        private static Dictionary<string,object> Fixture;
        private static int Assertions;
        private sealed class Context:IScenarioContext
        {
            public int Year {get;set;}=1;
            public int Month {get;set;}=7;
            public int Stage {get;set;}=1;
            public int Action {get;set;}=3;
            public object Read(string source,string key) {if(source=="stat" && key=="money")return 80;throw new InvalidOperationException("missing fixture value");}
        }
        private static void Check(bool ok,string message) {Assertions++;if(!ok)throw new Exception("Scenario test: "+message);}
        private static void Throws(Action action,string message) {try{action();}catch{Assertions++;return;}throw new Exception("Expected scenario failure: "+message);}
        private static Dictionary<string,object> Base()
        {
            return ScenarioJson.Object(MiniJson.Parse(ScenarioState.ToJson(Fixture["config"])));
        }
        private static void Patch(object root,string path,object value,bool remove=false)
        {
            string[] parts=path.Substring(1).Split('/');object current=root;
            for(int i=0;i<parts.Length-1;i++)current=current is List<object> ? ((List<object>)current)[int.Parse(parts[i])] : ScenarioJson.Object(current)[parts[i]];
            string last=parts[parts.Length-1];
            if(current is List<object>) {var list=(List<object>)current;int index=int.Parse(last);if(remove)list.RemoveAt(index);else list[index]=value;}
            else {var d=ScenarioJson.Object(current);if(remove)d.Remove(last);else d[last]=value;}
        }
        private static ScenarioConfig Config(Action<Dictionary<string,object>> edit=null)
        {
            var c=Base();edit?.Invoke(c);var config=ScenarioConfig.Parse(ScenarioState.ToJson(c),"sandbox");config.ValidateScripts(new[]{"main"});return config;
        }
        private static object Json(string json){return MiniJson.Parse(json);}
        internal static void Run()
        {
            Fixture=ScenarioJson.Object(MiniJson.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory,"scenario-contract.json"))));
            int cases=0;
            foreach(object raw in ScenarioJson.Array(Fixture,"cases"))
            {
                var c=ScenarioJson.Object(raw);var root=Base();
                foreach(object patch in ScenarioJson.Array(c,"patches")) {var p=ScenarioJson.Object(patch);object value;p.TryGetValue("value",out value);Patch(root,ScenarioJson.Text(p,"path"),value,ScenarioJson.Bool(p,"remove",false));}
                bool valid=true;string error="";
                try{
                    var config=ScenarioConfig.Parse(ScenarioState.ToJson(root),ScenarioJson.Text(c,"mode",false)??"sandbox");
                    var chapters=new Dictionary<string,string>(StringComparer.Ordinal);
                    foreach(var chapter in ScenarioJson.Object(Fixture["stories"])) chapters.Add(chapter.Key,ScenarioState.ToJson(chapter.Value));
                    config.ValidateScripts(chapters.Keys);
                    config.ValidatePresentations(chapters);
                }
                catch(Exception ex){valid=false;error=ex.Message;}
                Check(valid==ScenarioJson.Bool(c,"valid",false),ScenarioJson.Text(c,"name")+": "+error);cases++;
            }
            TestModes();TestRepeatAndRestore();TestChanceAndCadence();TestPriorityAndWeight();TestResultsAndAtomicState();TestTransitions();TestPresentation();TestPackagedSamples();
            Console.WriteLine("PASS scenario: "+cases+" shared contract cases, "+Assertions+" assertions; game execution=false");
        }
        private static void TestModes()
        {
            Check(ScenarioEngine.ModeAllows("addon","one",null),"addon in official play");
            Check(!ScenarioEngine.ModeAllows("addon","one","one"),"addon excluded from custom campaigns");
            Check(ScenarioEngine.ModeAllows("sandbox","one","one"),"sandbox own campaign");
            Check(!ScenarioEngine.ModeAllows("sandbox","one",null) && !ScenarioEngine.ModeAllows("sandbox","one","two"),"sandbox isolation");
        }
        private static void TestRepeatAndRestore()
        {
            var config=Config();var state=new ScenarioState();var context=new Context();var engine=new ScenarioEngine(config,state,context,()=>0.1);
            Check(engine.BeginEntry().Id=="opening","entry event");engine.Complete();
            Check(engine.Select("position","Center").Id=="visit","pool event");
            var restored=ScenarioState.Parse(state.Serialize());
            Check(restored.Pending=="visit" && restored.Tasks["delivery"]=="active","pending event and task state persisted");
            engine=new ScenarioEngine(config,restored,context,()=>0.1);engine.Complete();
            Check(restored.Variables["visits"]==1 && restored.Tasks["delivery"]=="done","configured outcome");
            Check(engine.Select("position","Center")==null,"same action cannot repeat");
            context.Action--;
            Check(engine.Select("position","Center").Id=="visit","repeat in the same month on next action");engine.Complete();
            Check(restored.Variables["visits"]==2,"repeat applied twice");
            context.Month=8;
            Check(engine.Select("position","Center")!=null,"repeat across month");engine.Abort();
            Check(restored.Variables["visits"]==2 && restored.Pending==null,"abort never settles outcome");
            Throws(()=>ScenarioState.Parse("{\"schema\":2}"),"malformed stored state");
            var incompatible=ScenarioState.Parse(restored.Serialize());incompatible.Tasks["delivery"]="deleted";
            Throws(()=>new ScenarioEngine(config,incompatible,context,()=>0),"unknown saved task state must not reset silently");
        }
        private static ScenarioConfig Single(Action<Dictionary<string,object>> extra=null)
        {
            return Config(c=>{Patch(c,"/events/2/enabled",false);Patch(c,"/events/1/pool",null,true);extra?.Invoke(c);});
        }
        private static void TestChanceAndCadence()
        {
            var config=Single(c=>{Patch(c,"/events/1/check","month");Patch(c,"/events/1/chance",30);});
            var state=new ScenarioState();var context=new Context();int draws=0;
            var engine=new ScenarioEngine(config,state,context,()=>{draws++;return 0.99;});
            Check(engine.Select("position","Center")==null && draws==1,"chance miss");context.Action--;
            Check(engine.Select("position","Center")==null && draws==1,"failed monthly chance cannot reroll");
            var stored=ScenarioState.Parse(state.Serialize());
            engine=new ScenarioEngine(config,stored,context,()=>{draws++;return 0.29;});
            Check(engine.Select("position","Center")==null,"load restores failed chance record");context.Month++;
            Check(engine.Select("position","Center")!=null && draws==2,"next month allows a fresh chance");engine.Complete();
            var once=Single(c=>Patch(c,"/events/1/once",true));context=new Context();engine=new ScenarioEngine(once,new ScenarioState(),context,()=>0);
            engine.Select("position","Center");engine.Complete();context.Action--;
            Check(engine.Select("position","Center")==null,"one time successful event stays complete");
            int zeroDraws=0;var never=Single(c=>Patch(c,"/events/1/chance",0));engine=new ScenarioEngine(never,new ScenarioState(),new Context(),()=>{zeroDraws++;return 0;});
            Check(engine.Select("position","Center")==null && zeroDraws==0,"zero percent never hits");
        }
        private static void TestPriorityAndWeight()
        {
            var config=Config();var low=new ScenarioEngine(config,new ScenarioState(),new Context(),()=>0.6999);
            Check(low.Select("position","Center").Id=="visit","weight below 70 percent boundary");
            var high=new ScenarioEngine(config,new ScenarioState(),new Context(),()=>0.7);
            Check(high.Select("position","Center").Id=="alternate","weight at 70 percent boundary");
            Check(high.Select("auto",null)==null,"pending event blocks concurrent events");
            high.Complete();
            Check(high.Select("position","Center")==null,"a pool has one draw per period even after its selected event completes");
            var modified=Config(c=>Patch(c,"/events/1/weight_modifiers",Json("[{\"when\":{\"all\":[]},\"add\":-70}]")));
            var engine=new ScenarioEngine(modified,new ScenarioState(),new Context(),()=>0);
            Check(engine.Select("position","Center").Id=="alternate","zero effective weight is excluded");
            Throws(()=>new ScenarioEngine(config,new ScenarioState(),new Context(),()=>1).Select("position","Center"),"invalid random source");
            var priority=Config(c=>{Patch(c,"/events/1/pool",null,true);Patch(c,"/events/2/pool",null,true);Patch(c,"/events/2/priority",5);});
            engine=new ScenarioEngine(priority,new ScenarioState(),new Context(),()=>0);
            Check(engine.Select("position","Center").Id=="alternate","explicit priority");
        }
        private static void TestResultsAndAtomicState()
        {
            var config=Single(c=>Patch(c,"/events/1/outcomes/declined",Json("{\"variables\":{\"declined\":{\"op\":\"set\",\"value\":1}},\"tasks\":{\"delivery\":\"locked\"}}")));
            var state=new ScenarioState();var engine=new ScenarioEngine(config,state,new Context(),()=>0);
            engine.Select("position","Center");Throws(()=>engine.SetResult("undefined"),"unknown result");engine.SetResult("declined");
            state=ScenarioState.Parse(state.Serialize());engine=new ScenarioEngine(config,state,new Context(),()=>0);engine.Complete();
            Check(state.Variables["declined"]==1 && state.Tasks["delivery"]=="locked","custom choice result survives save/load");
            state=new ScenarioState();state.Variables["visits"]=int.MaxValue;engine=new ScenarioEngine(Single(),state,new Context(),()=>0);engine.Select("position","Center");
            Throws(()=>engine.Complete(),"overflow");Check(state.Variables["visits"]==int.MaxValue && state.Tasks["delivery"]=="active" && state.Pending!=null,"overflow does not partially commit tasks");
            config=Single(c=>Patch(c,"/events/1/outcomes/completed/actions",Json("[{\"type\":\"stat\",\"key\":\"money\",\"op\":\"add\",\"value\":5}]")));
            state=new ScenarioState();engine=new ScenarioEngine(config,state,new Context(),()=>0);engine.Select("position","Center");
            Throws(()=>engine.Complete(a=>{throw new Exception("synthetic world action failure");}),"failed world actions");
            Check(!state.Variables.ContainsKey("visits") && state.Pending!=null,"world action failure does not report event completion");
            int actions=0;
            string before=state.Serialize();
            Throws(()=>engine.Complete(a=>actions+=a.Count, prepared=>{throw new InvalidOperationException("synthetic final-state size limit");}),"final-state preflight");
            Check(actions==0 && state.Serialize()==before,"state preflight fails before world actions and preserves owned state");
            bool checkedFinal=false;
            engine.Complete(a=>{Check(checkedFinal,"world actions after final-state preflight");actions+=a.Count;},prepared=>{
                Check(prepared.Pending==null && prepared.Completed.Contains(state.Pending),"preflight receives completed state");
                checkedFinal=true;
            });
            Check(actions==1 && state.Pending==null,"result action callback");
        }
        private static void TestTransitions()
        {
            var config=Config(c=>Patch(c,"/tasks",Json("[{\"id\":\"delivery\",\"initial\":\"locked\",\"states\":[\"locked\",\"active\",\"done\"],\"transitions\":[{\"from\":\"locked\",\"to\":\"active\",\"when\":{\"all\":[]}}]},{\"id\":\"dependent\",\"initial\":\"locked\",\"states\":[\"locked\",\"active\"],\"transitions\":[{\"from\":\"locked\",\"to\":\"active\",\"when\":{\"source\":\"task\",\"key\":\"delivery\",\"op\":\"eq\",\"value\":\"active\"}}]}]")));
            var state=new ScenarioState();var engine=new ScenarioEngine(config,state,new Context(),()=>0);
            engine.Select("auto",null);Check(state.Tasks["delivery"]=="active" && state.Tasks["dependent"]=="locked","task transitions use one snapshot");
            engine.Select("auto",null);Check(state.Tasks["dependent"]=="active","dependent transition on next evaluation");
            Check(engine.Test(Json("{\"all\":[]}")) && !engine.Test(Json("{\"any\":[]}")),"empty condition groups");
            Throws(()=>engine.Test(Json("{\"not\":{\"source\":\"stat\",\"key\":\"missing\",\"op\":\"eq\",\"value\":0}}")),"missing official values must not become true under NOT");
        }
        private static void TestPresentation()
        {
            ScenarioConfig.ValidateChapter("{\"nodes\":[{\"type\":\"say\"},{\"type\":\"end\",\"event_result\":\"completed\"}]}");
            var forbidden=ScenarioJson.Array(Fixture,"presentation_forbidden");
            var schema=ScenarioJson.Object(MiniJson.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory,"node-schema.json"))));
            var kinds=ScenarioJson.Object(schema["nodes"]);
            foreach (string kind in kinds.Keys)
            {
                bool accepted=true;
                try { ScenarioConfig.ValidateChapter("{\"nodes\":[{\"type\":\""+kind+"\"},{\"type\":\"end\",\"event_result\":\"completed\"}]}"); }
                catch (FormatException) { accepted=false; }
                Check(accepted==!forbidden.Contains(kind),"shared chapter boundary: "+kind);
            }
            Console.WriteLine("PASS scenario chapter boundary: "+kinds.Count+" shared node types");
            Throws(()=>ScenarioConfig.ValidateChapter("{\"nodes\":[{\"type\":\"mod_quest\"}]}"),"chapter task operation");
            Throws(()=>ScenarioConfig.ValidateChapter("{\"nodes\":[{\"type\":\"result_screen\"}]}"),"embedded rewards belong to event outcomes");
        }
        private static void TestPackagedSamples()
        {
            string folder=Environment.GetEnvironmentVariable("LOM_SCENARIO_PACKAGES");if(string.IsNullOrEmpty(folder))return;
            var errors=new List<string>();var mods=ModLoader.ScanMods(folder,null,errors.Add);
            Check(errors.Count==0,"Rust packages must load in C#: "+string.Join(";",errors));
            Check(mods.Count==2 && mods.Exists(m=>m.PlayMode=="addon" && !m.Campaign.NewGame) && mods.Exists(m=>m.PlayMode=="sandbox" && m.Campaign.NewGame),"both packaged modes");
            Check(mods.Exists(m=>m.PlayMode=="sandbox" && m.LuaScripts[m.Entry].Contains("luamanager.SetGameTime(1, 7, 1)")),"Rust time step exports and loads in Host scenario");
        }
    }
}
