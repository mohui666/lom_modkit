using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Threading;

namespace MortalModHost
{
    // One batch scan over pinned UTF-16. Exact i32 values and ordinary four-hex
    // escapes are completed in Rust; C# keeps rounding-sensitive numbers,
    // negative zero and unusual HexNumber spellings, in their original order.
    // A rejected native parse is never retried through ParseManaged.
    internal static unsafe class NativeJson
    {
        [StructLayout(LayoutKind.Sequential, Pack = 8)]
        internal struct Node
        {
            internal int Kind, Start, Length, Next, KeyStart, KeyLength;
            internal double Number;
        }

        // All pointers are valid only during TryUseDocument's callback. Strings
        // with a nonnegative location borrow Source; negative locations refer
        // to Text[-1-location]. Duplicate keys remain ordered on the tape.
        [StructLayout(LayoutKind.Sequential, Pack = 8)]
        internal struct DocumentView
        {
            internal char* Source;
            internal Node* Nodes;
            internal char* Text;
            internal int SourceLength, NodeCount, TextLength;
        }

        [StructLayout(LayoutKind.Sequential, Pack = 8)]
        private struct Validation
        {
            internal int Kind, Start, Length, Target;
        }
        [StructLayout(LayoutKind.Sequential, Pack = 8)]
        private struct Result
        {
            internal Node* Nodes;
            internal char* Text;
            internal Validation* Validations;
            internal int NodeCount, TextLength, ValidationCount;
            internal int ErrorCode, ErrorPosition, ErrorDetail;
        }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr ParseCallback(char* source, int length);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void FreeCallback(IntPtr result);
        private sealed class Backend
        {
            internal readonly ParseCallback Parse;
            internal readonly FreeCallback Free;
            internal Backend(ParseCallback parse, FreeCallback free) { Parse = parse; Free = free; }
        }
        private static readonly Lazy<Backend> Current = new Lazy<Backend>(() =>
        {
            ParseCallback parse = NativeHostKernel.Bind<ParseCallback>("lom_json_parse_utf16_v1");
            return parse == null ? new Backend(null, null)
                : new Backend(parse, NativeHostKernel.Bind<FreeCallback>("lom_json_free_v1"));
        }, LazyThreadSafetyMode.ExecutionAndPublication);
        private static readonly Func<DocumentView, object> Construct = BuildRoot;

        internal static bool IsAvailable { get { return Current.Value.Parse != null; } }

        internal static bool TryParse(string json, out object value)
        {
            return TryUseDocument(json, Construct, out value);
        }

        // The single callback can reuse the validated tape in another native
        // batch calculation without first allocating a managed DOM. It must
        // never retain these pointers; all success/error paths release the arena.
        internal static bool TryUseDocument<T>(string json, Func<DocumentView, T> callback, out T value)
        {
            if (json == null) throw new FormatException("JSON 文本为 null");
            if (callback == null) throw new ArgumentNullException("callback");
            Backend backend = Current.Value;
            if (backend.Parse == null) { value = default(T); return false; }
            fixed (char* source = json)
            {
                IntPtr allocation = backend.Parse(source, json.Length);
                if (allocation == IntPtr.Zero) throw new InvalidOperationException("原生 JSON 解析拒绝了调用参数");
                try
                {
                    Result* result = (Result*)allocation;
                    Validate(json, result);
                    ThrowSyntaxError(result);
                    value = callback(new DocumentView { Source = source, SourceLength = json.Length,
                        Nodes = result->Nodes, NodeCount = result->NodeCount,
                        Text = result->Text, TextLength = result->TextLength });
                    return true;
                }
                finally { backend.Free(allocation); }
            }
        }

        private static void Validate(string json, Result* result)
        {
            for (int i = 0; i < result->ValidationCount; i++)
            {
                Validation item = result->Validations[i];
                string text = json.Substring(item.Start, item.Length);
                if (item.Kind == 0)
                {
                    double number;
                    if (!double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out number)
                        || double.IsNaN(number) || double.IsInfinity(number))
                        throw new FormatException("非法数字 '" + text + "'，位置 " + item.Start);
                    result->Nodes[item.Target].Number = number;
                }
                else if (item.Kind == 1)
                {
                    // Keep int.Parse's HexNumber whitespace, terminal-NUL and
                    // exception behavior, including raw/unpaired UTF-16 input.
                    result->Text[item.Target] = (char)int.Parse(text, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                }
                else throw new InvalidOperationException("原生 JSON 校验事件类型无效：" + item.Kind);
            }
        }

        private static void ThrowSyntaxError(Result* result)
        {
            string error;
            switch (result->ErrorCode)
            {
                case 0: return;
                case 1: error = "JSON 意外结束"; break;
                case 2: error = "对象键必须是字符串，位置 " + result->ErrorPosition; break;
                case 3: error = "期望 ':'，位置 " + result->ErrorPosition; break;
                case 4: error = "对象未闭合"; break;
                case 5: error = "对象元素间缺少 ','，位置 " + result->ErrorPosition; break;
                case 6: error = "数组未闭合"; break;
                case 7: error = "数组元素间缺少 ','，位置 " + result->ErrorPosition; break;
                case 8: error = "\\u 转义不完整，位置 " + result->ErrorPosition; break;
                case 9: error = "非法转义字符 \\" + (char)result->ErrorDetail; break;
                case 10: error = "字符串未闭合"; break;
                case 11: error = "JSON 嵌套层数超过 128 上限"; break;
                case 12: error = "非法字面值，位置 " + result->ErrorPosition; break;
                case 13: error = "JSON 末尾存在多余内容，位置 " + result->ErrorPosition; break;
                default: throw new InvalidOperationException("原生 JSON 错误类型无效：" + result->ErrorCode);
            }
            throw new FormatException(error);
        }

        private static object BuildRoot(DocumentView document)
        {
            int index = 0;
            return BuildValue(document, ref index);
        }

        private static string ReadString(DocumentView document, int start, int length)
        {
            if (length == 0) return string.Empty;
            return start >= 0 ? new string(document.Source, start, length)
                : new string(document.Text, -1 - start, length);
        }

        private static object BuildValue(DocumentView document, ref int index)
        {
            Node node = document.Nodes[index++];
            switch (node.Kind)
            {
                case 0: return null;
                case 1: return false;
                case 2: return true;
                case 3: return node.Number;
                case 4: return ReadString(document, node.Start, node.Length);
                case 5:
                    var list = new List<object>(node.Length);
                    while (index < node.Next) list.Add(BuildValue(document, ref index));
                    return list;
                case 6:
                    // Duplicate keys are legal and overwrite earlier values;
                    // their token count must not reserve an enormous dictionary.
                    var dictionary = new Dictionary<string, object>(Math.Min(node.Length, 256));
                    while (index < node.Next)
                    {
                        Node child = document.Nodes[index];
                        string key = ReadString(document, child.KeyStart, child.KeyLength);
                        dictionary[key] = BuildValue(document, ref index);
                    }
                    return dictionary;
                default: throw new InvalidOperationException("原生 JSON 节点类型无效：" + node.Kind);
            }
        }
    }
}
