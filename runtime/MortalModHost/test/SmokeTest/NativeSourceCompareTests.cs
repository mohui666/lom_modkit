using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Reflection;
using System.Text;

namespace MortalModHost
{
    internal static class NativeSourceCompareTests
    {
        private const BindingFlags Members = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;
        private static string Q(string text)
        {
            if (text == null) return "null";
            var result = new StringBuilder("\"");
            foreach (char c in text)
            {
                if (c == '"' || c == '\\') result.Append('\\').Append(c);
                else if (c < 32 || char.IsSurrogate(c)) result.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                else result.Append(c);
            }
            return result.Append('"').ToString();
        }
        private static string Node(string id, int origin, string fields)
        { return "{\"id\":" + Q(id) + ",\"type\":\"say\",\"text\":\"正文\",\"character\":\"趙活\",\"_editor\":{\"vanilla_origin\":" + origin + "}," + fields + "}"; }
        private static string Story(string[] original, string[] nodes, int schema = 2, string extra = "", string[] codes = null)
        {
            var result = new StringBuilder("{\"vanilla_import\":{\"schema\":").Append(schema).Append(",\"target\":\"S0101\",\"origins\":[");
            for (int i = 0; i < original.Length; ++i)
            {
                if (i > 0) result.Append(',');
                result.Append("{\"code\":").Append(Q(codes == null ? "-- original " + i + "\n" : codes[i])).Append(",\"node\":").Append(original[i]).Append('}');
            }
            return result.Append("]},\"nodes\":[").Append(string.Join(",", nodes)).Append(']').Append(extra).Append('}').ToString();
        }

        private static IEnumerable<KeyValuePair<string, string>> Cases()
        {
            string a = Node("a", 0, "\"goto\":\"b\",\"font_size\":40");
            string b = Node("b", 1, "\"goto\":\"c\"");
            string c = "{\"id\":\"c\",\"type\":\"raw\",\"code\":\"wait(1)\",\"_editor\":{\"vanilla_origin\":2}}";
            string[] originals = { a, b, c };
            yield return Pair("untouched", Story(originals, originals));
            yield return Pair("schema1", Story(originals, originals, 1));
            foreach (string field in new[] { "\"text\":\"新字\"", "\"font_size\":28", "\"voice\":\"audio.wav\"", "\"mood_icon\":\"happy\"", "\"mood_character\":\"npc\"", "\"character\":\"other\"", "\"position\":1" })
                yield return Pair("native say " + field, Story(originals, new[] { a.Insert(a.Length - 1, "," + field), b, c }));
            yield return Pair("remove presentation fields", Story(originals, new[] { Node("a", 0, "\"goto\":\"b\",\"text\":null"), b, c }));
            yield return Pair("raw changed", Story(originals, new[] { a, b, c.Replace("wait(1)", "wait(2)") }));
            yield return Pair("reordered", Story(originals, new[] { b, a, c }));
            yield return Pair("deleted", Story(originals, new[] { a, c }));
            yield return Pair("duplicate origin", Story(originals, new[] { a, a.Replace("\"id\":\"a\"", "\"id\":\"extra\""), b, c }));
            yield return Pair("root metadata ignored", Story(originals, new[] { a.Replace("\"id\":\"a\"", "\"id\":\"renamed\"").Replace("\"vanilla_origin\":0", "\"arbitrary\":5,\"vanilla_origin\":0"), b, c }));
            foreach (string type in new[] { "say", "wait", "music", "choice", "stat_check", "end" })
            {
                string inserted = "{\"id\":\"extra\",\"type\":" + Q(type) + ",\"inherit_dialog\":true,\"goto\":\"b\"}";
                yield return Pair("insert " + type, Story(originals, new[] { a.Replace("\"goto\":\"b\"", "\"goto\":\"extra\""), inserted, b, c }));
            }
            yield return Pair("inserted noninherit say", Story(originals, new[] { a, "{\"id\":\"extra\",\"type\":\"say\"}", b, c }));
            string detour = a.Replace("\"goto\":\"b\"", "\"goto\":\"extra\"");
            yield return Pair("schema1 no detour normalization", Story(originals, new[] { detour, "{\"id\":\"extra\",\"type\":\"wait\",\"goto\":\"b\"}", b, c }, 1));
            foreach (string inserted in new[] {
                "{\"id\":\"extra\",\"type\":\"wait\"}",
                "{\"id\":\"extra\",\"type\":\"wait\",\"goto\":\"missing\"}",
                "{\"id\":\"extra\",\"type\":\"wait\",\"goto\":\"extra\"}",
                "{\"id\":\"extra\",\"type\":null,\"goto\":\"b\"}",
                "{\"id\":\"extra\",\"type\":\"end\",\"resume_original\":true,\"resume_node\":\"b\"}",
                "{\"id\":\"extra\",\"type\":\"choice\",\"options\":[{\"goto\":\"b\"},{\"goto\":\"b\"}]}",
                "{\"id\":\"extra\",\"type\":\"choice\",\"options\":[{\"goto\":\"b\"},{\"goto\":\"c\"}]}",
                "{\"id\":\"extra\",\"type\":\"wait\",\"success\":\"branch\",\"failure\":\"branch\"}"
            }) yield return Pair("detour " + inserted, Story(originals, new[] { detour, inserted, "{\"id\":\"branch\",\"type\":\"wait\",\"goto\":\"b\"}", b, c }));
            yield return Pair("mood changed", Story(originals, originals, 2, ",\"mood\":false"));
            yield return Pair("translation", Story(originals, originals, 2, ",\"localization\":{\"translations\":{\"en\":{\"a.text\":\"translated\",\"story.title\":\"title\",\"b\":\"no field\",\".text\":\"no id\"}}}"));
            foreach (string languages in new[] { "null", "1", "[]", "{\"a\":null,\"b\":1}", "{\"a\":1,\"b\":null}", "{\"a\":null,\"a\":{},\"b\":1}", "{\"a\":{},\"b\":null}" })
                yield return Pair("invalid translation " + languages, Story(originals, originals, 2, ",\"localization\":{\"translations\":" + languages + "}"));
            yield return Pair("external", Story(originals, originals).Replace("\"schema\":2", "\"schema\":2,\"external\":true"));
            yield return Pair("UTF16 source across origins", Story(originals, originals, 2, "", new[] { "\uFEFF\uD800", "\uDC00\0\uD800", "\uDC01\uDC00" }));
            yield return Pair("duplicate last ID", Story(originals, new[] { a, b, c, "{\"id\":\"extra\",\"type\":\"wait\"}", "{\"id\":\"extra\",\"type\":\"end\",\"resume_original\":true}" }));
            foreach (string left in new[] { "9007199254740992", "1.0000000000000002", "-0.0", "1e-300", "[1,{\"id\":7,\"_editor\":{\"x\":1}}]", "{\"x\":1,\"x\":2}", Q("\0\uD800😀") })
            {
                string valueA = Node("a", 0, "\"value\":" + left);
                foreach (string right in new[] { left, "9007199254740993", "1.0000000000000004", "0.0", "[1,{\"id\":8,\"_editor\":{\"x\":1}}]", "{\"x\":2}" })
                    yield return Pair("numeric/structure " + left + " vs " + right, Story(new[] { valueA }, new[] { Node("a", 0, "\"value\":" + right) }));
            }
            foreach (string value in new[] { "null", "true", "{}", "{\"schema\":1}", "{\"schema\":3}", "{\"schema\":2,\"target\":\"MOD_bad\"}", "{\"schema\":2,\"target\":null}", "{\"schema\":2,\"target\":\"S1\",\"origins\":null}" })
                yield return Pair("invalid metadata " + value, "{\"vanilla_import\":" + value + ",\"nodes\":[]}");
            foreach (string json in new[] { "null", "[]", "{}", "{\"vanilla_import\":{},\"vanilla_import\":null}", "{\"vanilla_import\":{\"schema\":2,\"target\":\"S\",\"origins\":[]},\"nodes\":[]}" })
                yield return Pair("root " + json, json);
            foreach (string node in new[] { "null", "1", "{}", "{\"id\":null}", "{\"id\":7}", "{\"id\":\"a\"}", "{\"id\":\"a\",\"type\":null}", "{\"id\":\"a\",\"type\":false}" })
                yield return Pair("invalid actual " + node, Story(new[] { a }, new[] { node }));
            foreach (int schema in new[] { 1, 2 })
                foreach (string baseline in new[] { "null", "1", "{}", "{\"type\":null}" })
                    yield return Pair("invalid baseline " + schema + " " + baseline, Story(new[] { baseline }, new[] { a }, schema));
            foreach (string origin in new[] { "null", "1", "{}", "{\"code\":null}", "{\"code\":1}", "{\"code\":\"\"}" })
                yield return Pair("invalid origin " + origin, "{\"vanilla_import\":{\"schema\":2,\"target\":\"S1\",\"origins\":[" + origin + "]},\"nodes\":[" + a + "]}");
            yield return Pair("source exact 4 MiB", Story(new[] { a }, new[] { a }, 2, "", new[] { new string('x', 4 * 1024 * 1024) }));
            yield return Pair("source UTF8 exceeds", Story(new[] { a }, new[] { a }, 2, "", new[] { new string('中', 1400000) }));
            yield return Pair("source unit exceeds", Story(new[] { a }, new[] { a }, 2, "", new[] { new string('x', 4 * 1024 * 1024 + 1) }));
            var random = new Random(1847);
            for (int i = 0; i < 100; ++i)
            {
                string baseNode = Node("a", 0, "\"font_size\":" + random.Next(1, 201) + ",\"payload\":{\"x\":[1,2,3],\"z\":true}");
                string changed = baseNode;
                switch (random.Next(7))
                {
                    case 0: changed = changed.Replace("正文", "新字"); break;
                    case 1: changed = changed.Replace("趙活", "other"); break;
                    case 2: changed = changed.Replace("[1,2,3]", "[3,2,1]"); break;
                    case 3: changed = changed.Replace("\"id\":\"a\"", "\"id\":\"extra\""); break;
                    case 4: changed = changed.Replace("\"vanilla_origin\":0", "\"vanilla_origin\":1"); break;
                    case 5: changed = changed.Replace("\"x\":[1,2,3],\"z\":true", "\"z\":true,\"x\":[1,2,3]"); break;
                }
                yield return Pair("random " + i, Story(new[] { baseNode }, new[] { changed }));
            }
        }
        private static KeyValuePair<string, string> Pair(string name, string json)
        { return new KeyValuePair<string, string>(name, json); }
        private static object Call(object source, string method, params object[] arguments)
        {
            try { return source.GetType().GetMethod(method, Members).Invoke(source, arguments); }
            catch (TargetInvocationException error) { throw error.InnerException; }
        }
        private static string Capture(Func<string, object> parse, string json)
        {
            try
            {
                object source = parse(json);
                if (source == null) return "null";
                Type type = source.GetType();
                string original = (string)type.GetProperty("OriginalSource", Members).GetValue(source, null);
                var result = new StringBuilder();
                foreach (string property in new[] { "Target", "OriginalSource", "LogicChanged", "OriginalVerified" })
                {
                    string value = Convert.ToString(type.GetProperty(property, Members).GetValue(source, null), CultureInfo.InvariantCulture);
                    result.Append(value.Length).Append(':').Append(value).Append('|');
                }
                var ids = new SortedSet<string>(StringComparer.Ordinal) { "a", "b", "c", "extra", "branch", "renamed", "missing" };
                foreach (string field in new[] { "_original", "_persistent" })
                    foreach (string id in (IEnumerable<string>)type.GetField(field, Members).GetValue(source)) ids.Add(id);
                try { Call(source, "VerifyOriginal", original + "-- stale"); result.Append("verified-stale|"); }
                catch (Exception e) { result.Append(e.GetType().FullName).Append(':').Append(e.Message).Append('|'); }
                Call(source, "VerifyOriginal", "\uFEFF" + original);
                foreach (string id in ids)
                    result.Append(Call(source, "Begin")).Append(':').Append(id).Append(':').Append(Call(source, "Node", id)).Append(':').Append(Call(source, "ContinueOriginal")).Append('|');
                result.Append(Call(source, "Begin"));
                foreach (string id in ids) result.Append(':').Append(Call(source, "Node", id)).Append(':').Append(Call(source, "ContinueOriginal"));
                result.Append(':').Append(Call(source, "Begin"));
                return result.ToString();
            }
            catch (Exception e) { return e.GetType().FullName + ":" + e.Message; }
        }
        private static object ParseNative(string json)
        {
            VanillaImportSource result;
            if (!NativeSourceCompare.TryParse(json, out result)) throw new Exception("Native source comparison required");
            return result;
        }
        internal static void CompareReference(Func<string, object> reference)
        {
            int count = 0;
            foreach (var item in Cases())
            {
                string expected = Capture(reference, item.Value), actual = Capture(ParseNative, item.Value);
                if (!string.Equals(expected, actual, StringComparison.Ordinal))
                    throw new Exception("Source compare differential " + item.Key + ": expected=" + Short(expected) + "; actual=" + Short(actual));
                count++;
            }
            Console.WriteLine("PASS native source compare: " + count + " exact classification/error/state cases");
        }
        private static string Short(string text) { return text.Length <= 900 ? text : text.Substring(0, 900) + "..."; }
        internal static void Run(bool requireNative)
        {
            if (!NativeSourceCompare.IsAvailable)
            {
                if (requireNative) throw new Exception("Source compare native backend required: " + NativeHostKernel.UnavailableReason);
                Console.WriteLine("SKIP native source compare differential: backend unavailable"); return;
            }
            CompareReference(json => VanillaImportSource.ParseManaged(json));
        }

        // End-to-end parse/classify milliseconds and a retained result checksum.
        // Numbers remain raw JSON, never a round-trip through 15-digit ToJson.
        internal static double[] Benchmark(bool native, int iterations)
        {
            if (native && !NativeSourceCompare.IsAvailable) throw new Exception("Native source compare required");
            var nodes = new string[1000];
            for (int i = 0; i < nodes.Length; ++i)
                nodes[i] = Node("n" + i, i, "\"font_size\":40,\"value\":1.0000000000000002,\"nested\":{\"a\":[1,2,3],\"b\":true}");
            string json = Story(nodes, nodes);
            Func<VanillaImportSource> run = () => native ? VanillaImportSource.Parse(json) : VanillaImportSource.ParseManaged(json);
            run(); long checksum = 0;
            var timer = Stopwatch.StartNew();
            for (int i = 0; i < iterations; ++i)
            {
                VanillaImportSource value = run(); value.VerifyOriginal(value.OriginalSource);
                checksum += value.OriginalSource.Length + (value.Begin() && value.Node("n999") ? 1 : 0);
            }
            timer.Stop();
            return new[] { timer.Elapsed.TotalMilliseconds, (double)checksum };
        }
    }
}
