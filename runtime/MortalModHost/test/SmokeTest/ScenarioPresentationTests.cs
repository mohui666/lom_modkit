using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Text;

namespace MortalModHost
{
    internal static class ScenarioPresentationTests
    {
        // Frozen pre-optimization algorithms. Their parse, boundary and DFS
        // ordering is the reference, including errors on malformed old values.
        private static void OriginalChapter(string json)
        {
            var chapter=ScenarioJson.Object(MiniJson.Parse(json));
            foreach(object raw in ScenarioJson.Array(chapter,"nodes")) {
                var node=ScenarioJson.Object(raw);string kind=ScenarioJson.Text(node,"type");
                ScenarioJson.Require(chapter.ContainsKey("vanilla_import") || Array.IndexOf(new[]{"free_trigger","mod_quest","quest_check","persistent_var","persistent_check","reward","result_screen","raw"},kind)<0,"剧情章节含事件/任务或结果动作："+kind);
                ScenarioJson.Require(kind!="end" || !node.ContainsKey("event_result") || !node.ContainsKey("next_script"),"章节结果不能与 next_script 混用");
            }
        }
        private static void Original(ScenarioConfig config,Dictionary<string,string> chapters)
        {
            var data=new Dictionary<string,Dictionary<string,object>>(StringComparer.Ordinal);
            foreach(var chapter in chapters) {OriginalChapter(chapter.Value);data[chapter.Key]=ScenarioJson.Object(MiniJson.Parse(chapter.Value));}
            foreach(ScenarioEvent e in config.Events) {
                var pending=new Stack<string>();var seen=new HashSet<string>(StringComparer.Ordinal);pending.Push(e.Script);
                while(pending.Count>0) {
                    string id=pending.Pop();if(!seen.Add(id))continue;
                    ScenarioJson.Require(data.ContainsKey(id),"缺少演出章节："+id);
                    foreach(object raw in ScenarioJson.Array(data[id],"nodes")) {
                        var node=ScenarioJson.Object(raw);if(ScenarioJson.Text(node,"type")!="end")continue;
                        string next=ScenarioJson.Text(node,"next_script",false);
                        if(next!=null)pending.Push(next);
                        else ScenarioJson.Require(e.Outcomes.ContainsKey(ScenarioJson.Text(node,"event_result",false)??"completed"),"演出回报了未配置结果："+id);
                    }
                }
            }
        }
        private static string Capture(Action run)
        {
            try {run();return "accepted";}
            catch(Exception error){return error.GetType().FullName+":"+error.Message;}
        }
        private static ScenarioConfig Config(params string[] scripts)
        {
            var config=new ScenarioConfig();
            foreach(string script in scripts)config.Events.Add(new ScenarioEvent {Script=script,Outcomes=new Dictionary<string,object>{{"completed",null}}});
            return config;
        }
        private static string Chapter(params string[] nodes){return "{\"nodes\":["+string.Join(",",nodes)+"]}";}
        private static string End(string fields){return "{\"type\":\"end\""+(fields.Length>0?","+fields:"")+"}";}
        private static string Next(string id){return End("\"next_script\":\""+id+"\"");}
        private static int _cases;
        private static void Compare(string name,ScenarioConfig config,Dictionary<string,string> chapters)
        {
            string expected=Capture(()=>Original(config,chapters)),actual=Capture(()=>config.ValidatePresentations(chapters));
            if(expected!=actual)throw new Exception("Presentation differential "+name+": expected="+expected+"; actual="+actual);
            _cases++;
        }
        private static void ChapterCase(string json)
        {
            string expected=Capture(()=>OriginalChapter(json)),actual=Capture(()=>ScenarioConfig.ValidateChapter(json));
            if(expected!=actual)throw new Exception("Chapter differential expected="+expected+"; actual="+actual);
            Compare("single chapter",Config("a"),new Dictionary<string,string>{{"a",json}});
            _cases++;
        }
        internal static void Run()
        {
            _cases=0;
            foreach(string value in new[]{null,"", "null", "[]", "{}", "{\"nodes\":null}","{\"nodes\":{}}",Chapter(),Chapter("null"),Chapter("1"),Chapter("{}"),Chapter("{\"type\":null}"),Chapter("{\"type\":1}"),Chapter("{\"type\":\"unknown_old_type\"}")})ChapterCase(value);
            foreach(string kind in new[]{"free_trigger","mod_quest","quest_check","persistent_var","persistent_check","reward","result_screen","raw","say","end","old_unknown"}) {
                string json=Chapter("{\"type\":\""+kind+"\"}");ChapterCase(json);ChapterCase(json.Insert(1,"\"vanilla_import\":null,"));
            }
            foreach(string fields in new[]{"","\"next_script\":null","\"next_script\":false","\"next_script\":1","\"next_script\":\"\"","\"event_result\":null","\"event_result\":true","\"event_result\":\"completed\"","\"event_result\":\"other\"","\"event_result\":null,\"next_script\":null","\"event_result\":false,\"next_script\":\"missing\""}) {
                string json=Chapter(End(fields));ChapterCase(json);
                Compare("unvisited malformed end",Config("a"),new Dictionary<string,string>{{"a",Chapter(End(""))},{"unvisited",json}});
                Compare("no events",Config(),new Dictionary<string,string>{{"unvisited",json}});
            }
            foreach(string json in new[]{"{\"nodes\":null,\"nodes\":[]}","{\"nodes\":[],\"nodes\":null}",Chapter("{\"type\":null,\"type\":\"say\"}"),Chapter("{\"type\":\"say\",\"type\":null}"),Chapter(End("\"event_result\":null,\"event_result\":\"completed\"")),Chapter(End("\"next_script\":false,\"next_script\":\"a\""))})ChapterCase(json);
            Compare("LIFO missing branch",Config("a"),new Dictionary<string,string>{{"a",Chapter(Next("first_missing"),Next("last_missing"))}});
            Compare("late field before queued missing",Config("a"),new Dictionary<string,string>{{"a",Chapter(Next("missing"),End("\"next_script\":null"))}});
            Compare("all boundaries before graph",Config("missing"),new Dictionary<string,string>{{"a",Chapter(End("\"next_script\":null"))},{"b",Chapter("{\"type\":\"raw\"}")}});
            Compare("next chapter boundary before bad result",Config("a"),new Dictionary<string,string>{{"a",Chapter(End("\"event_result\":false"))},{"b",Chapter("{\"type\":null}")}});
            Compare("cycle",Config("a","b"),new Dictionary<string,string>{{"a",Chapter(Next("b"))},{"b",Chapter(Next("a"))}});
            Compare("diamond",Config("a"),new Dictionary<string,string>{{"a",Chapter(Next("b"),Next("c"))},{"b",Chapter(Next("d"))},{"c",Chapter(Next("d"))},{"d",Chapter(End(""))}});
            var outcomes=Config("a","a");outcomes.Events[1].Outcomes=new Dictionary<string,object>{{"other",null}};
            Compare("outcomes vary by event",outcomes,new Dictionary<string,string>{{"a",Chapter(End(""))}});
            Compare("null script",Config((string)null),new Dictionary<string,string>{{"a",Chapter(End(""))}});
            var nullOutcomes=Config("a");nullOutcomes.Events[0].Outcomes=null;
            Compare("outcomes unused in cycle",nullOutcomes,new Dictionary<string,string>{{"a",Chapter(Next("a"))}});
            Compare("outcomes null reached",nullOutcomes,new Dictionary<string,string>{{"a",Chapter(End(""))}});
            Compare("bad result before null outcomes",nullOutcomes,new Dictionary<string,string>{{"a",Chapter(End("\"event_result\":null"))}});
            var random=new Random(1431);
            for(int c=0;c<150;++c){
                var chapters=new Dictionary<string,string>(StringComparer.Ordinal);var config=new ScenarioConfig();
                for(int i=0;i<6;++i){var nodes=new List<string>{"{\"type\":\"say\",\"text\":\"正文\"}"};
                    for(int j=0;j<random.Next(5);++j){int shape=random.Next(6);nodes.Add(shape<3?Next("c"+random.Next(8)):shape==3?End("\"event_result\":\"other\""):shape==4?End("\"event_result\":null"):End(""));}
                    if(random.Next(20)==0)nodes.Add("{\"type\":\"raw\"}");chapters.Add("c"+i,Chapter(nodes.ToArray()));}
                for(int i=0;i<4;++i)config.Events.Add(new ScenarioEvent{Script="c"+random.Next(8),Outcomes=new Dictionary<string,object>{{random.Next(2)==0?"completed":"other",null}}});
                Compare("random "+c,config,chapters);
            }
            Console.WriteLine("PASS presentation validation: "+_cases+" frozen-order differential cases; no native graph module");
        }
        internal static void ReleasedFixture(string fixture,string outputDirectory)
        {
            byte[] original=File.ReadAllBytes(fixture);
            var chapters=new Dictionary<string,string>(StringComparer.Ordinal);string manifest=null,scenario=null;
            using(var input=new MemoryStream(original))using(var zip=new ZipArchive(input,ZipArchiveMode.Read))
                foreach(var entry in zip.Entries) {
                    if(entry.FullName!="manifest.json" && entry.FullName!="scenario.json" && !(entry.FullName.StartsWith("story/",StringComparison.Ordinal) && entry.FullName.EndsWith(".json",StringComparison.Ordinal)))continue;
                    string text;using(var reader=new StreamReader(entry.Open(),Encoding.UTF8))text=reader.ReadToEnd();
                    if(entry.FullName=="manifest.json")manifest=text;
                    else if(entry.FullName=="scenario.json")scenario=text;
                    else chapters.Add(entry.FullName.Substring(6,entry.FullName.Length-11),text);
                }
            var metadata=ScenarioJson.Object(MiniJson.Parse(manifest));
            var config=ScenarioConfig.Parse(scenario,ScenarioJson.Text(metadata,"play_mode"));
            Original(config,chapters);config.ValidatePresentations(chapters);
            Directory.CreateDirectory(outputDirectory);
            var edited=new Dictionary<string,string>(StringComparer.Ordinal);
            foreach(var chapter in chapters) {
                int end=chapter.Value.LastIndexOf('}');
                string json=chapter.Value.Substring(0,end)+",\"title\":\"继续编辑历史章节\"}";
                string path=Path.Combine(outputDirectory,Path.GetFileNameWithoutExtension(fixture)+"-"+chapter.Key+".json");
                File.WriteAllText(path,json,new UTF8Encoding(false));edited.Add(chapter.Key,File.ReadAllText(path,Encoding.UTF8));
            }
            Original(config,edited);config.ValidatePresentations(edited);
            if(Convert.ToBase64String(original)!=Convert.ToBase64String(File.ReadAllBytes(fixture)))throw new Exception("Released fixture changed");
            Console.WriteLine("PASS presentation historical JSON open/edit/save/reopen: "+Path.GetFileName(fixture));
        }
        internal static Action CreateBenchmark(bool original,bool dense)
        {
            var chapters=new Dictionary<string,string>(StringComparer.Ordinal);var config=new ScenarioConfig();
            for(int c=0;c<8;++c){var text=new StringBuilder("{\"nodes\":[");
                for(int n=0;n<1500;++n){if(n>0)text.Append(',');text.Append("{\"id\":\"n").Append(n).Append("\",\"type\":\"say\",\"text\":\"用于成本测量的较长正文内容\",\"font_size\":40}");}
                for(int e=0;e<(dense?50:1);++e)text.Append(',').Append(c<7?Next("c"+(c+1)):End(""));chapters.Add("c"+c,text.Append("]}").ToString());}
            for(int e=0;e<512;++e)config.Events.Add(new ScenarioEvent{Script="c0",Outcomes=new Dictionary<string,object>{{"completed",null}}});
            Action run=original?(Action)(()=>Original(config,chapters)):(()=>config.ValidatePresentations(chapters));
            return run;
        }
        internal static double[] Benchmark(bool original,int iterations,bool dense)
        {
            var run=CreateBenchmark(original,dense);
            run();var timer=Stopwatch.StartNew();for(int i=0;i<iterations;++i)run();timer.Stop();
            return new[]{timer.Elapsed.TotalMilliseconds,(double)(iterations*8*512)};
        }
    }
}
