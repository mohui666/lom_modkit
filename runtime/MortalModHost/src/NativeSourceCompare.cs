using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativeSourceCompare
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct Mark { internal int Start, Length; internal uint Flags; }
        [StructLayout(LayoutKind.Sequential)]
        private struct Header
        {
            internal uint Status, LogicChanged, OriginalVerified;
            internal int TargetStart, TargetLength, SourceLength, MarkCount;
            internal char* Source;
            internal Mark* Marks;
        }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr CompareCallback(NativeJson.DocumentView* view);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void FreeCallback(IntPtr result);
        private static readonly CompareCallback Compare = NativeHostKernel.Bind<CompareCallback>("lom_source_compare_v1");
        private static readonly FreeCallback Free = NativeHostKernel.Bind<FreeCallback>("lom_source_compare_free_v1");
        internal static bool IsAvailable { get { return Compare != null; } }

        private static NullReferenceException MissingObject()
        {
            // Mono's implicit null-reference message differs from its default
            // exception constructor. Keep the original dereference behavior.
            try { Dictionary<string, object> missing = null; missing.Clear(); }
            catch (NullReferenceException error) { return error; }
            throw new InvalidOperationException();
        }

        private static KeyNotFoundException MissingKey(uint status)
        {
            string key = new[] { "schema", "target", "origins", "nodes", "code", "id", "type", "node" }[status - 30];
            try { object value = new Dictionary<string, object>()[key]; }
            catch (KeyNotFoundException error) { return error; }
            throw new InvalidOperationException();
        }

        private static InvalidCastException InvalidValue(uint status)
        {
            int kind = (int)(status >= 90 ? status - 90 : status - 80);
            object value;
            switch (kind)
            {
                case 1: case 2: value = false; break;
                case 3: value = 0d; break;
                case 4: value = ""; break;
                case 5: value = new List<object>(); break;
                case 6: value = new Dictionary<string, object>(); break;
                default: throw new InvalidOperationException("原生来源分类返回无效转换类型");
            }
            try
            {
                if (status >= 90) GC.KeepAlive((string)value);
                else GC.KeepAlive((Dictionary<string, object>)value);
            }
            catch (InvalidCastException error) { return error; }
            throw new InvalidOperationException();
        }

        internal static bool TryParse(string json, out VanillaImportSource result)
        {
            result = null;
            if (Compare == null) return false;
            return NativeJson.TryUseDocument(json, ReadResult, out result);
        }

        private static string Text(NativeJson.DocumentView view, int at, int length)
        {
            long offset = at >= 0 ? at : -1L - at;
            int available = at >= 0 ? view.SourceLength : view.TextLength;
            if (length < 0 || offset < 0 || offset > available || length > available - offset)
                throw new InvalidOperationException("原生来源分类返回无效文本范围");
            return length == 0 ? "" : new string((at >= 0 ? view.Source : view.Text) + offset, 0, length);
        }

        private static VanillaImportSource ReadResult(NativeJson.DocumentView view)
        {
            IntPtr owned = Compare(&view);
            if (owned == IntPtr.Zero) throw new InvalidOperationException("原生来源分类拒绝了文档");
            try
            {
                Header header = *(Header*)owned;
                switch (header.Status)
                {
                    case 1: return null;
                    case 2: break;
                    case 10: throw new FormatException("原版整章来源格式无效");
                    case 11: throw new FormatException("原版整章来源缺少章节编号");
                    case 12: throw new FormatException("原版整章来源缺少步骤");
                    case 13: throw new FormatException("原版来源文本超过 4 MiB");
                    case 22: throw MissingObject();
                    case 23: throw new ArgumentNullException("key");
                    case 24: throw new ArgumentNullException("dictionary");
                    case 30: case 31: case 32: case 33: case 34: case 35: case 36: case 37:
                        throw MissingKey(header.Status);
                    case 81: case 82: case 83: case 84: case 85:
                    case 91: case 92: case 93: case 95: case 96:
                        throw InvalidValue(header.Status);
                    default: throw new InvalidOperationException("原生来源分类拒绝了请求：" + header.Status);
                }
                if (header.SourceLength < 0 || header.SourceLength > 4 * 1024 * 1024 || header.MarkCount < 0
                    || header.SourceLength > 0 && header.Source == null || header.MarkCount > 0 && header.Marks == null)
                    throw new InvalidOperationException("原生来源分类返回无效结果范围");
                string target = Text(view, header.TargetStart, header.TargetLength);
                string original = header.SourceLength == 0 ? "" : new string(header.Source, 0, header.SourceLength);
                var ids = new string[header.MarkCount];
                var flags = new uint[header.MarkCount];
                for (int i = 0; i < ids.Length; ++i)
                {
                    Mark mark = header.Marks[i];
                    if ((mark.Flags & ~3u) != 0) throw new InvalidOperationException("原生来源分类返回无效标记");
                    flags[i] = mark.Flags;
                    if (mark.Flags != 0) ids[i] = Text(view, mark.Start, mark.Length);
                }
                return VanillaImportSource.FromNative(target, original, header.LogicChanged != 0,
                    header.OriginalVerified != 0, ids, flags);
            }
            finally { Free(owned); }
        }
    }
}
