using System;
using System.Collections.Generic;

namespace MortalModHost
{
    internal static class NativeArchivePathTests
    {
        internal static void Run(bool required)
        {
            if (required && !NativeArchivePath.IsAvailable) throw new Exception("ZIP path native backend missing");
            int cases = 0;
            foreach (string path in new[] { null,"","\0","a\0\\","/a\\b","C:/x","c:x","/","//x","a//b","a/./b","a/../x","a/a:b","a./x","a /x","a/con.txt","a/COM1.LUA","LPT9/","COM0","COM¹","con..x","COnx","目录/角色.json","\ud800/\udc00","x.txt/"," x/ok","x/.hidden","a/b/c/d.json" })
                { Compare(path); cases++; }
            foreach (string reserved in new[] { "CON","PRN","AUX","NUL","COM1","LPT9" })
            for (int index = 0; index < reserved.Length; index++)
            {
                char expected = reserved[index];
                for (int unit = 0; unit <= char.MaxValue; unit++)
                {
                    char value = (char)unit;
                    if (char.ToUpperInvariant(value) != expected) continue;
                    string variant = reserved.Substring(0,index) + value + reserved.Substring(index+1);
                    Compare("user/"+variant+".png"); cases++;
                }
            }
            var random = new Random(781);
            char[] alphabet = "abcCOMnul0129./:\\ \0_\ud800\udc00目录".ToCharArray();
            for (int n = 0; n < 2500; n++)
            {
                var path = new char[random.Next(1,65)];
                for(int i=0;i<path.Length;i++) path[i]=alphabet[random.Next(alphabet.Length)];
                Compare(new string(path)); cases++;
            }
            Console.WriteLine("PASS ZIP path differential: "+cases+" cases; native="+NativeArchivePath.IsAvailable);
        }
        private static void Compare(string path)
        {
            string expected = Result(()=>NativeArchivePath.CheckManaged(path));
            string actual = Result(()=>NativeArchivePath.Check(path));
            if (expected != actual) throw new Exception("ZIP path differs: "+path+"\n"+expected+"\n"+actual);
        }
        private static string Result(Func<string> call)
        { try { return "OK:"+call(); } catch (Exception error) { return error.GetType().Name+":"+error.Message; } }
    }
}
