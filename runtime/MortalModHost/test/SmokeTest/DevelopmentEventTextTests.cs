using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal static class DevelopmentEventTextTests
    {
        private static int _assertions;
        private static void Check(bool value,string message)
        {_assertions++;if(!value)throw new Exception("Event text: "+message);}
        internal static void Run()
        {
            _assertions=0;DevelopmentEventText.Clear();
            bool windows=Environment.OSVersion.Platform==PlatformID.Win32NT;
            try {
                Check(DevelopmentEventText.ToSimplified(null)==null && DevelopmentEventText.ToTraditional(null)==null,"null preserved");
                Check(DevelopmentEventText.ToSimplified("")=="" && DevelopmentEventText.ToTraditional("")=="","empty preserved");
                foreach(string locale in new[]{null,"","chs","cht","ja","ko","en"}) {
                    Check(DevelopmentEventText.Display(null,locale)==null,"display null");
                    Check(DevelopmentEventText.Display("",locale)=="","display empty");
                }
                foreach(var pair in new[]{
                    new[]{"唐門會議","唐门会议","唐門會議"},new[]{"後續節點","后续节点","后續節點"},new[]{"小師妹歸來","小师妹归来","小師妹歸來"},
                    new[]{"唐門會議 [script=STORY_A; key=M001; value>=10]","唐门会议 [script=STORY_A; key=M001; value>=10]","唐門會議 [script=STORY_A; key=M001; value>=10]"},
                    new[]{"😀e\u0301 𠮷かな 한국어\0後續節點","😀e\u0301 𠮷かな 한국어\0后续节点","😀e\u0301 𠮷かな 한국어\0后續節點"}
                }) {
                    string traditional=pair[0],simplified=pair[1];
                    Check(DevelopmentEventText.ToSimplified(traditional)==(windows?simplified:traditional),"simplify "+traditional);
                    // NLS leaves ambiguous 后 as written; do not invent a
                    // reversible dictionary translation such as 皇后 -> 皇後.
                    Check(DevelopmentEventText.ToTraditional(simplified)==(windows?pair[2]:simplified),"traditionalize "+simplified);
                    Check(DevelopmentEventText.Display(traditional,"chs")== (windows?simplified:traditional),"chs display");
                    Check(DevelopmentEventText.Display(simplified,"cht")== (windows?pair[2]:simplified),"cht display");
                    Check(DevelopmentEventText.Display(traditional,"cht")==traditional,"cht preserves original traditional notes");
                    Check(DevelopmentEventText.Display(traditional,"ja")==traditional,"ja keeps original notes");
                    Check(DevelopmentEventText.Display(traditional,"ko")==traditional,"ko keeps original notes");
                    Check(pair[0]==traditional && pair[1]==simplified,"source values unchanged");
                }
                const string code="STORY_ABC-001 / key=M30077 / series=MainRoute / next_key=END_2";
                Check(DevelopmentEventText.ToSimplified(code)==code && DevelopmentEventText.ToTraditional(code)==code,"ASCII identifiers untouched");
                Check(DevelopmentEventText.ToTraditional("皇后")=="皇后","ambiguous queen character preserved");
                Check(DevelopmentEventText.ToSimplified("皇后與後續節點")== (windows?"皇后与后续节点":"皇后與後續節點"),"safe simplification retains queen meaning");
                Check(DevelopmentEventText.Display("唐門會議","CHS")== (windows?"唐门会议":"唐門會議"),"ordinal locale");
                const string original="唐門會議，後續節點待條件判定";
                var fields=new Dictionary<string,string>{{"title",original},{"script","STORY_A"},{"key","M001"},{"series","唐門支線"},{"next_key","M002"}};
                string display=DevelopmentEventText.Display(fields["title"],"chs");
                Check(display==(windows?"唐门会议，后续节点待条件判定":original),"display title only");
                Check(fields["title"]==original && fields["script"]=="STORY_A" && fields["key"]=="M001"
                    && fields["series"]=="唐門支線" && fields["next_key"]=="M002","raw fields unchanged");
                Check(DevelopmentEventText.Display(original,"cht")==original,"complete traditional source retained");
                foreach(string locale in new[]{"ja","ko","en",null,""})Check(ReferenceEquals(DevelopmentEventText.Display(original,locale),original),"untranslated locale preserves source reference");
                if(windows) {
                    string first=DevelopmentEventText.ToSimplified(original);
                    string equalInput=new string(original.ToCharArray());
                    Check(!ReferenceEquals(equalInput,original),"distinct equal input");
                    Check(ReferenceEquals(first,DevelopmentEventText.ToSimplified(equalInput)),"simplified cached by value");
                    string reverse=DevelopmentEventText.ToTraditional("唐门会议，后续节点待条件判定");
                    Check(ReferenceEquals(reverse,DevelopmentEventText.ToTraditional("唐门会议，后续节点待条件判定")),"traditional cached");
                    DevelopmentEventText.Clear();
                    string again=DevelopmentEventText.ToSimplified(original);
                    Check(again==first && !ReferenceEquals(again,first),"clear releases simplified cache");
                    string reverseAgain=DevelopmentEventText.ToTraditional("唐门会议，后续节点待条件判定");
                    Check(reverseAgain==reverse && !ReferenceEquals(reverseAgain,reverse),"clear releases traditional cache");
                }
            }
            finally {DevelopmentEventText.Clear();}
            Console.WriteLine("DevelopmentEventTextTests PASS: "+_assertions+" assertions; Windows NLS executed="+windows);
        }
    }
}
