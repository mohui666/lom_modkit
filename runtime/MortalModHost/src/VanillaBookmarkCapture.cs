using System;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    internal static class VanillaBookmarkCapture
    {
        private static string _script = "", _key = "", _text = "";
        private static readonly string[] ConditionValueFields = { "_value1", "_value2" };

        internal static void Begin(string script)
        {
            _script = script ?? ""; _key = ""; _text = "";
        }
        internal static void Observe(string script, string key, string text)
        {
            if (!string.Equals(script,_script,StringComparison.OrdinalIgnoreCase)) Begin(script);
            _key = key ?? ""; _text = text ?? "";
        }
        internal static string Mark(string game)
        {
            LuaManager manager = LuaManager.Instance;
            if (SceneController.Instance == null || SceneController.Instance.CurrentScene != "Story"
                || manager == null || !VanillaStoryPatch.ValidTarget(manager.ScriptName))
                throw new InvalidOperationException("请在原版剧情演出中标记");
            string script = manager.ScriptName;
            if (Resources.Load<TextAsset>("Story/ChineseTraditional/"+script) == null)
                throw new InvalidOperationException("当前章节不是本机游戏的原版资源");
            if (!string.Equals(script,_script,StringComparison.OrdinalIgnoreCase)) Begin(script);
            GameTime time = PlayerStatManagerData.Instance.GameTime;
            string path = VanillaBookmarkStore.Write(VanillaBookmarkStore.Root,script,_key,_text,game,
                ReferenceEquals(time,null) ? 0 : time.Year,ReferenceEquals(time,null) ? 0 : time.Month,
                ReferenceEquals(time,null) ? 0 : (int)time.Stage);
            return path;
        }

        internal static List<Dictionary<string,object>> ReadTimelineRows(bool includeWithoutChapter = false)
        {
            MissionManagerData manager = MissionManagerData.Instance;
            if (manager == null) throw new InvalidOperationException("当前任务时间线尚未就绪");
            var series = new List<MissionData>();
            if (manager.Main != null) series.Add(manager.Main);
            SubMissionsData subs = Field<SubMissionsData>(manager,"_subMissions");
            if (subs != null) series.AddRange(subs.List);
            var rows = new List<Dictionary<string,object>>();
            foreach (MissionData item in series)
            {
                if (item == null) continue;
                MissionCheckData[] checks = Field<MissionCheckData[]>(item,"_missions");
                if (checks == null) continue;
                foreach (MissionCheckData check in checks)
                {
                    if (check == null) continue;
                    string script = VanillaStoryPatch.ValidTarget(check.StartScript) ? check.StartScript : "";
                    if (!includeWithoutChapter && script.Length == 0) continue;
                    int? from = null, to = null;
                    string when = "任意时间／条件触发";
                    if (Field<bool>(check,"_dayCheck"))
                    {
                        TimeCheckType type = Field<TimeCheckType>(check,"_timeCheckType");
                        if ((int)type == 1)
                        {
                            from = to = Rounds(Field<GameTime>(check,"_specificTime"));
                            when = Date(from);
                        }
                        else if ((int)type == 2)
                        {
                            from = Rounds(Field<GameTime>(check,"_minTime"));
                            to = Rounds(Field<GameTime>(check,"_maxTime"));
                            when = Date(from)+" ～ "+Date(to);
                        }
                        else if ((int)type == 3) when = "每月"+Stage((int)Field<MonthStageType>(check,"_periodStage"));
                        else when = "时间不定";
                    }
                    string title = Field<string>(check,"_desc") ?? "";
                    if (string.IsNullOrWhiteSpace(title)) title = Field<string>(item,"_desc") ?? "";
                    if (string.IsNullOrWhiteSpace(title)) title = check.Key;
                    rows.Add(new Dictionary<string,object> {
                        {"script",script},{"title",title},{"series",item.Name},
                        {"key",check.Key},{"main",ReferenceEquals(item,manager.Main)},
                        {"next_key",check.HasNextState ? check.NextState : ""},
                        {"condition",Conditions(check)},
                        {"condition_sources",ConditionSources(check)},
                        {"place",check.Position.ToString()},{"when",when},{"from_round",from},{"to_round",to},
                        {"active",Field<bool>(check,"_active")}
                    });
                }
            }
            if (rows.Count == 0) throw new InvalidOperationException("没有读到原版任务时间线");
            return rows;
        }
        internal static List<Dictionary<string,object>> ExportTimeline(string game)
        {
            var rows = ReadTimelineRows();
            VanillaBookmarkStore.WriteTimeline(VanillaBookmarkStore.Root,new Dictionary<string,object> {
                {"schema",2},{"game_dir",game},{"game_version",Application.version},{"missions",rows}
            });
            return rows;
        }
        private static string Conditions(MissionCheckData check)
        {
            var groups = new List<string>();
            ConditionResultItem[] conditions = Field<ConditionResultItem[]>(check,"_conditions");
            if (conditions == null) return "";
            foreach (ConditionResultItem group in conditions)
            {
                if (ReferenceEquals(group,null)) continue;
                var parts = new List<string>();
                var items = Field<List<StatCompareItem>>(group,"_items");
                if (items != null) foreach (StatCompareItem item in items)
                {
                    if (ReferenceEquals(item,null)) continue;
                    var left = Field<StatGroupVariable>(item,"_value1");
                    var right = Field<StatGroupVariable>(item,"_value2");
                    parts.Add(DescribeVariable(left)+" "+Comparison(Field<StatCompareType>(item,"_compareOp"))+" "+DescribeVariable(right));
                }
                string note = Field<string>(group,"_devNote");
                var logic = Field<LogicOperatorType>(group,"_logicOp");
                string join = logic == LogicOperatorType.AND ? " ∧ " : logic == LogicOperatorType.OR ? " ∨ " : " "+logic+" ";
                string expression = string.Join(join,parts.ToArray());
                if (parts.Count > 1) expression = "("+expression+")";
                groups.Add(string.IsNullOrWhiteSpace(note) ? expression : note+"："+expression);
            }
            return string.Join(" ∧ ",groups.ToArray());
        }

        private static string Comparison(StatCompareType comparison)
        {
            switch (comparison)
            {
                case StatCompareType.Equal: return "=";
                case StatCompareType.NotEqual: return "≠";
                case StatCompareType.Less: return "<";
                case StatCompareType.LessEqual: return "≤";
                case StatCompareType.Greater: return ">";
                case StatCompareType.GreaterEqual: return "≥";
                default: return comparison.ToString();
            }
        }

        private static string ConditionSources(MissionCheckData check)
        {
            var names = new List<string>();
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var conditions = Field<ConditionResultItem[]>(check,"_conditions");
            if (conditions == null) return "";
            foreach (var group in conditions)
            {
                if (ReferenceEquals(group,null)) continue;
                var items = Field<List<StatCompareItem>>(group,"_items");
                if (items == null) continue;
                foreach (var item in items)
                {
                    if (ReferenceEquals(item,null)) continue;
                    foreach (string field in ConditionValueFields)
                    {
                        var variable = Field<StatGroupVariable>(item,field);
                        if (variable != null && seen.Add(variable.name)) names.Add(variable.name);
                    }
                }
            }
            return string.Join("\n",names.ToArray());
        }

        private static string DescribeVariable(StatGroupVariable variable)
        {
            if (variable == null) return "未指定变量";
            var values = Field<List<StatValueReference>>(variable,"_value");
            if (values == null || values.Count != 1 || values[0] == null) return variable.name;
            var value = values[0];
            var type = Field<StatGroupType>(variable,"_groupType");
            bool boolean = type == StatGroupType.And || type == StatGroupType.Or;
            if (!boolean && type != StatGroupType.Sum && type != StatGroupType.Min
                && type != StatGroupType.Max && type != StatGroupType.Multiply) return variable.name;
            bool negative = Field<bool>(value,"_negative");
            if (value.CheckType == StatCheckType.常數)
            {
                int number = Field<int>(value,"_constant");
                if (negative) number = unchecked(-number);
                if (boolean) number = number == 0 ? 0 : 1;
                return number.ToString(CultureInfo.InvariantCulture);
            }
            if (!boolean && value.CheckType == StatCheckType.旗標)
            {
                var flag = Field<FlagData>(value,"_flag");
                if (flag != null)
                {
                    string note = (flag.DevNote ?? "").Trim().Replace("\r\n","；").Replace("\r","；").Replace("\n","；");
                    return (negative ? "−" : "")+"旗标 "+flag.name+(note.Length == 0 ? "" : "（"+note+"）");
                }
            }
            // GetValue/GetResult can draw randomness or execute a condition.
            // Keep complex expressions named; never evaluate them to display.
            return variable.name;
        }
        private static T Field<T>(object instance, string name)
        {
            Type type = instance.GetType();
            while (type != null)
            {
                FieldInfo field = type.GetField(name,BindingFlags.Instance|BindingFlags.NonPublic|BindingFlags.Public);
                if (field != null) return (T)field.GetValue(instance);
                type = type.BaseType;
            }
            throw new MissingFieldException(instance.GetType().FullName,name);
        }
        private static int? Rounds(GameTime time)
        { return ReferenceEquals(time,null) || time.Year <= 0 || time.Month < 1 || time.Month > 12 || (int)time.Stage < 1 || (int)time.Stage > 3
            ? (int?)null : (time.Year-1)*36+(time.Month-1)*3+(int)time.Stage-1; }
        private static string Stage(int value) { return value==1 ? "上旬" : value==2 ? "中旬" : value==3 ? "下旬" : "未指定旬"; }
        private static string Date(int? rounds)
        { return !rounds.HasValue ? "时间不定" : "第"+(rounds.Value/36+1)+"年"+(rounds.Value%36/3+1)+"月"+Stage(rounds.Value%3+1); }
    }

    [HarmonyPatch(typeof(LuaManager),"GetStoryText")]
    internal static class VanillaDialogueBookmarkPatch
    {
        private static void Postfix(LuaManager __instance, string key, string __result)
        { VanillaBookmarkCapture.Observe(__instance.ScriptName,key,__result); }
    }
}
