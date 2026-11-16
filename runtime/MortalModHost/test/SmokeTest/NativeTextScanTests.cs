using System;
using System.Diagnostics;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    internal static class NativeTextScanTests
    {
        private static readonly bool Mono = Type.GetType("Mono.Runtime") != null;
        private static readonly Regex Tags = new Regex(
            @"</?(?:b|i|size|color|material|quad|sprite|font|alpha|u|s|mark|link|align|voffset|space|cspace|mspace|nobr)(?:[=\s][^<>]*)?>",
            RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
        private static int _cases;
        internal static void Run(bool requireNative)
        {
            bool native = NativeTextScan.IsAvailable;
            if (requireNative && !native) throw new Exception("Text scan requires actual native: " + NativeHostKernel.UnavailableReason);
            _cases = 0;
            foreach (string text in new[] {
                null, "", "你好", "  \r\n\t\0", "e\u0301", "\u0301\u0301", " \u0301", "\0\u0301",
                "\uFFFF\u0301", "\u200D\u0301", "\uD800\u0301", "\uD800\uDC00\u0301",
                "😀", "👩‍👩‍👧‍👧", "🇨🇳", "👍🏻", "\uDB40\uDD00", "x\uDB40\uDD00",
                "\uD800<b>\uDC00", "\uD800</b>\uDC00", "\uD800<bad>\uDC00", "\uD800<b>x\uDC00",
                "e<b>\u0301</b>", " <b>\u0301</b>", "<b<color>></color>", "<b><i></b></i>",
                "<unknown>x</unknown>", "<b=one\ntwo>x", "<b\n=>X</b>", "<b/>X", "<b />X",
                "<B>x</B>", "<ſpace=1>X</ſpace>", "<linK=0>X</linK>", "<alİgn=0>X</alİgn>",
                new string('<', 4096), "<color=" + new string('x', 16384),
            }) Case(text, native);
            foreach (string tag in new[] { "b", "i", "size", "color", "material", "quad", "sprite", "font", "alpha", "u", "s", "mark", "link", "align", "voffset", "space", "cspace", "mspace", "nobr" })
            {
                Case("<" + tag + ">字</" + tag + ">", native);
                Case("<" + tag.ToUpperInvariant() + " = strange / value>字</" + tag.ToUpperInvariant() + ">", native);
            }
            // Every UTF-16 unit is exercised as a standalone/category boundary,
            // a possible regex separator, and a potential one-letter tag name.
            for (int code = 0; code <= 0xFFFF; code++)
            {
                string unit = new string((char)code, 1);
                Case(unit + "\u0301", native);
                Case("<b" + unit + ">X</b>", native);
                Case("<" + unit + ">X", native);
            }
            // Sweep every supplementary scalar through batched strings. Pair
            // boundaries never borrow a different runtime's Unicode database.
            var buffer = new StringBuilder(16384);
            for (int start = 0x10000; start < 0x110000; start += 4096)
            {
                buffer.Length = 0;
                for (int code = start; code < start + 4096; code++)
                {
                    int scalar = code - 0x10000;
                    buffer.Append((char)(0xD800 + (scalar >> 10))).Append((char)(0xDC00 + (scalar & 1023)));
                    buffer.Append('\u0301').Append(' ');
                }
                Case(buffer.ToString(), native);
            }
            uint random = 0x8A1739;
            for (int sample = 0; sample < 512; sample++)
            {
                buffer.Length = 0;
                for (int i = 0; i < 150; i++)
                {
                    random ^= random << 13; random ^= random >> 17; random ^= random << 5;
                    if ((random & 15) == 0) buffer.Append("<b>");
                    buffer.Append((char)(random >> 16));
                    if ((random & 31) == 0) buffer.Append("</b>");
                }
                Case(buffer.ToString(), native);
            }
            Console.WriteLine("PASS native text scan: " + _cases + " cases; all BMP + all supplementary scalars; native=" + native
                + "; reference=" + (Mono ? "original Unity Mono StringInfo/Regex" : native ? "explicit Mono segmentation with current BCL categories" : "original managed runtime")
                + (native ? "; retained table bytes=" + NativeTextScan.RetainedTableBytes : ""));
        }

        private static void Case(string text, bool native)
        {
            int expected = native && !Mono ? LegacyCount(text) : NativeTextScan.CountManaged(text);
            int actual = NativeTextScan.Count(text);
            if (actual != expected) throw new Exception("text scan differs: case=" + _cases + " expected=" + expected + " actual=" + actual + " UTF16=" + Hex(text));
            if (Mono && LegacyCount(text) != expected) throw new Exception("Frozen legacy rule differs from actual Mono: " + Hex(text));
            _cases++;
        }
        private static string Hex(string value)
        {
            if (value == null) return "null";
            var result = new StringBuilder();
            for (int i = 0; i < Math.Min(value.Length, 128); i++) result.Append(((int)value[i]).ToString("X4")).Append(' ');
            return result.ToString();
        }
        private static bool Combining(UnicodeCategory category)
        { return category == UnicodeCategory.NonSpacingMark || category == UnicodeCategory.SpacingCombiningMark || category == UnicodeCategory.EnclosingMark; }
        private static int ScalarLength(string text, int at)
        { return at + 1 < text.Length && char.IsHighSurrogate(text[at]) && char.IsLowSurrogate(text[at + 1]) ? 2 : 1; }
        private static int LegacyCount(string source)
        {
            string text = Tags.Replace(source ?? "", "");
            int count = 0, at = 0;
            while (at < text.Length)
            {
                UnicodeCategory category = CharUnicodeInfo.GetUnicodeCategory(text, at);
                bool visible = !char.IsWhiteSpace(text[at]);
                int width = ScalarLength(text, at);
                at += width;
                if (!Combining(category) && category != UnicodeCategory.Format && category != UnicodeCategory.Control
                    && category != UnicodeCategory.OtherNotAssigned && category != UnicodeCategory.Surrogate)
                {
                    while (at < text.Length && Combining(CharUnicodeInfo.GetUnicodeCategory(text, at)))
                    { visible = true; at += ScalarLength(text, at); }
                }
                if (visible) count++;
            }
            return count;
        }

        // Used only by the existing isolated Mono microbenchmark harness.
        // Both branches use the same text; result = [milliseconds, checksum, UTF16 length].
        internal static double[] Benchmark(bool native, int iterations)
        { return BenchmarkRepeated(native, iterations, 256); }
        internal static double[] BenchmarkShort(bool native, int iterations)
        { return BenchmarkRepeated(native, iterations, 2); }
        private static double[] BenchmarkRepeated(bool native, int iterations, int repetitions)
        {
            if (native && !NativeTextScan.IsAvailable) throw new Exception("text scan benchmark native missing");
            var text = new StringBuilder();
            for (int i = 0; i < repetitions; i++) text.Append("<color=#ffeebb>长段对白e\u0301与😀<b>组合</b></color> \n");
            string sample = text.ToString();
            int sum = 0;
            var watch = Stopwatch.StartNew();
            for (int i = 0; i < iterations; i++) sum += native ? NativeTextScan.Count(sample) : NativeTextScan.CountManaged(sample);
            watch.Stop();
            return new[] { watch.Elapsed.TotalMilliseconds, (double)sum, sample.Length };
        }
    }
}
