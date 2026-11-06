using System;
using System.Collections.Generic;
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

        internal static void ExportTimeline(string game)
        {
            MissionManagerData manager = MissionManagerData.Instance;
            if (manager == null) throw new InvalidOperationException("当前任务时间线尚未就绪");
            var series = new List<MissionData>();
            if (manager.Main != null) series.Add(manager.Main);
            SubMissionsData subs = Field<SubMissionsData>(manager,"_subMissions");
            if (subs != null) series.AddRange(subs.List);
            var rows = new List<object>();
            foreach (MissionData item in series)
            {
                if (item == null) continue;
                MissionCheckData[] checks = Field<MissionCheckData[]>(item,"_missions");
                if (checks == null) continue;
                foreach (MissionCheckData check in checks)
                {
                    if (check == null || !VanillaStoryPatch.ValidTarget(check.StartScript)) continue;
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
                        {"script",check.StartScript},{"title",title},{"series",item.Name},
                        {"key",check.Key},{"main",ReferenceEquals(item,manager.Main)},
                        {"next_key",check.HasNextState ? check.NextState : ""},
                        {"condition",Conditions(check)},
                        {"place",check.Position.ToString()},{"when",when},{"from_round",from},{"to_round",to},
                        {"active",Field<bool>(check,"_active")}
                    });
                }
            }
            if (rows.Count == 0) throw new InvalidOperationException("没有读到原版任务时间线");
            VanillaBookmarkStore.WriteTimeline(VanillaBookmarkStore.Root,new Dictionary<string,object> {
                {"schema",2},{"game_dir",game},{"game_version",Application.version},{"missions",rows}
            });
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
                    var left = Field<object>(item,"_value1") as UnityEngine.Object;
                    var right = Field<object>(item,"_value2") as UnityEngine.Object;
                    parts.Add((left != null ? left.name : "未指定变量")+" "+Field<StatCompareType>(item,"_compareOp")
                        +" "+(right != null ? right.name : "未指定变量"));
                }
                string note = Field<string>(group,"_devNote");
                string expression = string.Join(" "+Field<LogicOperatorType>(group,"_logicOp")+" ",parts.ToArray());
                groups.Add(string.IsNullOrWhiteSpace(note) ? expression : note+"："+expression);
            }
            return string.Join("；",groups.ToArray());
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
