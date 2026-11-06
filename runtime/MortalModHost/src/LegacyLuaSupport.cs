using System;
using System.Reflection;
using System.Runtime.CompilerServices;
using Fungus;
using HarmonyLib;
using MoonSharp.Interpreter;
using Mortal.Core;
using Mortal.Story;
using OBB.Framework.Utils;

namespace MortalModHost
{
    /// <summary>The read-only ext API exported by Binarizer ModSupport's LuaExt.</summary>
    internal static class LegacyLuaSupport
    {
        private sealed class Session
        {
            internal ModPackage Package;
        }
        private static readonly ConditionalWeakTable<Script, Session> Sessions = new ConditionalWeakTable<Script, Session>();

        internal static void Register(Script script, ModPackage package)
        {
            if (script == null || package == null) throw new ArgumentNullException("MOD Lua session");
            LegacyDataTableRuntime.EnsurePackage(package);
            Sessions.Remove(script);
            Sessions.Add(script, new Session { Package = package });
            var ext = new Table(script);
            foreach (string method in new[] { "GetPlayerStat", "GetEnemyTeamStat", "GetRelationship",
                "GetSkillLevel", "GetBookLevel", "GetDeveloped", "GetDevelopd", "GetDevelopLevel",
                "GetFlag", "GetCurrentTime", "GetTimeValue", "GetItemCount" })
            {
                string name = method;
                ext[name] = new CallbackFunction((context, arguments) =>
                    DynValue.NewNumber(Read(name, arguments)), "ext." + name);
            }
            // This interpreter is leased to this MOD. ResetLuaEnvironment destroys it
            // before official scripts or another package can receive these bindings.
            script.Globals["ext"] = ext;
            if (RuntimeTrace.IsDevelopmentPackage(package))
                script.Globals["mod_legacy_probe"] = (Func<string, string>)LegacyDataTableRuntime.ProbeStat;
        }

        private static int Read(string method, CallbackArguments arguments)
        {
            if (method == "GetTimeValue")
                return new GameTime(Integer(arguments, 0), Integer(arguments, 1), (MonthStageType)Integer(arguments, 2)).ConvertToRounds();
            PlayerStatManagerData stat = PlayerStatManagerData.Instance;
            if (stat == null) throw new ScriptRuntimeException("ext 查询时玩家数据尚未初始化");
            if (method == "GetCurrentTime") return stat.GameTime.ConvertToRounds();
            string key = String(arguments, 0);
            switch (method)
            {
                case "GetPlayerStat":
                    GameStatType statType;
                    return EnumUtils.TryParseByStringValue(key, out statType) ? stat.Stats.Get(statType).Value : 0;
                case "GetEnemyTeamStat":
                    var team = stat.EnemyTeam.Get(key);
                    if (team == null) return 0;
                    PropertyInfo property = AccessTools.Property(team.GetType(), String(arguments, 1));
                    var value = property == null ? null : property.GetValue(team, null) as GameStat;
                    return value == null ? 0 : value.Value;
                case "GetRelationship":
                    RelationshipStatType relationship;
                    return EnumUtils.TryParseByStringValue(key, out relationship) ? stat.Relationships.Get(relationship).Value : 0;
                case "GetSkillLevel":
                    var talent = stat.Talents.Get(key);
                    return talent == null ? 0 : talent.Level;
                case "GetBookLevel":
                    var book = ItemDatabase.Instance.Books.Get(key) as Book;
                    return book == null ? 0 : book.CurrentMartialLevel;
                case "GetDeveloped":
                case "GetDevelopd": // The original ToLua exporter emitted this spelling.
                case "GetDevelopLevel":
                    var item = FindDeveloped(stat, key);
                    return item == null ? 0 : method == "GetDevelopLevel" ? item.CurrentLevel : item.AlreadyDevelop ? 1 : 0;
                case "GetFlag":
                    var flag = MissionManagerData.Instance.GetFlag(key);
                    return flag == null ? 0 : flag.State;
                case "GetItemCount":
                    PropertyInfo collectionProperty = AccessTools.Property(typeof(ItemDatabase), key);
                    var collection = collectionProperty == null ? null : collectionProperty.GetValue(ItemDatabase.Instance, null) as ItemDataCollection;
                    if (collection == null) return 0;
                    var counted = collection.Get(String(arguments, 1));
                    if (counted == null) throw new ScriptRuntimeException("ext.GetItemCount 找不到物品：" + key + "/" + String(arguments, 1));
                    return counted.CurrentCount;
                default: throw new ScriptRuntimeException("未知旧 ext 方法：" + method);
            }
        }

        private static UpgradeItemData FindDeveloped(PlayerStatManagerData stat, string name)
        {
            foreach (string field in new[] { "_poisonUpgradeItems", "_weaponUpgradeItems" })
            {
                var collection = Traverse.Create(stat).Field(field).GetValue<UpgradeItemCollectionData>();
                if (collection == null) continue;
                foreach (var item in collection.List) if (item != null && item.name == name) return item;
            }
            return null;
        }
        private static string String(CallbackArguments arguments, int index)
        {
            if (arguments.Count <= index || arguments[index].Type != DataType.String)
                throw new ScriptRuntimeException("ext 参数 " + (index + 1) + " 必须是字符串");
            return arguments[index].String;
        }
        private static int Integer(CallbackArguments arguments, int index)
        {
            if (arguments.Count <= index || arguments[index].Type != DataType.Number
                || arguments[index].Number % 1 != 0 || arguments[index].Number < int.MinValue || arguments[index].Number > int.MaxValue)
                throw new ScriptRuntimeException("ext 参数 " + (index + 1) + " 必须是整数");
            return (int)arguments[index].Number;
        }

        internal static bool Evaluate(string kind, string name, out DynValue value)
        {
            value = null;
            var manager = LuaManager.Instance;
            if (manager == null) return false;
            var environment = Traverse.Create(manager).Field("_luaEnvironment").GetValue<LuaEnvironment>();
            Session session;
            if (environment == null || environment.Interpreter == null || !Sessions.TryGetValue(environment.Interpreter, out session)
                || !ModPlayModeSession.Allows(session.Package)) return false;
            var config = session.Package.LegacyExtensions;
            System.Collections.Generic.Dictionary<string, string> files;
            string source;
            if (config == null || !config.Equivalents.TryGetValue(kind, out files) || !files.TryGetValue(name, out source)) return false;
            value = environment.Interpreter.DoString(source, null, "LuaEquivalent/" + kind + "/" + name + ".lua");
            if (kind == "Condition" && value.Type != DataType.Boolean
                || kind == "Position" && value.Type != DataType.String
                || kind == "Switch" && (value.Type != DataType.Number || value.Number % 1 != 0 || value.Number < int.MinValue || value.Number > int.MaxValue))
                throw new ScriptRuntimeException("LuaEquivalent/" + kind + "/" + name + " 返回类型错误");
            return true;
        }
    }

    [HarmonyPatch(typeof(CheckPointManager), "Condition", new[] { typeof(string) })]
    internal static class LegacyConditionPatch
    {
        private static bool Prefix(string name, ref bool __result)
        {
            DynValue value;
            if (!LegacyLuaSupport.Evaluate("Condition", name, out value)) return true;
            __result = value.Boolean;
            return false;
        }
    }
    [HarmonyPatch(typeof(CheckPointManager), "Switch", new[] { typeof(string) })]
    internal static class LegacySwitchPatch
    {
        private static bool Prefix(string name, ref int __result)
        {
            DynValue value;
            if (!LegacyLuaSupport.Evaluate("Switch", name, out value)) return true;
            __result = (int)value.Number;
            return false;
        }
    }
    [HarmonyPatch(typeof(CheckPointManager), "Position", new[] { typeof(string) })]
    internal static class LegacyPositionPatch
    {
        private static bool Prefix(string name, ref string __result)
        {
            DynValue value;
            if (!LegacyLuaSupport.Evaluate("Position", name, out value)) return true;
            __result = value.String;
            return false;
        }
    }
}
