using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Text;

namespace MortalModHost
{
    internal static class NativeStoryPatchTests
    {
        private static int _cases;
        private static Dictionary<string, object> Edit(string find, string replace, object occurrence)
        { return new Dictionary<string, object> { { "find", find }, { "replace", replace }, { "occurrence", occurrence } }; }
        private static void Case(string name, string source, params object[] edits)
        {
            var values = new List<object>(edits);
            string expected = Capture(() => VanillaStoryPatch.ApplyManaged(source, values));
            string actual = Capture(() => NativeStoryPatch.Apply(source, values));
            if (!string.Equals(expected, actual, StringComparison.Ordinal))
                throw new Exception("Story patch differential " + name + ": expected=" + expected + "; actual=" + actual);
            _cases++;
        }
        private static string Capture(Func<string> call)
        {
            try { return "value:" + call(); }
            catch (Exception e) { return e.GetType().FullName + ":" + e.Message; }
        }
        internal static void Run(bool requireNative)
        {
            bool native = NativeStoryPatch.IsAvailable;
            if (!native)
            {
                if (requireNative) throw new Exception("Story patch native backend required: " + NativeHostKernel.UnavailableReason);
                Console.WriteLine("SKIP native story patch differential: backend unavailable"); return;
            }
            _cases = 0;
            Case("empty", ""); Case("unchanged", "正文\0\uD800");
            Case("delete", "abc", Edit("abc", "", 1d));
            Case("nth is nonoverlapping", "aaaaa", Edit("aa", "x", 2d));
            Case("third nonoverlapping absent", "aaaaa", Edit("aa", "x", 3d));
            Case("sort against original", "aabbaa", Edit("aa", "b", 2d), Edit("bb", "aa", 1d), Edit("aa", "z", 1d));
            Case("duplicate anchor overlap", "aa", Edit("a", "x", 1d), Edit("a", "y", 1d));
            Case("nested anchor overlap", "abc", Edit("ab", "x", 1d), Edit("bc", "y", 1d));
            Case("missing before malformed", "abc", Edit("z", "x", 1d), null);
            Case("malformed before missing", "abc", null, Edit("z", "x", 1d));
            Case("overlap after missing", "abc", Edit("a", "x", 1d), Edit("a", "y", 1d), Edit("z", "", 1d));
            Case("missing before invalid range", "abc", Edit("z", "x", 1d), Edit("", "", 1d));
            Case("missing before later same find", "abc", Edit("a", "x", 2d), Edit("a", "x", 1d));
            Case("UTF16 code units", "\uD800a\uDC00\0😀\uD800", Edit("\uD800", "\uDC00", 2d), Edit("😀", "\0", 1d));
            Case("prefix fallback", "ababababacababac", Edit("ababac", "z", 2d));
            foreach (object n in new object[] { 0d, -1d, 1.5d, 1000001d, double.NaN, double.PositiveInfinity, true, "1", null })
                Case("occurrence " + Convert.ToString(n, CultureInfo.InvariantCulture), "a", Edit("a", "b", n));
            Case("int occurrence", "aa", Edit("a", "b", 2));
            Case("long occurrence", "aa", Edit("a", "b", 2L));
            Case("find maximum", new string('a', 65536), Edit(new string('a', 65536), "x", 1d));
            Case("find too long", "a", Edit(new string('a', 65537), "x", 1d));
            Case("replacement maximum", "a", Edit("a", new string('b', 262144), 1d));
            Case("replacement too long", "a", Edit("a", new string('b', 262145), 1d));
            Case("case sensitive", "AbCabc", Edit("abc", "X", 1d));
            var many = new object[4096];
            for (int i = 0; i < many.Length; ++i) many[i] = Edit("x", "y", (double)(many.Length - i));
            Case("4096 requested positions", new string('x', many.Length), many);
            var random = new Random(913);
            for (int n = 0; n < 150; ++n)
            {
                var source = new StringBuilder();
                for (int j = 0; j < 160; ++j) source.Append((char)('a' + random.Next(4)));
                var edits = new object[random.Next(1, 12)];
                for (int j = 0; j < edits.Length; ++j)
                {
                    int at = random.Next(source.Length - 4), length = random.Next(1, 5);
                    edits[j] = Edit(source.ToString(at, length), j % 2 == 0 ? "替换" : "", (double)random.Next(1, 6));
                }
                Case("random " + n, source.ToString(), edits);
            }
            if (VanillaStoryPatch.Apply("aa", "[{\"find\":\"a\",\"replace\":\"b\",\"occurrence\":2}]") != "ab")
                throw new Exception("Story patch integrated JSON path");
            Console.WriteLine("PASS native story patch: " + _cases + " exact output/error differential cases");
        }

        // Milliseconds and checksum. Parsing is excluded so this measures the
        // migrated matching/copy batch with the same already validated objects.
        internal static double[] Benchmark(bool native, int iterations)
        {
            if (native && !NativeStoryPatch.IsAvailable) throw new Exception("Native story patch required");
            var source = new StringBuilder();
            for (int i = 0; i < 8192; ++i) source.Append("line=正文1234567890;\n");
            var edits = new List<object>();
            for (int i = 0; i < 512; ++i) edits.Add(Edit("line=", "行=", (double)(i * 16 + 1)));
            string input = source.ToString();
            Func<string> run = () => native ? NativeStoryPatch.Apply(input, edits) : VanillaStoryPatch.ApplyManaged(input, edits);
            run(); long checksum = 0;
            var timer = Stopwatch.StartNew();
            for (int i = 0; i < iterations; ++i) { string value = run(); checksum += value.Length + value[0]; }
            timer.Stop();
            return new[] { timer.Elapsed.TotalMilliseconds, (double)checksum };
        }
    }
}
