using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Text;

namespace MortalModHost
{
    internal static unsafe class NativeJsonTests
    {
        private static int _cases, _accepted, _rejected;

        internal static void Run(bool requireNative)
        {
            _cases = _accepted = _rejected = 0;
            bool native = NativeJson.IsAvailable;
            if (requireNative && !native)
                throw new Exception("JSON differential requires actual native: " + NativeHostKernel.UnavailableReason);
            foreach (string text in new string[] {
                null, "", " \t\r\n", "null", "true", "false", "truefalse", "false0", "null x",
                "t", "fals", "nul", "NULL", "True", "NaN", "Infinity", "-Infinity",
                "{}", "[]", "{\"a\":1,\"a\":2}", "{\"a\":{\"x\":1},\"a\":[3]}",
                "{\"\":null,\"x\":false,\"y\":true}", "{\"a\":1,\"\\u0061\":2}",
                "[null,true,false,{},[],\"\",0]", "[", "{", "[1", "{\"a\":1", "[1,]", "{\"a\":1,}",
                "{a:1}", "{\"a\" 1}", "{\"a\":}", "[1 2]", "{\"a\":1 \"b\":2}",
                "\"a\n\r\t\0b\"", "\"\ud800\udc00\ud800\udc00\udfff\"", "\"", "\"abc", "\"abc\\",
                "\"\\q\"", "\"\\\ud800\"", "\"\\u\"", "\"\\u1\"", "\"\\u12\"", "\"\\u123\"",
                "\"\\uGGGG\"", "\"\\uD800\\u0000\\uDC00\"", "\"\\\"\\\\\\/\\b\\f\\n\\r\\t\"",
                "[1e,\"\\uGGGG\",false x]", "[\"\\uGGGG\",1e,false x]", "[\"\\uGGGG\\q\",1]",
                "{\"\\uGGGG\" x}", "[1e,truX]", "[1,truX,1e]", "[1e,[" })
                Case(text);
            foreach (string number in new[] { "+1", ".5", "-.5", "01", "-01", "00", "1.", "1.e1", "1e+2",
                "1E-2", "-0", "-0.0", "1e-99999", "-1e-99999", "1e309", "-1e309", "1e", "1e+",
                "+", "-", ".", "--1", "++1", "1-2", "1.2.3", "1,000", "١", "９", "0x10",
                "2.2250738585072011e-308", "4.9406564584124654e-324", "1.7976931348623157e308", new string('9', 400) })
            {
                Case(number);
                Case("[" + number + ",true]");
            }
            foreach (char whitespace in new[] { '\t', '\n', '\v', '\f', '\r', ' ', '\u0085', '\u00a0', '\u1680',
                '\u2000', '\u2001', '\u2002', '\u2003', '\u2004', '\u2005', '\u2006', '\u2007', '\u2008',
                '\u2009', '\u200a', '\u2028', '\u2029', '\u202f', '\u205f', '\u3000', '\u180e', '\ufeff', '\u200b', '\0' })
            {
                Case(whitespace + "{\"a\"" + whitespace + ":" + whitespace + "true}" + whitespace);
                Case("\"\\u" + whitespace + "041\"");
                Case("\"\\u041" + whitespace + "\"");
            }
            foreach (string hex in new[] { "  A ", "0A\0\0", "A\0\0\0", "\0 0A", "A\0 \0", "00\rA", "A\r\n ",
                "\t\n A", "\uff10\uff10\uff14\uff11", "12\"x", "+041", "-001" }) Case("\"\\u" + hex + "\"");
            for (int depth = 127; depth <= 129; depth++)
            {
                Case(new string('[', depth) + "true" + new string(']', depth));
                var nested = new StringBuilder();
                for (int i = 0; i < depth; i++) nested.Append("{\"a\":");
                nested.Append('0').Append('}', depth);
                Case(nested.ToString());
            }
            var raw = new StringBuilder("\"");
            var escaped = new StringBuilder("\"");
            for (int c = 0; c <= 65535; c++)
            {
                if (c == '"' || c == '\\') raw.Append('\\');
                raw.Append((char)c);
                escaped.Append("\\u").Append(c.ToString("X4"));
            }
            Case(raw.Append('"').ToString());
            Case(escaped.Append('"').ToString());
            var random = new Random(14831);
            var doubles = new StringBuilder("[");
            var bits = new byte[8];
            for (int i = 0; i < 2048; i++)
            {
                random.NextBytes(bits);
                double value = BitConverter.ToDouble(bits, 0);
                if (double.IsInfinity(value) || double.IsNaN(value)) { i--; continue; }
                if (i != 0) doubles.Append(',');
                doubles.Append(value.ToString("R", CultureInfo.InvariantCulture));
            }
            Case(doubles.Append(']').ToString());
            // Exact integer lanes must keep boxed-double types and bit patterns
            // on Mono and current .NET, including the deliberately delegated -0.
            var integers = new StringBuilder("[");
            for (int i = short.MinValue; i <= short.MaxValue; i++)
            {
                if (i != short.MinValue) integers.Append(',');
                integers.Append(i.ToString(CultureInfo.InvariantCulture));
            }
            for (int i = 0; i < 16384; i++)
            {
                int value = random.Next() ^ (random.Next(2) == 0 ? 0 : int.MinValue);
                integers.Append(',').Append(value.ToString(CultureInfo.InvariantCulture));
            }
            integers.Append(",-2147483648,2147483647,2147483648,-2147483649,-0,-00,0000000010,+01]");
            Case(integers.ToString());
            Case("[\"\\u0041\\u0061\\uD800\\udc00\",\"\\u 041\",-0,1e]");
            Case("[0,\"\\u0000\",\"\\uGGGG\",1e]");
            const string alphabet = "{}[]\"\\:, +-.eE0129truefalsn\t\nX";
            for (int i = 0; i < 1200; i++)
            {
                var input = new char[random.Next(1, 50)];
                for (int c = 0; c < input.Length; c++) input[c] = alphabet[random.Next(alphabet.Length)];
                Case(new string(input));
            }
            if (native) ValidatedLeaseAndLayout();
            Console.WriteLine("PASS JSON differential: " + _cases + " cases; accepted=" + _accepted
                + "; rejected=" + _rejected + "; actual native=" + native);
        }

        internal static void ReleasedJsonRoundTrip(string fixture, string outputDirectory)
        {
            byte[] original = File.ReadAllBytes(fixture);
            string chapter = null;
            using (var stream = new MemoryStream(original))
            using (var archive = new ZipArchive(stream, ZipArchiveMode.Read))
            {
                foreach (ZipArchiveEntry entry in archive.Entries)
                {
                    if (!entry.FullName.EndsWith(".json", StringComparison.Ordinal)) continue;
                    using (var reader = new StreamReader(entry.Open(), Encoding.UTF8))
                    {
                        string text = reader.ReadToEnd();
                        Case(text);
                        if (entry.FullName == "story/main.json") chapter = text;
                    }
                }
            }
            if (chapter == null) throw new Exception("released fixture has no story/main.json");
            var expected = (Dictionary<string, object>)MiniJson.ParseManaged(chapter);
            expected["title"] = "继续编辑历史章节";
            int end = chapter.LastIndexOf('}');
            string edited = chapter.Substring(0, end) + ",\"title\":\"继续编辑历史章节\"}";
            Directory.CreateDirectory(outputDirectory);
            string saved = Path.Combine(outputDirectory, "continued-" + Path.GetFileNameWithoutExtension(fixture) + ".json");
            File.WriteAllText(saved, edited, new UTF8Encoding(false));
            Equal(expected, MiniJson.Parse(File.ReadAllText(saved, Encoding.UTF8)), "historical open/edit/save/reopen");
            byte[] unchanged = File.ReadAllBytes(fixture);
            if (Convert.ToBase64String(original) != Convert.ToBase64String(unchanged))
                throw new Exception("historical fixture must not change");
            Console.WriteLine("PASS JSON historical open/edit/save/reopen: " + Path.GetFileName(fixture));
        }

        private static void Case(string json)
        {
            object expected = null, actual = null;
            Exception managedError = null, nativeError = null;
            try { expected = MiniJson.ParseManaged(json); } catch (Exception error) { managedError = error; }
            try { actual = MiniJson.Parse(json); } catch (Exception error) { nativeError = error; }
            if (managedError != null || nativeError != null)
            {
                if (managedError == null || nativeError == null || managedError.GetType() != nativeError.GetType()
                    || managedError.Message != nativeError.Message)
                    throw new Exception("JSON error mismatch for " + Describe(json) + "; managed=" + managedError + "; backend=" + nativeError);
                _rejected++;
            }
            else { Equal(expected, actual, Describe(json)); _accepted++; }
            _cases++;
        }

        private static string Describe(string value)
        {
            if (value == null) return "<null>";
            var text = new StringBuilder();
            for (int i = 0; i < Math.Min(value.Length, 120); i++)
            {
                char c = value[i];
                if (char.IsControl(c) || char.IsSurrogate(c)) text.Append("\\u").Append(((int)c).ToString("X4"));
                else text.Append(c);
            }
            return text.ToString();
        }

        private static void Equal(object expected, object actual, string context)
        {
            if (expected == null || actual == null)
            { if (expected != actual) throw new Exception(context + ": null mismatch"); return; }
            if (expected.GetType() != actual.GetType()) throw new Exception(context + ": value type mismatch");
            var dictionary = expected as Dictionary<string, object>;
            if (dictionary != null)
            {
                var other = (Dictionary<string, object>)actual;
                if (dictionary.Count != other.Count) throw new Exception(context + ": duplicate-key/count mismatch");
                foreach (var pair in dictionary)
                {
                    object value;
                    if (!other.TryGetValue(pair.Key, out value)) throw new Exception(context + ": key mismatch");
                    Equal(pair.Value, value, context);
                }
                return;
            }
            var list = expected as List<object>;
            if (list != null)
            {
                var other = (List<object>)actual;
                if (list.Count != other.Count) throw new Exception(context + ": array length mismatch");
                for (int i = 0; i < list.Count; i++) Equal(list[i], other[i], context);
                return;
            }
            if (expected is double)
            {
                if (BitConverter.DoubleToInt64Bits((double)expected) != BitConverter.DoubleToInt64Bits((double)actual))
                    throw new Exception(context + ": double bits mismatch");
            }
            else if (!expected.Equals(actual)) throw new Exception(context + ": UTF-16/scalar mismatch");
        }

        private static void ValidatedLeaseAndLayout()
        {
            if (Marshal.SizeOf(typeof(NativeJson.Node)) != 32 || Marshal.SizeOf(typeof(NativeJson.DocumentView)) != (IntPtr.Size == 8 ? 40 : 24))
                throw new Exception("JSON tape/view ABI layout changed");
            int count;
            if (!NativeJson.TryUseDocument("[+1,\"\\u 041\"]", document =>
            {
                if (document.NodeCount != 3 || document.Nodes[1].Number != 1.0 || document.Text[0] != 'A')
                    throw new Exception("callback must receive a fully validated tape");
                return document.NodeCount;
            }, out count) || count != 3) throw new Exception("native lease unexpectedly unavailable");
            for (int i = 0; i < 20; i++)
            {
                bool threw = false;
                try
                {
                    NativeJson.TryUseDocument<int>("{}", document => { throw new InvalidOperationException("callback-error"); }, out count);
                }
                catch (InvalidOperationException error) { threw = error.Message == "callback-error"; }
                if (!threw) throw new Exception("native lease swallowed callback failure");
                Case("{\"after-error\":true}");
            }
        }
    }
}
