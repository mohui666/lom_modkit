using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MortalModHost
{
    internal interface IScenarioContext
    {
        int Year { get; }
        int Month { get; }
        int Stage { get; }
        int Action { get; }
        object Read(string source, string key);
    }
    internal sealed class ScenarioState
    {
        internal readonly Dictionary<string, int> Variables = new Dictionary<string, int>(StringComparer.Ordinal);
        internal readonly Dictionary<string, string> Tasks = new Dictionary<string, string>(StringComparer.Ordinal);
        internal readonly Dictionary<string, string> Attempts = new Dictionary<string, string>(StringComparer.Ordinal);
        internal readonly Dictionary<string, string> Pools = new Dictionary<string, string>(StringComparer.Ordinal);
        internal readonly HashSet<string> Completed = new HashSet<string>(StringComparer.Ordinal);
        internal string Pending;
        internal string Result = "completed";

        internal static ScenarioState Parse(string json)
        {
            var r = ScenarioJson.Object(MiniJson.Parse(json));
            ScenarioJson.Fields(r, "schema", "variables", "tasks", "attempts", "pools", "completed", "pending", "result");
            ScenarioJson.Int(r,"schema",1,1);
            var state = new ScenarioState();
            var vars = ScenarioJson.Object(ScenarioJson.Value(r,"variables"));
            ScenarioJson.Require(vars.Count <= 256,"存档变量过多");
            foreach (var v in vars) { ScenarioJson.Require(ScenarioJson.ValidId(v.Key),"存档变量键无效"); state.Variables.Add(v.Key,ScenarioJson.Int(vars,v.Key,int.MinValue,int.MaxValue)); }
            foreach (string key in new[] { "tasks", "attempts", "pools" })
            {
                if(key=="pools" && !r.ContainsKey(key))continue;
                var values = ScenarioJson.Object(ScenarioJson.Value(r,key));
                ScenarioJson.Require(values.Count <= 4096,"存档状态过多");
                foreach (var v in values)
                {
                    ScenarioJson.Require(ScenarioJson.ValidId(v.Key) && v.Value is string && ((string)v.Value).Length <= 128,"存档状态无效");
                    (key == "tasks" ? state.Tasks : key=="pools" ? state.Pools : state.Attempts).Add(v.Key,(string)v.Value);
                }
            }
            var completed = ScenarioJson.Array(r,"completed");
            ScenarioJson.Require(completed.Count <= 4096,"存档完成记录过多");
            foreach (object v in completed) { ScenarioJson.Require(ScenarioJson.ValidId(v as string),"完成记录无效"); state.Completed.Add((string)v); }
            state.Pending=ScenarioJson.Text(r,"pending",false);
            ScenarioJson.Require(state.Pending == null || ScenarioJson.ValidId(state.Pending),"存档事件 id 无效");
            state.Result=ScenarioJson.Id(r,"result");
            return state;
        }
        internal static string ToJson(object value) {var b=new StringBuilder();Write(b,value);return b.ToString();}
        internal string Serialize()
        {
            var completed = new List<string>(Completed); completed.Sort(StringComparer.Ordinal);
            var r = new Dictionary<string,object> {
                {"schema",1},{"variables",Variables},{"tasks",Tasks},{"attempts",Attempts},{"pools",Pools},
                {"completed",completed},{"result",Result}
            };
            if (Pending != null) r.Add("pending",Pending);
            var b = new StringBuilder(); Write(b,r); return b.ToString();
        }
        private static void Write(StringBuilder b, object value)
        {
            if (value == null) { b.Append("null"); return; }
            string text = value as string;
            if (text != null)
            {
                b.Append('"');
                foreach (char c in text)
                {
                    if (c == '"' || c == '\\') b.Append('\\').Append(c);
                    else if (c < 32) b.Append("\\u").Append(((int)c).ToString("x4",CultureInfo.InvariantCulture));
                    else b.Append(c);
                }
                b.Append('"'); return;
            }
            var d = value as IDictionary;
            if (d != null)
            {
                var keys = new List<string>(); foreach (object key in d.Keys) keys.Add((string)key); keys.Sort(StringComparer.Ordinal);
                b.Append('{'); bool first=true;
                foreach (string key in keys) { if (!first) b.Append(','); first=false; Write(b,key); b.Append(':'); Write(b,d[key]); }
                b.Append('}'); return;
            }
            var a = value as IEnumerable;
            if (a != null)
            {
                b.Append('['); bool first=true;
                foreach (object v in a) { if (!first) b.Append(','); first=false; Write(b,v); } b.Append(']'); return;
            }
            if (value is bool) b.Append((bool)value ? "true" : "false");
            else b.Append(Convert.ToString(value,CultureInfo.InvariantCulture));
        }
    }
    internal sealed class ScenarioEngine
    {
        internal readonly ScenarioConfig Config;
        internal readonly ScenarioState State;
        private readonly IScenarioContext _context;
        private readonly Func<double> _random;
        internal ScenarioEngine(ScenarioConfig config, ScenarioState state, IScenarioContext context, Func<double> random)
        {
            Config=config; State=state; _context=context; _random=random;
            foreach (ScenarioTask task in config.Tasks.Values)
            {
                string current;
                if (!state.Tasks.TryGetValue(task.Id,out current)) state.Tasks[task.Id]=task.Initial;
                else if (!task.States.Contains(current)) throw new InvalidOperationException("任务存档状态不在当前配置中："+task.Id+"/"+current);
            }
            if (state.Pending != null && config.FindEvent(state.Pending) == null) throw new InvalidOperationException("存档中的进行中事件已不在配置中："+state.Pending);
        }
        internal static bool ModeAllows(string mode, string packageCampaign, string activeCampaign)
        {
            return mode == "addon" ? string.IsNullOrEmpty(activeCampaign) : !string.IsNullOrEmpty(activeCampaign) && string.Equals(packageCampaign,activeCampaign,StringComparison.Ordinal);
        }
        internal ScenarioEvent BeginEntry()
        {
            if (State.Pending != null) throw new InvalidOperationException("已有进行中的事件");
            ScenarioEvent e=Config.FindEvent(Config.EntryEvent);
            if (e == null || e.Trigger != "start" || !e.Enabled) throw new InvalidOperationException("开场事件不可用");
            State.Pending=e.Id; State.Result="completed"; return e;
        }
        internal ScenarioEvent Select(string trigger, string position)
        {
            if (State.Pending != null) return null;
            var transitions=new Dictionary<string,string>(StringComparer.Ordinal);
            foreach (ScenarioTask task in Config.Tasks.Values)
            {
                foreach (object raw in ScenarioJson.Array(task.Raw,"transitions",true))
                {
                    var r=ScenarioJson.Object(raw);
                    if (State.Tasks[task.Id]==ScenarioJson.Text(r,"from") && Test(ScenarioJson.Value(r,"when")))
                    { transitions[task.Id]=ScenarioJson.Text(r,"to"); break; }
                }
            }
            foreach (var change in transitions) State.Tasks[change.Key]=change.Value;
            var candidates=new List<ScenarioEvent>();
            foreach (ScenarioEvent e in Config.Events)
            {
                string attempt;
                if (!e.Enabled || e.Trigger != trigger || (trigger=="position" && e.Position!=position)
                    || e.Once && State.Completed.Contains(e.Id)
                    || e.Pool!=null && State.Pools.TryGetValue(e.Pool,out attempt) && attempt==Token(e.Check)
                    || State.Attempts.TryGetValue(e.Id,out attempt) && attempt==Token(e.Check) || !Test(e.When)) continue;
                candidates.Add(e);
            }
            // List.Sort is not stable; preserve declaration order explicitly for equal priorities.
            candidates.Sort((a,b) => a.Priority != b.Priority ? b.Priority.CompareTo(a.Priority) : Config.Events.IndexOf(a).CompareTo(Config.Events.IndexOf(b)));
            var processed=new HashSet<string>(StringComparer.Ordinal);
            foreach (ScenarioEvent first in candidates)
            {
                if (!processed.Add(first.Id)) continue;
                var group=new List<ScenarioEvent>();
                foreach (ScenarioEvent e in candidates)
                    if (ReferenceEquals(e,first) || first.Pool!=null && first.Pool==e.Pool) { group.Add(e); processed.Add(e.Id); }
                var ready=new List<ScenarioEvent>(); var weights=new List<long>(); long total=0;
                if(first.Pool!=null)State.Pools[first.Pool]=Token(first.Check);
                foreach (ScenarioEvent e in group)
                {
                    State.Attempts[e.Id]=Token(e.Check);
                    // A failed chance draw consumes this period, so reopening a scene cannot reroll it.
                    if (e.Chance <= 0 || e.Chance < 100 && Draw()*100 >= e.Chance) continue;
                    long weight=e.Weight;
                    foreach (object raw in ScenarioJson.Array(e.Raw,"weight_modifiers",true))
                    {
                        var r=ScenarioJson.Object(raw);
                        if (Test(ScenarioJson.Value(r,"when"))) weight+=ScenarioJson.Int(r,"add",-100000,100000);
                    }
                    weight=Math.Max(0,weight);
                    if (weight==0) continue;
                    ready.Add(e); weights.Add(weight); total+=weight;
                }
                if (ready.Count==0) continue;
                int index=0;
                if (first.Pool!=null && ready.Count>1)
                {
                    double roll=Draw()*total;
                    while (index<ready.Count-1 && roll>=weights[index]) { roll-=weights[index]; index++; }
                }
                ScenarioEvent selected=ready[index]; State.Pending=selected.Id; State.Result="completed"; return selected;
            }
            return null;
        }
        internal void SetResult(string result)
        {
            var e=Config.FindEvent(State.Pending);
            if (e==null || !e.Outcomes.ContainsKey(result)) throw new InvalidOperationException("事件结果未在配置中定义："+result);
            State.Result=result;
        }
        internal void Complete(Action<List<object>> applyActions = null, Action<ScenarioState> validateState = null)
        {
            var e=Config.FindEvent(State.Pending);
            if (e==null) throw new InvalidOperationException("没有进行中的事件");
            object raw;
            if (!e.Outcomes.TryGetValue(State.Result,out raw)) throw new InvalidOperationException("当前结果不存在："+State.Result);
            var effects=ScenarioJson.Object(raw);
            var variables=new Dictionary<string,int>(State.Variables,StringComparer.Ordinal);
            object ops;
            if (effects.TryGetValue("variables",out ops))
            {
                foreach (var effect in ScenarioJson.Object(ops))
                {
                    var op=ScenarioJson.Object(effect.Value); int current; variables.TryGetValue(effect.Key,out current);
                    int value=ScenarioJson.Int(op,"value",int.MinValue,int.MaxValue);
                    variables[effect.Key]=ScenarioJson.Text(op,"op")=="set" ? value : checked(current+value);
                }
            }
            if (variables.Count>256) throw new InvalidOperationException("事件变量超过 256 项");
            var next=ScenarioState.Parse(State.Serialize());
            next.Variables.Clear(); foreach (var v in variables) next.Variables.Add(v.Key,v.Value);
            if (effects.TryGetValue("tasks",out ops)) foreach (var t in ScenarioJson.Object(ops)) next.Tasks[t.Key]=(string)t.Value;
            next.Completed.Add(e.Id); next.Pending=null; next.Result="completed";
            if(validateState!=null) validateState(next);
            var actions=ScenarioJson.Array(effects,"actions",true);
            if(actions.Count>0) {
                if(applyActions==null) throw new InvalidOperationException("结果动作执行器未提供");
                applyActions(actions);
            }
            // Commit owned state only after preflight and official actions succeed.
            State.Variables.Clear(); foreach (var v in next.Variables) State.Variables.Add(v.Key,v.Value);
            State.Tasks.Clear(); foreach (var t in next.Tasks) State.Tasks.Add(t.Key,t.Value);
            State.Completed.Add(e.Id); State.Pending=null; State.Result="completed";
        }
        internal void Abort() { State.Pending=null; State.Result="completed"; }
        private string Token(string check)
        {
            string token=_context.Year.ToString(CultureInfo.InvariantCulture)+"/"+_context.Month.ToString(CultureInfo.InvariantCulture);
            if (check!="month") token+="/"+_context.Stage.ToString(CultureInfo.InvariantCulture);
            if (check=="action") token+="/"+_context.Action.ToString(CultureInfo.InvariantCulture);
            return token;
        }
        private double Draw() { double v=_random(); if (double.IsNaN(v) || v<0 || v>=1) throw new InvalidOperationException("随机源必须返回 [0,1)"); return v; }
        internal bool Test(object raw)
        {
            var c=ScenarioJson.Object(raw); object children;
            if (c.TryGetValue("all",out children)) { foreach (object child in (List<object>)children) if (!Test(child)) return false; return true; }
            if (c.TryGetValue("any",out children)) { foreach (object child in (List<object>)children) if (Test(child)) return true; return false; }
            if (c.TryGetValue("not",out children)) return !Test(children);
            string source=ScenarioJson.Text(c,"source"),key=ScenarioJson.Text(c,"key"),op=ScenarioJson.Text(c,"op");
            object actual;
            if (source=="variable") { int n; State.Variables.TryGetValue(key,out n); actual=n; }
            else if (source=="task") actual=State.Tasks[key];
            else if (source=="time") actual=key=="year" ? _context.Year : key=="month" ? _context.Month : key=="stage" ? _context.Stage : _context.Action;
            else actual=_context.Read(source,key);
            if (actual==null) throw new InvalidOperationException("条件值不可用："+source+"/"+key);
            object wanted=ScenarioJson.Value(c,"value");
            int comparison=actual is string ? string.CompareOrdinal((string)actual,(string)wanted) : Convert.ToInt64(actual,CultureInfo.InvariantCulture).CompareTo(Convert.ToInt64(wanted,CultureInfo.InvariantCulture));
            switch (op) { case "eq":return comparison==0; case "ne":return comparison!=0; case "lt":return comparison<0; case "le":return comparison<=0; case "gt":return comparison>0; case "ge":return comparison>=0; default:throw new InvalidOperationException("比较无效"); }
        }
    }
}
