using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativeStoryPatch
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct Edit
        {
            internal IntPtr Find; internal int FindLength, Occurrence;
            internal IntPtr Replacement; internal int ReplacementLength, Validation;
        }
        [StructLayout(LayoutKind.Sequential)]
        private struct Span { internal int Start, End, Edit; }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint PlanCallback(char* source, int length, Edit* edits, int count, Span* plan, long* outputLength);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint WriteCallback(char* source, int length, Edit* edits, int count, Span* plan, char* output, int outputLength);
        private static readonly PlanCallback Plan = NativeHostKernel.Bind<PlanCallback>("lom_story_patch_plan_v1");
        private static readonly WriteCallback Write = NativeHostKernel.Bind<WriteCallback>("lom_story_patch_write_v1");
        internal static bool IsAvailable { get { return Plan != null; } }

        internal static string Apply(string source, List<object> edits)
        {
            var entries = new Edit[edits.Count];
            var spans = new Span[edits.Count];
            var pinned = new List<GCHandle>(edits.Count * 2);
            try
            {
                for (int i = 0; i < edits.Count; ++i)
                {
                    var edit = edits[i] as Dictionary<string,object>;
                    object f, r, n;
                    if (edit == null || edit.Count != 3 || !edit.TryGetValue("find", out f)
                        || !edit.TryGetValue("replace", out r) || !edit.TryGetValue("occurrence", out n)
                        || !(f is string) || !(r is string) || !(n is double || n is long || n is int))
                    { entries[i].Validation = 1; continue; }
                    string find = (string)f, replacement = (string)r;
                    double count = Convert.ToDouble(n, CultureInfo.InvariantCulture);
                    if (find.Length == 0 || find.Length > 65536 || replacement.Length > 262144
                        || count < 1 || count > 1000000 || count != Math.Floor(count))
                    { entries[i].Validation = 2; continue; }
                    GCHandle a = GCHandle.Alloc(find, GCHandleType.Pinned); pinned.Add(a);
                    GCHandle b = GCHandle.Alloc(replacement, GCHandleType.Pinned); pinned.Add(b);
                    entries[i] = new Edit { Find = a.AddrOfPinnedObject(), FindLength = find.Length,
                        Occurrence = (int)count, Replacement = b.AddrOfPinnedObject(), ReplacementLength = replacement.Length };
                }
                long length = 0;
                fixed (char* original = source)
                fixed (Edit* input = entries)
                fixed (Span* plan = spans)
                {
                    uint status = Plan(original, source.Length, input, entries.Length, plan, &length);
                    if (status == 2) throw new FormatException("原版剧情补丁修改格式无效");
                    if (status == 3) throw new FormatException("原版剧情补丁的定位或替换文本无效");
                    if (status == 4) throw new InvalidOperationException("补丁定位原文不存在，游戏章节可能已更新；请重新读取原版章节后核对修改");
                    if (status == 5) throw new FormatException("原版剧情补丁修改范围重叠");
                    if (status == 6) throw new OutOfMemoryException();
                    if (status != 1 || length < 0 || length > int.MaxValue)
                        throw new InvalidOperationException("原生剧情补丁规划拒绝了请求：" + status);
                    string result = new string('\0', (int)length);
                    fixed (char* output = result)
                        if (Write(original, source.Length, input, entries.Length, plan, output, (int)length) != 1)
                            throw new InvalidOperationException("原生剧情补丁复制拒绝了请求");
                    return result;
                }
            }
            finally { foreach (GCHandle pin in pinned) pin.Free(); }
        }
    }
}
