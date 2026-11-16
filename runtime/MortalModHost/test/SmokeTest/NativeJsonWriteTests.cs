using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MortalModHost
{
    internal static class NativeJsonWriteTests
    {
        internal static string ManagedQuote(string text)
        {
            var output = new StringBuilder(); output.Append('"');
            foreach(char c in text)
                if(c=='"'||c=='\\') output.Append('\\').Append(c);
                else if(c<32) output.Append("\\u").Append(((int)c).ToString("x4",CultureInfo.InvariantCulture));
                else output.Append(c);
            return output.Append('"').ToString();
        }
        internal static string NativeQuote(string text)
        { var output=new StringBuilder();char[] scratch=null;NativeJsonWrite.Append(output,text,ref scratch);return output.ToString(); }
        internal static void Run(bool required)
        {
            if(required&&!NativeJsonWrite.IsAvailable) throw new Exception("JSON quote native backend missing");
            var texts = new List<string> { "","a","\0\u001f\n\r\t\"\\/","中文😀\ud800\udc00",new string('a',127),new string('b',128),new string('汉',9000) };
            var all=new char[65536]; for(int i=0;i<all.Length;i++)all[i]=(char)i;texts.Add(new string(all));
            var random=new Random(651);
            for(int n=0;n<100;n++){var chars=new char[random.Next(1,1000)];for(int i=0;i<chars.Length;i++)chars[i]=(char)random.Next(65536);texts.Add(new string(chars));}
            foreach(string text in texts)
            {
                string expected=ManagedQuote(text);
                if(NativeJsonWrite.IsAvailable && expected!=NativeQuote(text)) throw new Exception("JSON string escaping changed");
                if(expected!=ScenarioState.ToJson(text)||ScenarioState.ToJsonManaged(text)!=expected)throw new Exception("JSON serializer string changed");
            }
            var nested=new Dictionary<string,object> { {"z",new object[]{null,true,false,1,-2,1.23456789,new DateTime(1970,1,1,0,0,0,DateTimeKind.Utc)}}, {"a",texts.ToArray()}, {"控制\n",new Dictionary<string,int>{{"甲",1},{"乙",2}}} };
            if(ScenarioState.ToJson(nested)!=ScenarioState.ToJsonManaged(nested))throw new Exception("JSON serializer ordering or scalar formatting changed");
            Console.WriteLine("PASS JSON quote differential: "+texts.Count+" UTF16 cases and nested records; native="+NativeJsonWrite.IsAvailable);
        }
    }
}
