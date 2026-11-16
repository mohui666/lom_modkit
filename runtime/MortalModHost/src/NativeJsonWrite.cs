using System;
using System.Runtime.InteropServices;
using System.Text;

namespace MortalModHost
{
    internal static unsafe class NativeJsonWrite
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int LengthCallback(char* source,int length);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int WriteCallback(char* source,int length,char* output,int capacity);
        private static readonly LengthCallback Length = NativeHostKernel.Bind<LengthCallback>("lom_json_quote_length_v1");
        private static readonly WriteCallback Write = NativeHostKernel.Bind<WriteCallback>("lom_json_quote_write_v1");
        internal static bool IsAvailable { get { return Length != null && Write != null; } }
        // Keep small strings in the caller: crossing the ABI twice costs more
        // than their short loop. No native rejection takes this path.
        internal static bool TryAppend(StringBuilder target,string value,ref char[] scratch)
        {
            if (value.Length < 128 || !IsAvailable) return false;
            Append(target,value,ref scratch);
            return true;
        }
        internal static void Append(StringBuilder target,string value,ref char[] scratch)
        {
            if (value == null || target == null) throw new ArgumentNullException();
            if (!IsAvailable) throw new InvalidOperationException("原生 JSON 转义未加载");
            fixed(char* source = value)
            {
                int length = Length(source,value.Length);
                if (length < 2) throw new InvalidOperationException("原生 JSON 转义长度无效");
                if (length == value.Length + 2)
                {
                    target.Append('"');
                    // Bounded copies preserve StringBuilder's chunk growth;
                    // appending one full string can leave no room for the final
                    // quote and allocate an otherwise empty large chunk.
                    if (value.Length < 1024) foreach(char unit in value) target.Append(unit);
                    else for(int at=0;at<value.Length;at+=1024) target.Append(value,at,Math.Min(1024,value.Length-at));
                    target.Append('"');
                    return;
                }
                if (scratch == null || scratch.Length < length) scratch = new char[length];
                int written;
                fixed(char* output = scratch) written = Write(source,value.Length,output,scratch.Length);
                if (written != length) throw new InvalidOperationException("原生 JSON 转义拒绝请求或长度不一致");
                target.Append(scratch,0,written);
            }
        }
    }
}
