using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using System.Text;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using Mortal.Free;
using Mortal.Story;
using OBB.Framework.Utils;

namespace MortalModHost
{
    // Uses live-verified entry points: LuaManager.ChangeScene, FreeManager.Start,
    // MissionManagerData.HasAnyMissionTrigger and PlayerStatManagerData.StoryKeyList.
    // Only our namespaced StorySave record is changed. Official mission assets are untouched.
    internal static class ScenarioRuntime
    {
        internal static ManualLogSource Log;
        private static readonly Random Random = new Random();
        private static readonly UTF8Encoding Utf8 = new UTF8Encoding(false,true);
        internal static string Prefix(ModPackage package) { return "LOMMOD_SCENARIO_V1:"+package.CampaignId+":"; }
        private static List<string> Keys
        {
            get {
                var stat=PlayerStatManagerData.Instance;
                if (stat==null || stat.StoryKeyList==null) throw new InvalidOperationException("事件存档尚未就绪");
                return stat.StoryKeyList;
            }
        }
        internal static ScenarioEngine Open(ModPackage package)
        {
            string prefix=Prefix(package), encoded=null;
            foreach (string key in Keys)
            {
                if (key==null || !key.StartsWith(prefix,StringComparison.Ordinal)) continue;
                if (encoded!=null) throw new InvalidOperationException("事件存档存在重复记录："+package.Id);
                encoded=key.Substring(prefix.Length);
            }
            if (encoded!=null && encoded.Length>180000) throw new InvalidOperationException("事件存档超过上限");
            var state=encoded==null ? new ScenarioState() : ScenarioState.Parse(Utf8.GetString(Convert.FromBase64String(encoded)));
            return new ScenarioEngine(package.Scenario,state,new Context(),Random.NextDouble);
        }
        private static void Store(ModPackage package,ScenarioEngine engine)
        {
            StoreRecord(package,EncodeRecord(package,engine.State));
        }
        private static string EncodeRecord(ModPackage package,ScenarioState state)
        {
            string json=state.Serialize();
            byte[] bytes=Utf8.GetBytes(json);
            if (bytes.Length>128*1024) throw new InvalidOperationException("事件存档超过 128 KiB");
            return Prefix(package)+Convert.ToBase64String(bytes);
        }
        private static void StoreRecord(ModPackage package,string record)
        {
            string prefix=Prefix(package);
            var keys=Keys;
            if (keys.Contains(record)) return;
            keys.RemoveAll(k=>k!=null && k.StartsWith(prefix,StringComparison.Ordinal));
            keys.Add(record);
        }
        internal static string BeginCampaign(ModPackage package)
        {
            var engine=new ScenarioEngine(package.Scenario,new ScenarioState(),new Context(),Random.NextDouble);
            ScenarioEvent entry=engine.BeginEntry(); Store(package,engine);
            return package.GetRegisteredScriptName(entry.Script);
        }
        private static bool Allowed(ModPackage package)
        {
            if (package.Scenario==null) return false;
            var saves=SaveSystem.Instance;
            if(saves==null || package.PlayMode=="addon" && ModSaveIsolation.IsModSlot(saves.CurrentSlot)
                || package.PlayMode=="sandbox" && !CampaignIdentity.OwnsSlot(package.CampaignId,saves.CurrentSlot)) return false;
            return ScenarioEngine.ModeAllows(package.PlayMode,package.CampaignId,ModCampaignState.Active ? ModCampaignState.ActiveCampaignId : null);
        }
        internal static bool TryDispatch(string trigger,string position,out string script)
        {
            script=null;
            if (!Plugin.CanScheduleEvents || Plugin.LoadedMods==null) return false;
            var eligible=new List<ModPackage>();
            foreach (ModPackage package in Plugin.LoadedMods)
            {
                if (!Allowed(package)) continue;
                if (Open(package).State.Pending!=null) return false;
                eligible.Add(package);
            }
            foreach (ModPackage package in eligible)
            {
                var engine=Open(package);
                var selected=engine.Select(trigger,position);
                Store(package,engine);
                if (selected==null) continue;
                script=package.GetRegisteredScriptName(selected.Script);
                Log?.LogInfo("事件触发："+package.Id+"/"+selected.Id+" → "+script);
                return true;
            }
            return false;
        }
        internal static void SetResult(ModPackage package,string result)
        {
            if (package==null || package.Scenario==null) return; // legacy chapters and presentation preview
            var engine=Open(package);
            if (engine.State.Pending==null) return; // isolated F5 presentation does not advance a task
            engine.SetResult(result); Store(package,engine);
        }
        internal static void Complete(LuaManager manager)
        {
            ModPackage package;
            if (manager==null || !ModRegistry.TryGetPackageByRegisteredName(manager.ScriptName,out package) || package.Scenario==null) return;
            var engine=Open(package);
            if (engine.State.Pending==null) return;
            string id=engine.State.Pending,result=engine.State.Result;
            // Validate/encode the final owned state before any official result action.
            // Official actions themselves are not a cross-API transaction.
            string record=null;
            engine.Complete(ApplyActions,state=>record=EncodeRecord(package,state));
            StoreRecord(package,record);
            Log?.LogInfo("事件完成："+package.Id+"/"+id+"，结果 "+result);
        }
        internal static void Abort(ModPackage package)
        {
            if (package==null || package.Scenario==null || PlayerStatManagerData.Instance==null) return;
            var engine=Open(package);
            if (engine.State.Pending==null) return;
            engine.Abort(); Store(package,engine);
        }
        internal static bool OfficialHasPriority(MissionManagerData missions)
        {
            if (VanillaStorySwitch.ShouldSuppress) return false;
            if (missions==null) return true;
            if (missions.MainMissionStart && missions.MainMissionPosition==PositionType.無
                && missions.MainCheckData!=null && !string.IsNullOrEmpty(missions.MainCheckData.StartScript)) return true;
            if (missions.ActiveSubMissions!=null)
                foreach (var pair in missions.ActiveSubMissions)
                    if (pair.Value.Position==PositionType.無 && !string.IsNullOrEmpty(pair.Value.StartScript)) return true;
            return false;
        }
        private static GameItemType ItemType(string kind) {return kind=="book" ? GameItemType.書籍 : kind=="misc" ? GameItemType.雜物 : GameItemType.貴重品;}
        private static void ApplyActions(List<object> actions)
        {
            var stat=PlayerStatManagerData.Instance;
            var modifier=UnityEngine.Object.FindObjectOfType<StatModifyManager>();
            var lua=UnityEngine.Object.FindObjectOfType<LuaManager>();
            if(stat==null || modifier==null || lua==null) throw new InvalidOperationException("事件结果执行器未就绪");
            // Resolve every official identity first: the game's wrappers often silently ignore typos.
            foreach(object raw in actions) {
                var a=ScenarioJson.Object(raw);string type=ScenarioJson.Text(a,"type"),key=ScenarioJson.Text(a,"key",false);
                if(type=="stat") {GameStatType id;if(!EnumUtils.TryParseByStringValue<GameStatType>(key,out id) || stat.Stats.Get(id)==null) throw new InvalidOperationException("结果数值未定义："+key);}
                if(type=="affinity") {RelationshipStatType id;if(!EnumUtils.TryParseByStringValue<RelationshipStatType>(key,out id) || stat.Relationships.Get(id)==null) throw new InvalidOperationException("结果人物未定义："+key);}
                if(type=="game_flag" && MissionManagerData.Instance.GetFlag(key)==null) throw new InvalidOperationException("结果旗标未定义："+key);
                if(type=="talent" && stat.Talents.Get(key)==null) throw new InvalidOperationException("结果技能未定义："+key);
                if(type=="item" && ItemDatabase.Instance.GetItem(ItemType(ScenarioJson.Text(a,"kind")),key)==null) throw new InvalidOperationException("结果物品未定义："+key);
            }
            foreach(object raw in actions) {
                var a=ScenarioJson.Object(raw);string type=ScenarioJson.Text(a,"type"),key=ScenarioJson.Text(a,"key",false),op=ScenarioJson.Text(a,"op",false);
                int n=ScenarioJson.Int(a,"value",int.MinValue,int.MaxValue,0);
                switch(type) {
                    case "stat":if(op=="set")modifier.SetPlayer(key,n);else modifier.Player(key,n,"",0);break;
                    case "affinity":if(op=="set")modifier.SetCharacter(key,n);else modifier.Character(key,n,0);break;
                    case "flag":modifier.AddStory(key);break;
                    case "game_flag":if(op=="set")modifier.SetFlag(key,n);else modifier.AddFlag(key,n);break;
                    case "talent":modifier.AddTalent(key,n);break;
                    case "item":
                        var kind=ItemType(ScenarioJson.Text(a,"kind"));
                        // AddItem delegates to the official AddCount, including its zero clamp.
                        ItemDatabase.Instance.AddItem(kind,key,op=="remove" ? -n : n);break;
                    case "time":lua.SetGameTime(ScenarioJson.Int(a,"year",1,9),ScenarioJson.Int(a,"month",1,12),ScenarioJson.Int(a,"stage",1,3));break;
                    default:throw new InvalidOperationException("未知结果动作："+type);
                }
            }
        }
        private sealed class Context:IScenarioContext
        {
            private PlayerStatManagerData Stat {get {return PlayerStatManagerData.Instance ?? throw new InvalidOperationException("玩家状态未就绪");}}
            public int Year {get {return Stat.GameTime.Year;}}
            public int Month {get {return Stat.GameTime.Month;}}
            public int Stage {get {return (int)Stat.GameTime.Stage;}}
            public int Action {get {return Stat.CurrentActionCount;}}
            public object Read(string source,string key)
            {
                if (source=="flag") return Stat.StoryKeyList.Contains(key) ? 1 : 0;
                if (source=="game_flag") {
                    var flag=MissionManagerData.Instance.GetFlag(key);
                    if (flag==null) throw new InvalidOperationException("原版旗标未定义："+key);
                    return flag.State;
                }
                if (source=="stat") {
                    GameStatType type;
                    if (!EnumUtils.TryParseByStringValue<GameStatType>(key,out type)) throw new InvalidOperationException("角色数值未定义："+key);
                    return Stat.Stats.Get(type).FinalValue;
                }
                if (source=="affinity") {
                    RelationshipStatType type;
                    if (!EnumUtils.TryParseByStringValue<RelationshipStatType>(key,out type)) throw new InvalidOperationException("人物好感未定义："+key);
                    return Stat.Relationships.Get(type).Value;
                }
                throw new InvalidOperationException("未知事件条件来源："+source);
            }
        }
    }
    [HarmonyPatch(typeof(LuaManager),"ChangeScene",new Type[]{typeof(string),typeof(string),typeof(string)})]
    internal static class ScenarioReturnPatch
    {
        [ThreadStatic] internal static bool ReturningToFree;
        private static void Prefix(LuaManager __instance,string name)
        {
            ReturningToFree=name=="Free";
            if (!ReturningToFree && name!="Title" && name!="End" && name!="GameOver") return;
            var scenes=SceneController.Instance;
            if (scenes==null || scenes.IsPrepare || scenes.IsLoading) {ReturningToFree=false;return;}
            // An exception is intentionally propagated to the existing playback abort path:
            // failed task settlement must not be reported as a completed event.
            ScenarioRuntime.Complete(__instance);
        }
        private static Exception Finalizer(Exception __exception) {ReturningToFree=false;return __exception;}
    }
    [HarmonyPatch(typeof(MissionManagerData),"HasAnyMissionTrigger")]
    internal static class ScenarioAutomaticPatch
    {
        private static void Postfix(MissionManagerData __instance,ref bool __result)
        {
            if (!ScenarioReturnPatch.ReturningToFree || __result || ScenarioRuntime.OfficialHasPriority(__instance)) return;
            try {
                string script;
                if (ScenarioRuntime.TryDispatch("auto",null,out script)) {PlayerStatManagerData.Instance.SetStoryScript(script);__result=true;}
            } catch (Exception ex) {ScenarioRuntime.Log?.LogError("自动事件调度失败，未启动剧情："+ex);}
        }
    }
    [HarmonyPatch(typeof(FreeManager),"Start")]
    internal static class ScenarioFreeEntryPatch
    {
        private static readonly FieldInfo Menu=AccessTools.Field(typeof(FreeManager),"_menuOpen");
        private static readonly FieldInfo Save=AccessTools.Field(typeof(FreeManager),"_saveOpen");
        private static readonly FieldInfo Status=AccessTools.Field(typeof(FreeManager),"_statOpen");
        private static void Postfix(FreeManager __instance,ref IEnumerator __result) {
            if(Plugin.LoadedMods!=null && Plugin.LoadedMods.Exists(p=>p.Scenario!=null)) __result=AfterStart(__instance,__result);
        }
        private static IEnumerator AfterStart(FreeManager manager,IEnumerator original)
        {
            yield return original;
            // Current FreeManager.Start starts dialogue coroutines without awaiting them.
            while (manager!=null && SceneController.Instance!=null && SceneController.Instance.CurrentScene=="Free"
                && (manager.IsProcessing || (bool)Menu.GetValue(manager) || (bool)Save.GetValue(manager) || (bool)Status.GetValue(manager))) yield return null;
            if (manager==null || SceneController.Instance==null || SceneController.Instance.CurrentScene!="Free"
                || SceneController.Instance.IsPrepare || SceneController.Instance.IsLoading
                || ScenarioRuntime.OfficialHasPriority(MissionManagerData.Instance)) yield break;
            try {
                string script;
                if (ScenarioRuntime.TryDispatch("auto",null,out script)) {PlayerStatManagerData.Instance.SetStoryScript(script);SceneController.Instance.LoadStory();}
            } catch (Exception ex) {ScenarioRuntime.Log?.LogError("自由模式入口事件调度失败，未启动剧情："+ex);}
        }
    }
}
