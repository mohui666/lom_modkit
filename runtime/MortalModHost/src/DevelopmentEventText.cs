using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    // Display-only character conversion. Callers retain the captured text and
    // task/script identifiers; neither this cache nor NLS rewrites those values.
    internal static class DevelopmentEventText
    {
        private const uint SimplifiedChinese=0x02000000,TraditionalChinese=0x04000000;
        private static readonly object Sync=new object();
        private static readonly Dictionary<string,string> Simplified=new Dictionary<string,string>(StringComparer.Ordinal);
        private static readonly Dictionary<string,string> Traditional=new Dictionary<string,string>(StringComparer.Ordinal);

        internal static string ToSimplified(string value)
        {return Convert(value,true);}
        internal static string ToTraditional(string value)
        {return Convert(value,false);}
        internal static string Display(string value,string locale)
        {
            if(string.Equals(locale,"chs",StringComparison.OrdinalIgnoreCase))return ToSimplified(value);
            if(string.Equals(locale,"cht",StringComparison.OrdinalIgnoreCase))return ToTraditional(value);
            // Original developer notes have no Japanese/Korean translation.
            return value;
        }
        internal static void Clear()
        {
            lock(Sync){Simplified.Clear();Traditional.Clear();}
        }
        private static string Convert(string value,bool simplified)
        {
            if(string.IsNullOrEmpty(value) || Environment.OSVersion.Platform!=PlatformID.Win32NT)return value;
            var cache=simplified?Simplified:Traditional;
            lock(Sync) {
                string result;
                if(cache.TryGetValue(value,out result))return result;
                string locale=simplified?"zh-CN":"zh-TW";
                uint flags=simplified?SimplifiedChinese:TraditionalChinese;
                int needed=LCMapStringEx(locale,flags,value,value.Length,null,0,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero);
                if(needed==0)throw new Win32Exception(Marshal.GetLastWin32Error(),"Windows NLS 简繁转换失败");
                var output=new char[needed];
                int written=LCMapStringEx(locale,flags,value,value.Length,output,output.Length,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero);
                if(written==0)throw new Win32Exception(Marshal.GetLastWin32Error(),"Windows NLS 简繁转换失败");
                result=new string(output,0,written);
                // NLS is a character mapper, not a dictionary translator. It
                // leaves 後 unchanged even when simplifying; this direction is
                // unambiguous. Keep its traditional 后 ambiguity (e.g. 皇后).
                if(simplified)result=result.Replace('後','后');
                cache.Add(value,result);
                return result;
            }
        }

        // Microsoft Learn: LCMapStringEx (winnls.h). Positive source lengths
        // preserve embedded NULs; destination length zero queries UTF-16 units.
        // https://learn.microsoft.com/windows/win32/api/winnls/nf-winnls-lcmapstringex
        [DllImport("kernel32.dll",CharSet=CharSet.Unicode,ExactSpelling=true,SetLastError=true)]
        private static extern int LCMapStringEx(string locale,uint flags,string source,int sourceLength,
            [Out] char[] destination,int destinationLength,IntPtr version,IntPtr reserved,IntPtr sortHandle);
    }
}
