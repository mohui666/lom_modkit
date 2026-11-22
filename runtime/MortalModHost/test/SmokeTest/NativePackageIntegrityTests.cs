using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;

namespace MortalModHost
{
    internal static class NativePackageIntegrityTests
    {
        internal static void Run(bool requireNative)
        {
            if (requireNative && !NativeSha256.IsAvailable) throw new Exception("package integrity requires actual native SHA");
            PairedDigests();
            ZipTextSizing();
            string folder = Path.Combine(Path.GetTempPath(), "lom-package-pair-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(folder);
            try
            {
                var entries = new Dictionary<string, byte[]>(StringComparer.Ordinal)
                {
                    { "manifest.json", Utf8("{\"format\":3,\"package_format\":3,\"story_schema\":2,\"content_schema\":1,\"id\":\"paired_hash\",\"campaign_id\":\"paired-hash\",\"entry\":\"main\",\"campaign\":{\"new_game\":true}}") },
                    { "story/main.json", Utf8("{\"id\":\"main\",\"nodes\":[{\"id\":\"end\",\"type\":\"end\"}],\"start\":\"end\"}") },
                    { "lua/main.lua", Utf8("return mod_end()") },
                    { "lua/ja/main.lua", Utf8("return mod_end() -- ja") }
                };
                string header = "algorithm=" + ModLoader.StoryLuaHashAlgorithm + "\n";
                string row = Row(entries, "story/main.json", "lua/main.lua");
                string ja = Row(entries, "story/main.json", "lua/ja/main.lua");
                string record = header + row + ja;
                CheckPackage(folder, entries, record, null);
                CheckPackage(folder, entries, record.Replace("\n", "\r\n") + "\r\n", null);
                CheckPackage(folder, entries, "wrong\n" + row, ModLoader.StoryLuaHashEntry + " 算法标识无效");
                CheckPackage(folder, entries, header + "invalid\n" + row, ModLoader.StoryLuaHashEntry + " 第 2 行格式无效");
                CheckPackage(folder, entries, header + row + row + "invalid\n", ModLoader.StoryLuaHashEntry + " 重复声明 lua/main.lua");
                CheckPackage(folder, entries, header + Row(entries, "story/missing.json", "lua/missing.lua"),
                    ModLoader.StoryLuaHashEntry + " 引用不存在条目：story/missing.json / lua/missing.lua");
                var mismatched = new Dictionary<string, byte[]>(entries, StringComparer.Ordinal);
                mismatched["story/other.json"] = Utf8("{}");
                CheckPackage(folder, mismatched, header + Row(mismatched, "story/other.json", "lua/main.lua"),
                    ModLoader.StoryLuaHashEntry + " 的 Story/Lua id 不一致：story/other.json / lua/main.lua");
                CheckPackage(folder, entries, header + row + ja.Replace(Hash(entries["story/main.json"]), new string('0', 64)),
                    "Story/Lua 一致性校验失败：lua/ja/main.lua");
                CheckPackage(folder, entries, header + row, ModLoader.StoryLuaHashEntry + " 未完整覆盖全部 Lua 脚本");
                var substituted = new Dictionary<string, byte[]>(entries, StringComparer.Ordinal);
                substituted["lua/ja/main.lua"] = Utf8("return arbitrary_replacement()");
                CheckPackage(folder, substituted, record, "Story/Lua 一致性校验失败：lua/ja/main.lua");
                // Logical hash failure precedes malformed binding records.
                string path = Path.Combine(folder, "logical-first.lommod");
                Write(path, entries, "wrong\n", new string('0', 64));
                Expect(path, ModLoader.PackageContentHashEntry + " 与包内逻辑内容不一致");

                foreach (string name in new[] { "released-1.5.9-scenario-modes.lommod", "released-1.5.9-filters.lommod" })
                {
                    string original = Path.Combine(AppContext.BaseDirectory, name);
                    byte[] before = File.ReadAllBytes(original);
                    Load(original);
                    // Open a released archive, edit metadata, save/export and
                    // reopen it; historical Story/Lua bytes remain untouched.
                    var released = Read(original);
                    string manifest = Encoding.UTF8.GetString(released["manifest.json"]);
                    string edited = System.Text.RegularExpressions.Regex.Replace(manifest,
                        "\"name\"\\s*:\\s*\"(?:\\\\.|[^\"\\\\])*\"", "\"name\":\"历史工程摘要回归\"");
                    Check(edited != manifest, "released fixture must have editable name metadata");
                    released["manifest.json"] = Utf8(edited);
                    string oldRecord = Encoding.UTF8.GetString(released[ModLoader.StoryLuaHashEntry]);
                    string exported = Path.Combine(folder, name);
                    Write(exported, released, oldRecord, null);
                    Check(Load(exported).Name == "历史工程摘要回归", "released metadata export/reopen");
                    // Refresh only the logical content record, keeping the old
                    // binding record. An unrelated Lua remains rejected.
                    string lua = null;
                    foreach (string key in released.Keys) if (key.StartsWith("lua/", StringComparison.Ordinal) && key.EndsWith(".lua", StringComparison.Ordinal)) { lua = key; break; }
                    released[lua] = Utf8("return arbitrary_replacement()");
                    Write(exported, released, oldRecord, null);
                    Expect(exported, "Story/Lua 一致性校验失败：" + lua);
                    Check(Same(before, File.ReadAllBytes(original)), "released original must be untouched");
                }
            }
            finally { Directory.Delete(folder, true); }
            Console.WriteLine("PASS package paired integrity: error order, repeated Story/locale rows, released edit/save/reopen, Lua substitution; actual native=" + NativeSha256.IsAvailable);
        }

        private static void PairedDigests()
        {
            byte[] bytes = new byte[100003]; new Random(672).NextBytes(bytes);
            foreach (bool native in new[] { false, true })
            using (var first = new NativeSha256(native)) using (var second = new NativeSha256(native))
            {
                first.Append(bytes, 0, 3);
                first.AppendWith(second, bytes, 3, 0);
                for (int at = 3; at < bytes.Length; at += 997)
                    first.AppendWith(second, bytes, at, Math.Min(997, bytes.Length - at));
                Check(Same(first.Finish(), Digest(bytes)), "paired logical digest");
                var suffix = new byte[bytes.Length - 3]; Buffer.BlockCopy(bytes, 3, suffix, 0, suffix.Length);
                Check(Same(second.Finish(), Digest(suffix)), "paired entry digest");
            }
            using (var first = new NativeSha256()) using (var second = new NativeSha256())
            {
                Throws<ArgumentException>(() => first.AppendWith(first, bytes, 0, 1));
                Throws<ArgumentOutOfRangeException>(() => first.AppendWith(second, bytes, 0, bytes.Length + 1));
                second.Dispose();
                Throws<ObjectDisposedException>(() => first.AppendWith(second, bytes, 0, 1));
                Check(Same(first.Finish(), Digest(new byte[0])), "rejected pair must not update first digest");
            }
        }
        private static void ZipTextSizing()
        {
            MethodInfo exact = typeof(ModLoader).GetMethod("ReadZipTextBytes", BindingFlags.NonPublic | BindingFlags.Static);
            MethodInfo old = typeof(ModLoader).GetMethod("ReadBoundedBytes", BindingFlags.NonPublic | BindingFlags.Static);
            foreach (int actual in new[] { 0, 1, 7, 81921 })
            foreach (int declared in new[] { 0, 1, 7, 81921 })
            foreach (int maximum in new[] { 7, 100000 })
            {
                if (declared > maximum) continue; // Caller enforces the entry size limit first.
                var input = new byte[actual]; new Random(actual).NextBytes(input);
                byte[] expected = null, result = null; string expectedError = null, error = null;
                using (var stream = new ShortReads(input))
                    try { expected = (byte[])old.Invoke(null, new object[] { stream, (long)maximum, "fixture" }); }
                    catch (TargetInvocationException failure) { expectedError = failure.InnerException.GetType().Name + ":" + failure.InnerException.Message; }
                using (var stream = new ShortReads(input))
                    try { result = (byte[])exact.Invoke(null, new object[] { stream, (long)declared, (long)maximum, "fixture" }); }
                    catch (TargetInvocationException failure) { error = failure.InnerException.GetType().Name + ":" + failure.InnerException.Message; }
                Check(error == expectedError && (error != null || Same(result, expected)), "ZIP text sizing/short-read/actual-limit equivalence");
            }
        }
        private sealed class ShortReads : MemoryStream
        {
            internal ShortReads(byte[] input) : base(input, false) { }
            public override int Read(byte[] buffer, int offset, int count) { return base.Read(buffer, offset, Math.Min(count, 37)); }
        }
        private static void CheckPackage(string folder, Dictionary<string, byte[]> entries, string record, string error)
        { string path = Path.Combine(folder, "case.lommod"); Write(path, entries, record, null); if (error == null) Load(path); else Expect(path, error); }
        private static string Row(Dictionary<string, byte[]> entries, string story, string lua)
        { byte[] storyBytes, luaBytes; return story + "\t" + (entries.TryGetValue(story, out storyBytes) ? Hash(storyBytes) : new string('0', 64)) + "\t" + lua + "\t" + (entries.TryGetValue(lua, out luaBytes) ? Hash(luaBytes) : new string('0', 64)) + "\n"; }
        private static ModPackage Load(string path)
        {
            try { return (ModPackage)typeof(ModLoader).GetMethod("LoadPackage", BindingFlags.NonPublic | BindingFlags.Static).Invoke(null, new object[] { path, null, null }); }
            catch (TargetInvocationException error) { throw error.InnerException; }
        }
        private static void Expect(string path, string message)
        { try { Load(path); } catch (FormatException error) { Check(error.Message == message, "error order: expected " + message + "; got " + error.Message); return; } throw new Exception("invalid package accepted: " + message); }
        private static Dictionary<string, byte[]> Read(string path)
        {
            var result = new Dictionary<string, byte[]>(StringComparer.Ordinal);
            using (var input = File.OpenRead(path)) using (var zip = new ZipArchive(input, ZipArchiveMode.Read))
                foreach (var entry in zip.Entries) { using (var source = entry.Open()) using (var bytes = new MemoryStream()) { source.CopyTo(bytes); result.Add(entry.FullName, bytes.ToArray()); } }
            return result;
        }
        private static void Write(string path, Dictionary<string, byte[]> source, string binding, string forcedHash)
        {
            var entries = new Dictionary<string, byte[]>(source, StringComparer.Ordinal);
            entries.Remove(ModLoader.PackageContentHashEntry);
            entries[ModLoader.StoryLuaHashEntry] = Utf8(binding);
            var names = new List<string>(entries.Keys); names.Sort(StringComparer.Ordinal);
            using (var bytes = new MemoryStream())
            {
                foreach (string name in names)
                {
                    byte[] encoded = Utf8(name), data = entries[name];
                    BigEndian(bytes, (ulong)encoded.Length, 4); bytes.Write(encoded, 0, encoded.Length);
                    BigEndian(bytes, (ulong)data.Length, 8); bytes.Write(data, 0, data.Length);
                }
                entries[ModLoader.PackageContentHashEntry] = Utf8("algorithm=" + ModLoader.PackageContentHashAlgorithm + "\nsha256=" + (forcedHash ?? Hash(bytes.ToArray())) + "\n");
            }
            using (var output = File.Create(path)) using (var zip = new ZipArchive(output, ZipArchiveMode.Create))
                foreach (var entry in entries) using (var stream = zip.CreateEntry(entry.Key).Open()) stream.Write(entry.Value, 0, entry.Value.Length);
        }
        private static void BigEndian(Stream stream, ulong value, int count) { for (int shift = (count - 1) * 8; shift >= 0; shift -= 8) stream.WriteByte((byte)(value >> shift)); }
        private static byte[] Utf8(string value) { return new UTF8Encoding(false).GetBytes(value); }
        private static byte[] Digest(byte[] value) { using (var digest = SHA256.Create()) return digest.ComputeHash(value); }
        private static string Hash(byte[] value) { return BitConverter.ToString(Digest(value)).Replace("-", ""); }
        private static bool Same(byte[] first, byte[] second) { if (first.Length != second.Length) return false; for (int i = 0; i < first.Length; i++) if (first[i] != second[i]) return false; return true; }
        private static void Check(bool condition, string reason) { if (!condition) throw new Exception(reason); }
        private static void Throws<T>(Action action) where T : Exception { try { action(); } catch (T) { return; } throw new Exception("expected " + typeof(T).Name); }
    }
}
