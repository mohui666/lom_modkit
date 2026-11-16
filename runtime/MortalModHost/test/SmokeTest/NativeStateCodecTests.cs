using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;

namespace MortalModHost
{
    internal static class NativeStateCodecTests
    {
        private const string Prefix = "LOMMOD_SCENARIO_V1:owner_a:";
        private static int _cases;
        private static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
        private static string Capture(Func<string> read)
        {
            try { return "value:" + read(); }
            catch (Exception error) { return error.GetType().FullName + ":" + error.Message; }
        }
        private static void Encode(string name, string json)
        {
            Compare(name, () => NativeStateCodec.EncodeManaged(Prefix, json), () => NativeStateCodec.EncodeRecord(Prefix, json));
        }
        private static void Decode(string name, string payload)
        {
            Compare(name, () => NativeStateCodec.DecodeManaged(payload == null ? null : Prefix + payload, payload == null ? 0 : Prefix.Length),
                () => NativeStateCodec.DecodeRecord(payload == null ? null : Prefix + payload, payload == null ? 0 : Prefix.Length));
        }
        private static void Compare(string name, Func<string> expected, Func<string> actual)
        {
            string a = Capture(expected), b = Capture(actual);
            if (!string.Equals(a, b, StringComparison.Ordinal))
                throw new Exception("State codec " + name + ": expected=" + Short(a) + "; actual=" + Short(b));
            _cases++;
        }
        private static string Short(string value) { return value.Length <= 600 ? value : value.Substring(0, 600) + "..."; }
        private static void Check(bool value, string message) { if (!value) throw new Exception("State codec " + message); }
        internal static void Run(bool requireNative)
        {
            if (requireNative && !NativeStateCodec.IsAvailable) throw new Exception("State codec native required: " + NativeHostKernel.UnavailableReason);
            _cases = 0;
            foreach (string value in new[] { null, "", "a", "ab", "abc", "abcd", "\0正文😀\uFEFF", "\uD800", "\uDC00", "x\uD800y", "\uD800\uD800\uDC00", "\uD800\uDC00\uDC00" })
                Encode("UTF16 " + (value == null ? "null" : value.Length.ToString()), value);
            Encode("exact 128 KiB", new string('x', 128 * 1024));
            Encode("above 128 KiB", new string('x', 128 * 1024 + 1));
            Encode("exact UTF8 128 KiB", new string('中', 43690) + "ab");
            Encode("UTF8 exceeds", new string('中', 43690) + "abc");
            Encode("encoder error precedes limit", new string('x', 128 * 1024 + 1) + "\uD800");
            var bmp = new StringBuilder();
            for (int i = 0; i <= 65535; ++i) if (i < 0xd800 || i > 0xdfff) bmp.Append((char)i);
            string units = bmp.ToString();
            for (int start = 0; start < units.Length; start += 10000)
            {
                string text = units.Substring(start, Math.Min(10000, units.Length - start));
                Encode("BMP block " + start, text);
                Decode("BMP roundtrip " + start, Convert.ToBase64String(Utf8.GetBytes(text)));
            }
            foreach (string payload in new[] { null, "", "Zg==", "Zm8=", "Zm9v", "Zg", "Zg=", "Z===", "====", "=", "A", "AA", "AAA", "AAAA=", "AAAA==", "AAAA===", "AA=A", "=AAA", "AA==AA==", "A?==", "AA==?", "\0", "  \t\r\n", " Z \tg \r= \n= ", "/w==?", "/w==", "AA==\0" })
                Decode("base64 " + payload, payload);
            foreach (char space in new[] { '\t', '\n', '\r', ' ', '\0', '\v', '\f', '\u0085', '\u00a0', '\u1680', '\u2000', '\u200b', '\u2028', '\u3000', '\uFEFF' })
                foreach (int at in new[] { 0, 1, 2, 3, 4 }) Decode("whitespace " + (int)space + "/" + at, "Zg==".Insert(at, space.ToString()));
            const string alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            for (int bits = 0; bits < 16; ++bits) Decode("ignored four pad bits " + bits, "Z" + alphabet[32 + bits] + "==");
            for (int bits = 0; bits < 4; ++bits) Decode("ignored two pad bits " + bits, "Zm" + alphabet[60 + bits] + "=");
            foreach (byte[] bytes in new[] {
                new byte[] {0x80}, new byte[] {0xff}, new byte[] {0xc0,0x80}, new byte[] {0xc1,0xbf},
                new byte[] {0xe0,0x80,0x80}, new byte[] {0xed,0xa0,0x80}, new byte[] {0xf0,0x80,0x80,0x80},
                new byte[] {0xf4,0x90,0x80,0x80}, new byte[] {0xf5,0x80,0x80,0x80}, new byte[] {0xc2},
                new byte[] {0xe2,0x82}, new byte[] {0xf0,0x9f,0x98}, new byte[] {0xe2,0x20,0xac},
                new byte[] {0xc2,0x80}, new byte[] {0xdf,0xbf}, new byte[] {0xe0,0xa0,0x80},
                new byte[] {0xed,0x9f,0xbf}, new byte[] {0xf0,0x90,0x80,0x80}, new byte[] {0xf4,0x8f,0xbf,0xbf}
            })
            {
                string payload = Convert.ToBase64String(bytes);
                Decode("strict UTF8 " + payload, payload);
                Decode("base64 failure wins " + payload, payload + "?");
            }
            Decode("load permits >128 KiB", Convert.ToBase64String(new byte[135000]));
            Decode("exact encoded cap", new string(' ', 180000));
            Decode("over encoded cap", new string(' ', 180001));
            Decode("encoded cap precedes invalid data", "?" + new string(' ', 180000));
            var random = new Random(727);
            for (int c = 0; c < 120; ++c)
            {
                var text = new StringBuilder();
                for (int i = 0; i < 300; ++i)
                {
                    int scalar = random.Next(0x110000);
                    if (scalar < 0xd800 || scalar > 0xdfff) text.Append(char.ConvertFromUtf32(scalar));
                }
                string value = text.ToString();
                Encode("scalar random " + c, value);
                Decode("UTF8 random " + c, Convert.ToBase64String(Utf8.GetBytes(value)));
                var encoded = new StringBuilder();
                string symbols = alphabet + "= \t?\0";
                for (int i = 0; i < random.Next(1, 40); ++i) encoded.Append(symbols[random.Next(symbols.Length)]);
                Decode("malformed random " + c, encoded.ToString());
            }
            NamespaceRoundTrip();
            Console.WriteLine("PASS state codec: " + _cases + " exact output/exception differential cases; namespace/state roundtrip; native=" + NativeStateCodec.IsAvailable);
        }
        private static void NamespaceRoundTrip()
        {
            var state = new ScenarioState { Pending = "visit", Result = "completed" };
            state.Variables.Add("count", int.MaxValue); state.Tasks.Add("quest", "active");
            state.Attempts.Add("visit", "1/7/1/3"); state.Pools.Add("pool", "1/7/1"); state.Completed.Add("opening");
            string json = state.Serialize(), record = NativeStateCodec.EncodeRecord(Prefix, json);
            const string otherPrefix = "LOMMOD_SCENARIO_V1:owner_b:";
            string other = NativeStateCodec.EncodeRecord(otherPrefix, new ScenarioState().Serialize());
            var keys = new List<string> { "official_key", null, other, "LOMMOD_SCENARIO_V1:owner_a_suffix:bad", record };
            string[] before = keys.ToArray();
            var reopened = ScenarioState.Parse(NativeStateCodec.ReadRecord(keys, Prefix, "package_a"));
            Check(reopened.Serialize() == json && reopened.Pending == "visit" && reopened.Tasks["quest"] == "active", "owned record state roundtrip");
            Check(ScenarioState.Parse(NativeStateCodec.ReadRecord(keys, otherPrefix, "package_b")).Pending == null, "other namespace roundtrip");
            Check(NativeStateCodec.ReadRecord(keys, "LOMMOD_SCENARIO_V1:absent:", "absent") == null, "missing record remains absent");
            for (int i = 0; i < before.Length; ++i) Check(before[i] == keys[i], "read does not mutate keys");
            keys.Add(record);
            Check(Capture(() => NativeStateCodec.ReadRecord(keys, Prefix, "package_a")) == "System.InvalidOperationException:事件存档存在重复记录：package_a", "duplicate records rejected");
            keys.RemoveAt(keys.Count - 1); keys[keys.Count - 1] = Prefix + "?";
            Check(Capture(() => NativeStateCodec.ReadRecord(keys, Prefix, "package_a")).StartsWith("System.FormatException:", StringComparison.Ordinal), "damaged Base64 rejected");
            keys[keys.Count - 1] = NativeStateCodec.EncodeRecord(Prefix, "{\"schema\":99}");
            Check(Capture(() => ScenarioState.Parse(NativeStateCodec.ReadRecord(keys, Prefix, "package_a")).Serialize()).StartsWith("System.FormatException:", StringComparison.Ordinal), "damaged state schema still rejected");
            keys[keys.Count - 1] = Prefix + new string(' ', 180001); keys.Add(record);
            Check(Capture(() => NativeStateCodec.ReadRecord(keys, Prefix, "package_a")) == "System.InvalidOperationException:事件存档存在重复记录：package_a", "duplicate check precedes size check");
        }

        // A valid state JSON exactly 128 KiB in UTF-8; creation is outside timing.
        internal static string BenchmarkJson()
        {
            var state = new ScenarioState();
            for (int i = 0; i < 920; ++i) state.Attempts.Add("event_" + i, new string('x', 120));
            string json = state.Serialize();
            var result = new StringBuilder(json);
            while (result.Length < 128 * 1024) result.Append(' ');
            Check(result.Length == 128 * 1024, "benchmark 128 KiB fixture");
            ScenarioState.Parse(result.ToString());
            return result.ToString();
        }
        // Milliseconds and checksum, input generation excluded. Operation true
        // encodes the final namespaced record; false decodes that complete record.
        internal static double[] Benchmark(bool native, bool encode, int iterations)
        {
            string json = BenchmarkJson(), record = NativeStateCodec.EncodeManaged(Prefix, json);
            Func<string> run = encode
                ? (Func<string>)(() => native ? NativeStateCodec.EncodeRecord(Prefix, json) : NativeStateCodec.EncodeManaged(Prefix, json))
                : (() => native ? NativeStateCodec.DecodeRecord(record, Prefix.Length) : NativeStateCodec.DecodeManaged(record, Prefix.Length));
            run(); long checksum = 0;
            var timer = Stopwatch.StartNew();
            for (int i = 0; i < iterations; ++i) checksum += run().Length;
            timer.Stop(); return new[] { timer.Elapsed.TotalMilliseconds, (double)checksum };
        }
    }
}
