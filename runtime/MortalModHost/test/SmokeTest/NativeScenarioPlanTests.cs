using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;

namespace MortalModHost
{
    internal static class NativeScenarioPlanTests
    {
        private sealed class Context : IScenarioContext
        {
            internal int Step,Reads;
            public int Year {get{return 1;}}
            public int Month {get{return Step/9+1;}}
            public int Stage {get{return Step/3%3+1;}}
            public int Action {get{return Step%3;}}
            public object Read(string source,string key){Reads++;return Step%5;}
        }
        private sealed class RandomSource
        {
            internal uint Value;internal int Calls;
            internal RandomSource(int seed){Value=(uint)seed;}
            internal double Draw(){Calls++;Value=unchecked(Value*1664525+1013904223);return (Value&0xFFFFFF)/16777216d;}
        }
        private sealed class TimeReads : IScenarioContext
        {
            internal bool Changing;
            internal int Calls;
            internal readonly StringBuilder Trace = new StringBuilder();
            private int ReadTime(char field,int stable,int modulus)
            {
                Trace.Append(field);int call=Calls++;
                return Changing ? call/7%modulus+1 : stable;
            }
            public int Year {get{return ReadTime('Y',1,9);}}
            public int Month {get{return ReadTime('M',4,12);}}
            public int Stage {get{return ReadTime('S',2,3);}}
            public int Action {get{return ReadTime('A',5,20);}}
            public object Read(string source,string key){throw new Exception("time-only fixture read an external stat");}
        }
        private static ScenarioConfig Config(int count,int pools,int seed,bool chanceZero=false)
        {
            var config=new ScenarioConfig{Mode="addon"};var random=new Random(seed);
            for(int i=0;i<count;i++){
                string trigger=chanceZero?"auto":i%3==0?"auto":i%3==1?"position":"story_entry";
                object when=new Dictionary<string,object>{{"all",new List<object>()}};
                if(!chanceZero&&i%7==0)when=new Dictionary<string,object>{{"source","stat"},{"key","money"},{"op","ge"},{"value",2d}};
                var e=new ScenarioEvent{Id="e"+i,Script="main",Trigger=trigger,Position="Center",Target="ch2",From="ch1",Check="stage",Enabled=true,Once=!chanceZero&&i%11==0,
                    Pool=pools==0?null:pools==1?"p"+trigger:"p"+trigger+(i%13),Priority=chanceZero?0:random.Next(-3,4),Weight=i%5,
                    Chance=chanceZero?0:i%6==0?0:i%6==1?12.5:100,When=when,Raw=new Dictionary<string,object>(),
                    Outcomes=new Dictionary<string,object>{{"completed",new Dictionary<string,object>()}}};
                if(i%9==0&&!chanceZero)e.Raw["weight_modifiers"]=new List<object>{new Dictionary<string,object>{{"when",new Dictionary<string,object>{{"all",new List<object>()}}},{"add",-2d}}};
                config.Events.Add(e);
            }
            return config;
        }
        internal static void Run(bool required)
        {
            if(required&&!NativeScenarioPlan.IsAvailable)throw new Exception("scenario plan native missing");
            int cases=0;
            foreach(int count in new[]{16,32,96,512})foreach(int pools in new[]{0,1,2})for(int seed=1;seed<=8;seed++){
                var config=Config(count,pools,seed);var c1=new Context();var c2=new Context();var r1=new RandomSource(seed);var r2=new RandomSource(seed);
                var left=new ScenarioEngine(config,new ScenarioState(),c1,r1.Draw);var right=new ScenarioEngine(config,new ScenarioState(),c2,r2.Draw);
                for(int step=0;step<24;step++){
                    c1.Step=c2.Step=step;string trigger=step%3==0?"auto":step%3==1?"position":"story_entry";
                    ScenarioEvent a=left.SelectManaged(trigger,"Center","ch2","ch1"),b=right.Select(trigger,"Center","ch2","ch1");
                    if((a==null?null:a.Id)!=(b==null?null:b.Id)||r1.Calls!=r2.Calls||c1.Reads!=c2.Reads||left.State.Serialize()!=right.State.Serialize())
                        throw new Exception("scenario plan selection, RNG, read order or state changed; count="+count+" pools="+pools+" seed="+seed+" step="+step);
                    // Abort deliberately consumes the same attempted period and
                    // leaves probability/one-shot semantics available to later steps.
                    left.Abort();right.Abort();
                }
                cases++;
            }
            Console.WriteLine("PASS scenario candidate ordering/groups: "+cases+" seeded state/RNG cases; native="+NativeScenarioPlan.IsAvailable);
            foreach(bool changing in new[]{false,true})
            {
                var config=Config(96,0,1,true);
                for(int i=0;i<config.Events.Count;i++)
                {
                    config.Events[i].Check=i%3==0?"month":i%3==1?"stage":"action";
                    if(i%4==0)config.Events[i].Pool="pool_"+config.Events[i].Check;
                }
                var c1=new TimeReads{Changing=changing};var c2=new TimeReads{Changing=changing};
                var a=new ScenarioEngine(config,new ScenarioState(),c1,()=>0.5);
                var b=new ScenarioEngine(config,new ScenarioState(),c2,()=>0.5);
                for(int step=0;step<4;step++)
                {
                    a.SelectManaged("auto",null,null,null);b.Select("auto",null);
                    if(c1.Calls!=c2.Calls || c1.Trace.ToString()!=c2.Trace.ToString() || a.State.Serialize()!=b.State.Serialize())
                        throw new Exception("period formatting changed property read order, live values or attempted-period state");
                }
            }
            Console.WriteLine("PASS period tokens: stable and changing context, all periods, pools, repeated dispatch and exact property read order");
        }
        internal static double[] Benchmark(bool native,int iterations)
        {
            var config=Config(512,0,1,true);var state=new ScenarioState();var context=new Context();var engine=new ScenarioEngine(config,state,context,()=>0.5);
            var clock=Stopwatch.StartNew();long checksum=0;
            for(int i=0;i<iterations;i++){
                state.Attempts.Clear();state.Pools.Clear();state.Pending=null;
                var value=native?engine.Select("auto",null):engine.SelectManaged("auto",null,null,null);
                if(value!=null)throw new Exception("chance-zero benchmark selected event");checksum+=state.Attempts.Count;
            }
            clock.Stop();return new[]{clock.Elapsed.TotalMilliseconds,(double)checksum};
        }
    }
}
