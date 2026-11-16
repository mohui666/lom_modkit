using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;

namespace MortalModHost
{
    internal static class NativeSha256Tests
    {
        internal static void Run(bool requireNative)
        {
            if (requireNative && !NativeSha256.IsAvailable) throw new Exception("SHA-256 未实际加载 Rust");
            var random = new Random(2079);
            foreach (int length in new[] { 0, 1, 55, 56, 63, 64, 65, 81920, 1000003 })
            {
                var data = new byte[length + 7]; random.NextBytes(data);
                byte[] expected;
                using (var sha = SHA256.Create()) expected = sha.ComputeHash(data, 3, length);
                foreach (bool native in new[] { false, true })
                using (var digest = new NativeSha256(native))
                {
                    digest.Append(data, 3, 0);
                    for (int at = 0; at < length;)
                    {
                        int count = Math.Min(length - at, random.Next(1, 1000));
                        digest.Append(data, at + 3, count); at += count;
                    }
                    Same(expected, digest.Finish());
                    Throws<InvalidOperationException>(() => digest.Finish());
                    Throws<InvalidOperationException>(() => digest.Append(data, 0, 0));
                }
                using (var stream = new ShortReads(data, 3, length))
                    Same(expected, NativeSha256.Hash(stream, length, length, "fixture"));
            }
            using (var digest = new NativeSha256())
            {
                Throws<ArgumentOutOfRangeException>(() => digest.Append(new byte[1], -1, 1));
                Throws<ArgumentOutOfRangeException>(() => digest.Append(new byte[1], 1, 1));
                digest.Dispose(); digest.Dispose();
                Throws<ObjectDisposedException>(() => digest.Finish());
            }
            Throws<FormatException>(() => NativeSha256.Hash(new MemoryStream(new byte[5]), 4, -1, "limit"));
            Throws<FormatException>(() => NativeSha256.Hash(new MemoryStream(new byte[5]), 10, 4, "long"));
            Throws<FormatException>(() => NativeSha256.Hash(new MemoryStream(new byte[3]), 10, 4, "short"));
            foreach (string name in new[] { "released-1.5.9-scenario-modes.lommod", "released-1.5.9-filters.lommod" })
            {
                string folder = Path.Combine(Path.GetTempPath(), "lom-native-hash-" + Guid.NewGuid().ToString("N"));
                Directory.CreateDirectory(folder);
                try
                {
                    string source = Path.Combine(AppContext.BaseDirectory, name);
                    File.Copy(source, Path.Combine(folder, name));
                    var warnings = new List<string>();
                    var packages = ModLoader.ScanMods(folder, _ => { }, warnings.Add);
                    if (packages.Count != 1) throw new Exception("历史包加载失败：" + string.Join(";", warnings));
                    byte[] fingerprint;
                    using (var sha = SHA256.Create()) using (var input = File.OpenRead(source)) fingerprint = sha.ComputeHash(input);
                    if (packages[0].PackageFingerprint != BitConverter.ToString(fingerprint).Replace("-", ""))
                        throw new Exception("历史包指纹改变");
                }
                finally { Directory.Delete(folder, true); }
            }
            Console.WriteLine("PASS streaming SHA-256: chunk boundaries, bounded reads, historical packages; actual native=" + NativeSha256.IsAvailable);
        }
        private sealed class ShortReads : MemoryStream
        {
            internal ShortReads(byte[] data, int at, int length) : base(data, at, length) { }
            public override int Read(byte[] data, int at, int length) { return base.Read(data, at, Math.Min(length, 37)); }
        }
        private static void Same(byte[] expected, byte[] actual)
        {
            if (BitConverter.ToString(expected) != BitConverter.ToString(actual)) throw new Exception("SHA-256 字节不一致");
        }
        private static void Throws<T>(Action action) where T : Exception
        { try { action(); } catch (T) { return; } throw new Exception("未抛出 " + typeof(T).Name); }
    }
}
