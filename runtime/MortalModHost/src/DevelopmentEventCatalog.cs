using System;
using System.Collections.Generic;

namespace MortalModHost
{
    // A captured task entry and its configured range, not a prediction that
    // its runtime conditions are satisfied in the current saved game.
    internal sealed class DevelopmentEvent
    {
        internal readonly string Title,Script,Series,Key,NextKey,Condition,ConditionSources,Place,When;
        internal readonly bool IsMain,Active;
        internal readonly int? FromRound,ToRound;

        internal DevelopmentEvent(Dictionary<string,object> row)
        {
            if(row==null)throw new ArgumentNullException("row");
            Title=Text(row,"title");Script=Text(row,"script");Series=Text(row,"series");
            Key=Text(row,"key");NextKey=Text(row,"next_key");Condition=Text(row,"condition");
            Place=Text(row,"place");When=Text(row,"when");
            ConditionSources=Text(row,"condition_sources");
            IsMain=Flag(row,"main");Active=Flag(row,"active");
            FromRound=Round(row,"from_round");ToRound=Round(row,"to_round");
        }
        private static string Text(Dictionary<string,object> row,string key)
        {
            object value;
            if(!row.TryGetValue(key,out value)||value==null)return "";
            if(value is string)return (string)value;
            throw new FormatException("事件索引字段 "+key+" 必须是字符串");
        }
        private static bool Flag(Dictionary<string,object> row,string key)
        {
            object value;
            if(row.TryGetValue(key,out value)&&value is bool)return (bool)value;
            throw new FormatException("事件索引字段 "+key+" 必须是布尔值");
        }
        private static int? Round(Dictionary<string,object> row,string key)
        {
            object value;
            if(!row.TryGetValue(key,out value)||value==null)return null;
            if(value is int)return (int)value;
            if(value is long && (long)value>=int.MinValue && (long)value<=int.MaxValue)return (int)(long)value;
            if(value is double) {
                double number=(double)value;
                if(number>=int.MinValue && number<=int.MaxValue && number==Math.Truncate(number))return (int)number;
            }
            throw new FormatException("事件索引字段 "+key+" 必须是整数或空值");
        }
        internal bool Matches(string term)
        {
            return Title.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || When.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || Series.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || Key.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || Script.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || Condition.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || ConditionSources.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0
                || Place.IndexOf(term,StringComparison.OrdinalIgnoreCase)>=0;
        }
    }

    internal sealed class DevelopmentEventCatalog
    {
        private readonly List<DevelopmentEvent> _entries;
        internal IReadOnlyList<DevelopmentEvent> Entries {get;private set;}
        internal DevelopmentEventCatalog(List<Dictionary<string,object>> rows)
        {
            if(rows==null)throw new ArgumentNullException("rows");
            _entries=new List<DevelopmentEvent>(rows.Count);
            foreach(var row in rows)_entries.Add(new DevelopmentEvent(row));
            Entries=_entries.AsReadOnly();
        }
        internal List<DevelopmentEvent> Filter(string search,int kind,int? round,bool includeInactive)
        {
            if(kind<0||kind>2)throw new ArgumentOutOfRangeException("kind");
            string[] terms=string.IsNullOrWhiteSpace(search)?new string[0]
                :search.Split((char[])null,StringSplitOptions.RemoveEmptyEntries);
            var result=new List<DevelopmentEvent>();
            foreach(var entry in _entries) {
                if(!includeInactive && !entry.Active || kind==1 && !entry.IsMain || kind==2 && entry.IsMain)continue;
                if(round.HasValue && (entry.FromRound.HasValue && round.Value<entry.FromRound.Value
                    || entry.ToRound.HasValue && round.Value>entry.ToRound.Value))continue;
                bool match=true;
                foreach(string term in terms)if(!entry.Matches(term)){match=false;break;}
                if(match)result.Add(entry);
            }
            return result;
        }
        private static bool SameSeries(DevelopmentEvent a,DevelopmentEvent b)
        {
            return a.IsMain==b.IsMain && (a.IsMain || string.Equals(a.Series,b.Series,StringComparison.Ordinal));
        }
        internal List<DevelopmentEvent> Previous(DevelopmentEvent entry)
        {
            var result=new List<DevelopmentEvent>();
            if(entry==null || entry.Key.Length==0)return result;
            foreach(var candidate in _entries)
                if(SameSeries(entry,candidate) && string.Equals(candidate.NextKey,entry.Key,StringComparison.Ordinal))result.Add(candidate);
            return result;
        }
        internal List<DevelopmentEvent> Next(DevelopmentEvent entry)
        {
            var result=new List<DevelopmentEvent>();
            if(entry==null || entry.NextKey.Length==0)return result;
            foreach(var candidate in _entries)
                if(SameSeries(entry,candidate) && string.Equals(candidate.Key,entry.NextKey,StringComparison.Ordinal))result.Add(candidate);
            return result;
        }
        internal static int DateRound(int year,int month,int stage)
        {
            if(year<1||year>9)throw new ArgumentOutOfRangeException("year");
            if(month<1||month>12)throw new ArgumentOutOfRangeException("month");
            if(stage<1||stage>3)throw new ArgumentOutOfRangeException("stage");
            return (year-1)*36+(month-1)*3+stage-1;
        }
        internal static bool TryDate(int round,out int year,out int month,out int stage)
        {
            year=month=stage=0;
            if(round<0||round>323)return false;
            year=round/36+1;month=round%36/3+1;stage=round%3+1;
            return true;
        }
    }
}
