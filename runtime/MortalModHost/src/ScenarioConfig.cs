using System;
using System.Collections.Generic;

namespace MortalModHost
{
    // Pure authoring contract. Does not depend on Unity or official mission assets.
    internal sealed class ScenarioConfig
    {
        internal string Mode;
        internal string EntryEvent;
        internal readonly List<ScenarioEvent> Events = new List<ScenarioEvent>();
        internal readonly Dictionary<string, ScenarioTask> Tasks = new Dictionary<string, ScenarioTask>(StringComparer.Ordinal);
        internal static ScenarioConfig Parse(string json, string mode)
        {
            var root = ScenarioJson.Object(MiniJson.Parse(json));
            ScenarioJson.Fields(root, "schema", "entry_event", "events", "tasks");
            ScenarioJson.Require(ScenarioJson.Int(root, "schema", 1, 1, 0) == 1, "scenario schema 必须为 1");
            ScenarioJson.Require(mode == "addon" || mode == "sandbox", "play_mode 无效");
            var config = new ScenarioConfig { Mode = mode, EntryEvent = ScenarioJson.Text(root, "entry_event", false) };
            var tasks = ScenarioJson.Array(root, "tasks");
            ScenarioJson.Require(tasks.Count <= 256, "最多 256 条任务");
            foreach (object raw in tasks)
            {
                var t = ScenarioJson.Object(raw);
                ScenarioJson.Fields(t, "id", "name", "initial", "states", "transitions");
                var task = new ScenarioTask { Id = ScenarioJson.Id(t, "id"), Initial = ScenarioJson.Id(t, "initial"), Raw = t };
                foreach (object state in ScenarioJson.Array(t, "states"))
                {
                    ScenarioJson.Require(ScenarioJson.ValidId(state as string) && task.States.Add((string)state), "任务状态无效或重复");
                }
                ScenarioJson.Require(task.States.Count >= 1 && task.States.Count <= 32 && task.States.Contains(task.Initial), "任务 initial/states 无效");
                ScenarioJson.Require(!config.Tasks.ContainsKey(task.Id), "任务 id 重复");
                config.Tasks.Add(task.Id, task);
            }
            foreach (ScenarioTask task in config.Tasks.Values)
            {
                var transitions = ScenarioJson.Array(task.Raw, "transitions", true);
                ScenarioJson.Require(transitions.Count <= 128, "转换过多");
                foreach (object raw in transitions)
                {
                    var r = ScenarioJson.Object(raw);
                    ScenarioJson.Fields(r, "from", "to", "when");
                    string from = ScenarioJson.Id(r, "from"), to = ScenarioJson.Id(r, "to");
                    ScenarioJson.Require(from != to && task.States.Contains(from) && task.States.Contains(to), "转换状态无效");
                    config.ValidateCondition(ScenarioJson.Value(r, "when"), 0);
                }
            }
            var events = ScenarioJson.Array(root, "events");
            ScenarioJson.Require(events.Count >= 1 && events.Count <= 512, "需要 1~512 条事件");
            var ids = new HashSet<string>(StringComparer.Ordinal);
            var pools = new Dictionary<string, string>(StringComparer.Ordinal);
            foreach (object raw in events)
            {
                var e = ScenarioJson.Object(raw);
                ScenarioJson.Fields(e, "id", "name", "enabled", "trigger", "position", "script", "once", "check", "priority", "chance", "pool", "weight", "weight_modifiers", "when", "outcomes");
                var item = new ScenarioEvent {
                    Id = ScenarioJson.Id(e, "id"), Script = ScenarioJson.Id(e, "script"),
                    Trigger = ScenarioJson.Text(e, "trigger"), Position = ScenarioJson.Text(e, "position", false),
                    Check = ScenarioJson.Text(e, "check", false) ?? "stage", Pool = ScenarioJson.Text(e, "pool", false),
                    Enabled = ScenarioJson.Bool(e, "enabled", true), Once = ScenarioJson.Bool(e, "once", true),
                    Priority = ScenarioJson.Int(e, "priority", -100000, 100000, 0), Weight = ScenarioJson.Int(e, "weight", 0, 100000, 1),
                    Chance = ScenarioJson.Number(e, "chance", 0, 100, 100), When = ScenarioJson.Value(e, "when"),
                    Outcomes = ScenarioJson.Object(ScenarioJson.Value(e, "outcomes")), Raw = e
                };
                ScenarioJson.Require(ids.Add(item.Id), "事件 id 重复");
                ScenarioJson.Require(item.Trigger == "start" || item.Trigger == "auto" || item.Trigger == "position", "事件入口无效");
                ScenarioJson.Require(item.Check == "month" || item.Check == "stage" || item.Check == "action", "判定周期无效");
                if (item.Trigger == "position")
                    ScenarioJson.Require(Array.IndexOf(new[] { "Center", "Mall", "Alchemy", "Forge", "BackMountain", "Room1", "Room2", "Door", "Study", "Kitchen", "Secret" }, item.Position) >= 0, "事件位置无效");
                else ScenarioJson.Require(!e.ContainsKey("position"), "非地点事件不能填写 position");
                config.ValidateCondition(item.When, 0);
                var modifiers = ScenarioJson.Array(e, "weight_modifiers", true);
                ScenarioJson.Require(modifiers.Count <= 64, "权重修正过多");
                foreach (object modifier in modifiers)
                {
                    var r = ScenarioJson.Object(modifier);
                    ScenarioJson.Fields(r, "when", "add");
                    ScenarioJson.Int(r, "add", -100000, 100000);
                    config.ValidateCondition(ScenarioJson.Value(r, "when"), 0);
                }
                if (item.Pool != null)
                {
                    ScenarioJson.Require(ScenarioJson.ValidId(item.Pool) && item.Trigger != "start", "事件池无效");
                    string signature = item.Trigger + "\n" + item.Position + "\n" + item.Priority + "\n" + item.Check;
                    string previous;
                    ScenarioJson.Require(!pools.TryGetValue(item.Pool, out previous) || previous == signature, "同池入口/地点/优先级/周期必须一致");
                    pools[item.Pool] = signature;
                }
                ScenarioJson.Require(item.Outcomes.Count <= 32 && item.Outcomes.ContainsKey("completed"), "结果需要 completed，最多 32 项");
                foreach (var outcome in item.Outcomes)
                {
                    ScenarioJson.Require(ScenarioJson.ValidId(outcome.Key), "结果 id 无效");
                    config.ValidateEffects(ScenarioJson.Object(outcome.Value));
                }
                config.Events.Add(item);
            }
            if (mode == "sandbox")
            {
                ScenarioEvent entry = config.FindEvent(config.EntryEvent);
                ScenarioJson.Require(entry != null && entry.Trigger == "start", "entry_event 必须引用开场事件");
                ScenarioJson.Require(entry.Enabled && entry.Chance==100 && ScenarioState.ToJson(entry.When)=="{\"all\":[]}" && config.Events.FindAll(e=>e.Trigger=="start").Count==1,"开场事件必须唯一、启用、概率 100 且无条件");
            }
            else ScenarioJson.Require(!root.ContainsKey("entry_event") && !config.Events.Exists(e=>e.Trigger=="start"), "附加剧情不能设置开场事件");
            return config;
        }
        internal ScenarioEvent FindEvent(string id) { return Events.Find(e => string.Equals(e.Id, id, StringComparison.Ordinal)); }
        internal void ValidateScripts(ICollection<string> scripts)
        {
            foreach(ScenarioEvent e in Events) ScenarioJson.Require(scripts.Contains(e.Script),"事件引用不存在的剧情："+e.Script);
        }
        internal static void ValidateChapter(string json)
        {
            var chapter=ScenarioJson.Object(MiniJson.Parse(json));
            foreach(object raw in ScenarioJson.Array(chapter,"nodes")) {
                var node=ScenarioJson.Object(raw);string kind=ScenarioJson.Text(node,"type");
                // 与 Rust scenario::performance_node 保持一致：章节可直接调整时间和属性，
                // 跨事件任务/持久状态和统一结算仍由 scenario.json 管理，高级 Lua 不在此模式开放。
                ScenarioJson.Require(Array.IndexOf(new[]{"free_trigger","mod_quest","quest_check","persistent_var","persistent_check","reward","result_screen","raw"},kind)<0,"剧情章节含事件/任务或结果动作："+kind);
                ScenarioJson.Require(kind!="end" || !node.ContainsKey("event_result") || !node.ContainsKey("next_script"),"章节结果不能与 next_script 混用");
            }
        }
        internal void ValidatePresentations(Dictionary<string,string> chapters)
        {
            var data=new Dictionary<string,Dictionary<string,object>>(StringComparer.Ordinal);
            foreach(var chapter in chapters) {ValidateChapter(chapter.Value);data[chapter.Key]=ScenarioJson.Object(MiniJson.Parse(chapter.Value));}
            foreach(ScenarioEvent e in Events) {
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
        private void ValidateEffects(Dictionary<string, object> value)
        {
            ScenarioJson.Fields(value, "variables", "tasks", "actions");
            var actions=ScenarioJson.Array(value,"actions",true);
            ScenarioJson.Require(actions.Count<=64,"结果动作过多");
            foreach (object rawAction in actions)
            {
                var a=ScenarioJson.Object(rawAction); string type=ScenarioJson.Text(a,"type");
                if (type=="time") {
                    ScenarioJson.Fields(a,"type","year","month","stage");
                    ScenarioJson.Int(a,"year",1,9);ScenarioJson.Int(a,"month",1,12);ScenarioJson.Int(a,"stage",1,3);continue;
                }
                ScenarioJson.Require(Array.IndexOf(new[]{"stat","affinity","flag","game_flag","item","talent"},type)>=0,"结果动作类型无效");
                if(type=="flag") ScenarioJson.Fields(a,"type","key");
                else if(type=="talent") ScenarioJson.Fields(a,"type","key","value");
                else if(type=="item") ScenarioJson.Fields(a,"type","key","kind","op","value");
                else ScenarioJson.Fields(a,"type","key","op","value");
                ScenarioJson.Id(a,"key");
                if(type=="flag")continue;
                int n=ScenarioJson.Int(a,"value",int.MinValue,int.MaxValue);
                if(type=="talent") {ScenarioJson.Require(n==1 || n== -1,"技能变动无效");continue;}
                string op=ScenarioJson.Text(a,"op");
                if(type=="item") ScenarioJson.Require((op=="add" || op=="remove") && n>0 && Array.IndexOf(new[]{"book","misc","special"},ScenarioJson.Text(a,"kind"))>=0,"物品动作无效");
                else ScenarioJson.Require(op=="add" || op=="set","结果动作操作无效");
            }
            foreach (string field in new[] { "variables", "tasks" })
            {
                object raw;
                if (!value.TryGetValue(field, out raw)) continue;
                var effects = ScenarioJson.Object(raw);
                ScenarioJson.Require(effects.Count <= 256, "结果项过多");
                foreach (var effect in effects)
                {
                    ScenarioJson.Require(ScenarioJson.ValidId(effect.Key), "结果键无效");
                    if (field == "tasks")
                        ScenarioJson.Require(Tasks.ContainsKey(effect.Key) && Tasks[effect.Key].States.Contains(effect.Value as string), "结果引用了未知任务状态");
                    else
                    {
                        var op = ScenarioJson.Object(effect.Value);
                        ScenarioJson.Fields(op, "op", "value");
                        string kind = ScenarioJson.Text(op, "op");
                        ScenarioJson.Require(kind == "set" || kind == "add", "变量操作无效");
                        ScenarioJson.Int(op, "value", int.MinValue, int.MaxValue);
                    }
                }
            }
        }
        private void ValidateCondition(object raw, int depth)
        {
            ScenarioJson.Require(depth <= 12, "条件嵌套过深");
            var c = ScenarioJson.Object(raw);
            foreach (string kind in new[] { "all", "any", "not" })
            {
                object child;
                if (!c.TryGetValue(kind, out child)) continue;
                ScenarioJson.Require(c.Count == 1, "组合条件不能混用字段");
                if (kind == "not") ValidateCondition(child, depth + 1);
                else
                {
                    var children = ScenarioJson.Array(c, kind);
                    ScenarioJson.Require(children.Count <= 64, "条件项过多");
                    foreach (object item in children) ValidateCondition(item, depth + 1);
                }
                return;
            }
            ScenarioJson.Fields(c, "source", "key", "op", "value");
            string source = ScenarioJson.Text(c, "source"), key = ScenarioJson.Text(c, "key"), op = ScenarioJson.Text(c, "op");
            ScenarioJson.Require(Array.IndexOf(new[] { "time", "stat", "affinity", "flag", "game_flag", "variable", "task" }, source) >= 0, "条件来源无效");
            ScenarioJson.Require(Array.IndexOf(new[] { "eq", "ne", "lt", "le", "gt", "ge" }, op) >= 0, "条件比较无效");
            if (source == "time") ScenarioJson.Require(Array.IndexOf(new[] { "year", "month", "stage", "action" }, key) >= 0, "时间字段无效");
            else ScenarioJson.Require(ScenarioJson.ValidId(key), "条件键无效");
            if (source == "task") ScenarioJson.Require((op == "eq" || op == "ne") && Tasks.ContainsKey(key) && Tasks[key].States.Contains(ScenarioJson.Text(c, "value")), "任务条件无效");
            else ScenarioJson.Int(c, "value", int.MinValue, int.MaxValue);
        }
    }
    internal sealed class ScenarioEvent
    {
        internal string Id, Script, Trigger, Position, Check, Pool;
        internal bool Enabled, Once;
        internal int Priority, Weight;
        internal double Chance;
        internal object When;
        internal Dictionary<string, object> Outcomes, Raw;
    }
    internal sealed class ScenarioTask
    {
        internal string Id, Initial;
        internal readonly HashSet<string> States = new HashSet<string>(StringComparer.Ordinal);
        internal Dictionary<string, object> Raw;
    }
    internal static class ScenarioJson
    {
        internal static void Require(bool ok, string message) { if (!ok) throw new FormatException("scenario: " + message); }
        internal static Dictionary<string, object> Object(object value) { var d = value as Dictionary<string, object>; Require(d != null, "必须为对象"); return d; }
        internal static object Value(Dictionary<string, object> d, string key) { object v; Require(d.TryGetValue(key, out v), "缺少 " + key); return v; }
        internal static List<object> Array(Dictionary<string, object> d, string key, bool optional = false)
        {
            object v; if (!d.TryGetValue(key, out v) && optional) return new List<object>();
            var a = v as List<object>; Require(a != null, key + " 必须为数组"); return a;
        }
        internal static string Text(Dictionary<string, object> d, string key, bool required = true)
        {
            object v; if (!d.TryGetValue(key, out v) && !required) return null;
            Require(v is string, key + " 必须是字符串"); return (string)v;
        }
        internal static bool ValidId(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > 64) return false;
            foreach (char c in value) if (!(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_' || c == '-')) return false;
            return true;
        }
        internal static string Id(Dictionary<string, object> d, string key) { string v = Text(d,key); Require(ValidId(v),key+" id 无效"); return v; }
        internal static int Int(Dictionary<string, object> d, string key, int min, int max, int? fallback = null)
        {
            object v; if (!d.TryGetValue(key,out v) && fallback.HasValue) return fallback.Value;
            Require(v is double && !double.IsNaN((double)v) && !double.IsInfinity((double)v) && (double)v >= min && (double)v <= max && (double)v % 1 == 0, key+" 必须是整数且在范围内"); return (int)(double)v;
        }
        internal static double Number(Dictionary<string, object> d, string key, double min, double max, double fallback)
        {
            object v; if (!d.TryGetValue(key,out v)) return fallback;
            Require(v is double && !double.IsNaN((double)v) && !double.IsInfinity((double)v) && (double)v >= min && (double)v <= max,key+" 数值无效"); return (double)v;
        }
        internal static bool Bool(Dictionary<string, object> d, string key, bool fallback) { object v; if (!d.TryGetValue(key,out v)) return fallback; Require(v is bool,key+" 必须是布尔值"); return (bool)v; }
        internal static void Fields(Dictionary<string, object> d, params string[] keys) { foreach (string key in d.Keys) Require(System.Array.IndexOf(keys,key)>=0,"未知字段 "+key); }
    }
}
