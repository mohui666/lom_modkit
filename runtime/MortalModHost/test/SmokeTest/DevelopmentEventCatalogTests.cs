using System;
using System.Collections.Generic;
using System.Globalization;

namespace MortalModHost
{
    internal static class DevelopmentEventCatalogTests
    {
        private static int _assertions;
        private static Dictionary<string,object> Row(string key,string next,string series="side",bool main=false,bool active=true,int? from=null,int? to=null)
        {
            return new Dictionary<string,object>{{"title","标题 "+key},{"script","STORY_"+key},{"series",series},
                {"key",key},{"next_key",next},{"main",main},{"active",active},{"condition","条件未判定"},
                {"place","山道"},{"when","时间不明"},{"from_round",from},{"to_round",to}};
        }
        private static void Check(bool value,string message){_assertions++;if(!value)throw new Exception("Event catalog: "+message);}
        private static void Entries(List<DevelopmentEvent> actual,params DevelopmentEvent[] expected)
        {
            Check(actual.Count==expected.Length,"result length");
            for(int i=0;i<expected.Length;++i)Check(ReferenceEquals(actual[i],expected[i]),"input order and duplicate identity "+i);
        }
        private static void Throws<T>(Action run)where T:Exception
        {
            try{run();}catch(T){_assertions++;return;}throw new Exception("Event catalog: expected "+typeof(T).Name);
        }
        internal static void Run()
        {
            _assertions=0;
            Check(DevelopmentEventCatalog.DateRound(1,1,1)==0,"first stage");
            Check(DevelopmentEventCatalog.DateRound(9,12,3)==323,"last stage of year nine");
            for(int round=0;round<=323;++round){
                int year,month,stage;
                Check(DevelopmentEventCatalog.TryDate(round,out year,out month,out stage)
                    && DevelopmentEventCatalog.DateRound(year,month,stage)==round,"date roundtrip "+round);
            }
            foreach(int invalid in new[]{-1,324,int.MinValue,int.MaxValue}){
                int year,month,stage;
                Check(!DevelopmentEventCatalog.TryDate(invalid,out year,out month,out stage) && year==0 && month==0 && stage==0,"invalid date outputs");
            }
            Throws<ArgumentOutOfRangeException>(()=>DevelopmentEventCatalog.DateRound(10,1,1));
            Throws<ArgumentOutOfRangeException>(()=>DevelopmentEventCatalog.DateRound(0,1,1));
            Throws<ArgumentOutOfRangeException>(()=>DevelopmentEventCatalog.DateRound(1,13,1));
            Throws<ArgumentOutOfRangeException>(()=>DevelopmentEventCatalog.DateRound(1,1,0));
            Throws<ArgumentOutOfRangeException>(()=>DevelopmentEventCatalog.DateRound(9,12,4));

            var rows=new List<Dictionary<string,object>>{
                Row("lower","",from:10),Row("upper","",to:20),Row("unknown",""),
                Row("last","",main:true,from:323,to:323),Row("first","",main:true,from:0,to:0),Row("inactive","",active:false)
            };
            rows[2]["when"]="每月上旬（日期范围未知）";
            var calendar=new DevelopmentEventCatalog(rows);var d=calendar.Entries;
            Entries(calendar.Filter(null,0,9,true),d[1],d[2],d[5]);
            Entries(calendar.Filter(null,0,10,true),d[0],d[1],d[2],d[5]);
            Entries(calendar.Filter(null,0,20,false),d[0],d[1],d[2]);
            Entries(calendar.Filter(null,0,21,false),d[0],d[2]);
            Entries(calendar.Filter(null,0,323,false),d[0],d[2],d[3]);
            Entries(calendar.Filter(null,0,0,false),d[1],d[2],d[4]);
            Entries(calendar.Filter("",1,null,false),d[3],d[4]);
            Entries(calendar.Filter(" \t\r\n",2,null,false),d[0],d[1],d[2]);
            Entries(calendar.Filter("not-present",0,null,true));
            Throws<ArgumentOutOfRangeException>(()=>calendar.Filter("",3,null,true));
            rows[0]["title"]="changed after capture";rows.Clear();
            Check(calendar.Entries.Count==6 && calendar.Entries[0].Title!="changed after capture","immutable snapshot");
            Throws<NotSupportedException>(()=>((IList<DevelopmentEvent>)calendar.Entries).Clear());

            rows=new List<Dictionary<string,object>>{
                Row("m0","m1","Main_Old",main:true),Row("m1","m2","Main_New",main:true),
                Row("m1","","Main_Duplicate",main:true),Row("m2","","Main_New",main:true),
                Row("q0","q1","quest"),Row("qx","q1","quest"),Row("q1","q2","quest",active:false),
                Row("q1","","quest"),Row("q2","","quest"),Row("q0","q1","Quest"),
                Row("Q1","","quest"),Row("q0","q2","quest"),Row("m0","m1","quest"),Row("","","quest")
            };
            rows[1]["title"]="终章 归来";rows[1]["script"]="STORY_I_END";rows[1]["condition"]="钱>=10";rows[1]["when"]="第9年12月下旬";
            var catalog=new DevelopmentEventCatalog(rows);var e=catalog.Entries;
            Entries(catalog.Next(e[0]),e[1],e[2]);
            Entries(catalog.Previous(e[1]),e[0]);
            Entries(catalog.Next(e[1]),e[3]);
            Entries(catalog.Previous(e[6]),e[4],e[5]);
            Entries(catalog.Next(e[4]),e[6],e[7]);
            Entries(catalog.Next(e[11]),e[8]);
            Entries(catalog.Previous(e[8]),e[6],e[11]);
            Entries(catalog.Next(e[9]));
            Entries(catalog.Next(e[10]));
            Entries(catalog.Previous(e[10]));
            Entries(catalog.Previous(e[13]));
            Entries(catalog.Next(e[13]));
            Entries(catalog.Next(null));Entries(catalog.Previous(null));
            Entries(catalog.Filter("终章\tstory_i_end 钱>=10",0,null,true),e[1]);
            Entries(catalog.Filter("第9年12月下旬 Main_New m1 山道",0,null,true),e[1]);
            Entries(catalog.Filter("终章 missing-token",0,null,true));
            CultureInfo previous=CultureInfo.CurrentCulture;
            try{CultureInfo.CurrentCulture=new CultureInfo("tr-TR");Entries(catalog.Filter("story_i_end",0,null,true),e[1]);}
            finally{CultureInfo.CurrentCulture=previous;}
            var nextOnly=Row("only","next_only_token");
            Entries(new DevelopmentEventCatalog(new List<Dictionary<string,object>>{nextOnly}).Filter("next_only_token",0,null,true));

            var boxed=Row("numeric","");boxed["from_round"]=10d;boxed["to_round"]=20L;
            var numbers=new DevelopmentEventCatalog(new List<Dictionary<string,object>>{boxed});
            Check(numbers.Entries[0].FromRound==10 && numbers.Entries[0].ToRound==20,"exact boxed numeric input");
            boxed["from_round"]=10.5d;Throws<FormatException>(()=>new DevelopmentEventCatalog(new List<Dictionary<string,object>>{boxed}));
            boxed["from_round"]=double.NaN;Throws<FormatException>(()=>new DevelopmentEventCatalog(new List<Dictionary<string,object>>{boxed}));
            boxed["from_round"]=null;boxed["active"]="false";Throws<FormatException>(()=>new DevelopmentEventCatalog(new List<Dictionary<string,object>>{boxed}));
            var empty=new DevelopmentEventCatalog(new List<Dictionary<string,object>>());
            Check(empty.Entries.Count==0,"empty catalog");Entries(empty.Filter("",0,null,false));
            Throws<ArgumentNullException>(()=>new DevelopmentEventCatalog(null));
            Console.WriteLine("DevelopmentEventCatalogTests PASS: "+_assertions+" assertions; pure data, no game calls");
        }
    }
}
