using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal sealed unsafe class NativeScenarioPlan
    {
        [StructLayout(LayoutKind.Sequential)] private struct Candidate { internal int Priority,Declaration,Pool; }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate uint BuildCallback(Candidate* input,int count,int* order,int* offsets,int* counts,int* members);
        private static readonly BuildCallback BuildNative=NativeHostKernel.Bind<BuildCallback>("lom_scenario_plan_v1");
        internal static bool IsAvailable {get{return BuildNative!=null;}}
        internal readonly int[] Order,Offsets,Counts,Members;
        private NativeScenarioPlan(int count){Order=new int[count];Offsets=new int[count];Counts=new int[count];Members=new int[count];}
        internal static NativeScenarioPlan Build(List<ScenarioEvent> candidates,List<int> declarations)
        {
            if(!IsAvailable)throw new InvalidOperationException("原生事件分组未加载");
            var pools=new Dictionary<string,int>(StringComparer.Ordinal);
            var input=new Candidate[candidates.Count];
            for(int i=0;i<candidates.Count;i++){
                var candidate=candidates[i];int pool=-(i+1);
                if(candidate.Pool!=null&&!pools.TryGetValue(candidate.Pool,out pool)){pool=pools.Count;pools.Add(candidate.Pool,pool);}
                input[i]=new Candidate{Priority=candidate.Priority,Declaration=declarations[i],Pool=pool};
            }
            var plan=new NativeScenarioPlan(candidates.Count);uint status;
            fixed(Candidate* values=input)fixed(int* order=plan.Order)fixed(int* offsets=plan.Offsets)fixed(int* counts=plan.Counts)fixed(int* members=plan.Members)
                status=BuildNative(values,input.Length,order,offsets,counts,members);
            if(status!=1)throw new InvalidOperationException("原生事件分组拒绝了请求："+status);
            return plan;
        }
    }
}
