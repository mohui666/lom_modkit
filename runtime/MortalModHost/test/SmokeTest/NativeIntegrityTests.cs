using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;

namespace MortalModHost
{
    internal static class NativeIntegrityTests
    {
        private static int _cases;
        private static bool _native;
        private static readonly string Fingerprint = new string('A', 64);

        internal static void Run(bool requireNative)
        {
            _cases = 0;
            _native = NativeIntegrity.IsAvailable;
            if (requireNative && !_native)
                throw new InvalidOperationException("本次必须使用 native，不能以 managed 回退通过："
                    + NativeIntegrity.UnavailableReason);
            if (_native)
                Check(Path.IsPathRooted(NativeIntegrity.LoadedPath) && File.Exists(NativeIntegrity.LoadedPath),
                    "loaded native path must be an existing absolute file");
            else
            {
                var notices = new List<string>();
                NativeIntegrity.ReportFallbackOnce(notices.Add);
                NativeIntegrity.ReportFallbackOnce(notices.Add);
                Check(notices.Count == 1 && !NativeIntegrity.FallbackNoticePending,
                    "unavailable backend must report once without retrying");
                Console.WriteLine(notices[0]);
            }

            byte[] seal = DisclosureIntegrity.CreateSessionSeal("demo_mod", Fingerprint);
            Case("valid", "demo_mod", Fingerprint, seal, 3);
            Case("lowercase fingerprint", "demo_mod", Fingerprint.ToLowerInvariant(), seal, 3);
            string mixed = "0123456789aBcDeF0123456789aBcDeF0123456789aBcDeF0123456789aBcDeF";
            Case("mixed fingerprint", "demo_mod", mixed,
                DisclosureIntegrity.CreateSessionSeal("demo_mod", mixed), 3);
            Case("mod id is case sensitive", "DEMO_MOD", Fingerprint, seal, 0);
            Case("uppercase mod own seal", "DEMO_MOD", Fingerprint,
                DisclosureIntegrity.CreateSessionSeal("DEMO_MOD", Fingerprint), 3);
            foreach (string mod in new[]
            {
                "中文角色", "emoji\ud83d\ude00", "e\u0301", "\u00e9",
                "\ud800", "\udc00", "\ud800\ud800", "\udc00\ud800",
                "\ud800A\udc00", "a\0b", "\0", " \t", "a\nb",
                new string('x', 8193)
            })
                Case("UTF16 mod id", mod, Fingerprint,
                    DisclosureIntegrity.CreateSessionSeal(mod, Fingerprint), 3);
            foreach (string mod in new string[] { null, "" })
                Case("empty mod id", mod, Fingerprint, seal, 0);
            foreach (string fingerprint in new string[]
            {
                null, "", new string('A', 63), new string('A', 65),
                new string('G', 64), new string('\uff21', 64),
                new string('0', 63) + "\0", new string('0', 63) + "\ud800"
            })
                Case("invalid fingerprint", "demo_mod", fingerprint, seal, 0);
            foreach (byte[] candidate in new byte[][] { null, new byte[0], new byte[31], new byte[33] })
                Case("invalid seal length", "demo_mod", Fingerprint, candidate, 0);
            for (int index = 0; index < seal.Length; index++)
            {
                var changed = (byte[])seal.Clone();
                changed[index] ^= 1;
                Case("changed seal byte " + index, "demo_mod", Fingerprint, changed, 0);
            }

            for (int index = 0; index < 16; index++)
            {
                Mutate("EncodedStamp", index, 1, () =>
                {
                    Case("changed stamp invalidates old identity", "demo_mod", Fingerprint, seal, 0);
                    Case("changed stamp with matching identity", "demo_mod", Fingerprint,
                        DisclosureIntegrity.CreateSessionSeal("demo_mod", Fingerprint), 1);
                });
                Mutate("EncodedStamp", index, 128, () =>
                    Case("high stamp byte uses ASCII replacement", "demo_mod", Fingerprint,
                        DisclosureIntegrity.CreateSessionSeal("demo_mod", Fingerprint), 1));
            }
            for (int index = 0; index < 32; index++)
                Mutate("ExpectedStampDigest", index, 1, () =>
                    Case("changed expected digest", "demo_mod", Fingerprint, seal, 1));
            foreach (string key in new[] { "KeyPartA", "KeyPartB" })
                for (int index = 0; index < 16; index++)
                    Mutate(key, index, 1, () =>
                    {
                        Case("changed key invalidates old identity", "demo_mod", Fingerprint, seal, 0);
                        Case("changed key with matching identity", "demo_mod", Fingerprint,
                            DisclosureIntegrity.CreateSessionSeal("demo_mod", Fingerprint), 3);
                    });

            // The native implementation must read the owning C# arrays on each
            // call. A consistent replacement is valid under the original rules.
            byte[] digest = Field("ExpectedStampDigest");
            byte[] savedDigest = (byte[])digest.Clone();
            try
            {
                Mutate("EncodedStamp", 0, 128, () =>
                {
                    byte[] expected;
                    using (var sha = SHA256.Create())
                        expected = sha.ComputeHash(Encoding.ASCII.GetBytes(DisclosureIntegrity.FixedStamp()));
                    Buffer.BlockCopy(expected, 0, digest, 0, digest.Length);
                    Case("matching changed stamp and digest", "demo_mod", Fingerprint,
                        DisclosureIntegrity.CreateSessionSeal("demo_mod", Fingerprint), 3);
                });
            }
            finally { Buffer.BlockCopy(savedDigest, 0, digest, 0, digest.Length); }
            Check(DisclosureIntegrity.IsFixedStampIntact(), "test must restore private arrays");
            Console.WriteLine("PASS integrity differential: " + _cases + " cases; actual native="
                + _native + (_native ? "; " + NativeIntegrity.LoadedPath
                    : "; managed compatibility only (native cases skipped)"));
        }

        private static byte[] Field(string name)
        {
            return (byte[])typeof(DisclosureIntegrity).GetField(name,
                BindingFlags.Static | BindingFlags.NonPublic).GetValue(null);
        }

        private static void Mutate(string name, int index, byte mask, Action test)
        {
            byte[] bytes = Field(name);
            byte old = bytes[index];
            try { bytes[index] ^= mask; test(); }
            finally { bytes[index] = old; }
        }

        private static void Case(string name, string mod, string fingerprint, byte[] seal, uint expected)
        {
            uint original = !DisclosureIntegrity.VerifySessionSeal(mod, fingerprint, seal) ? 0u
                : DisclosureIntegrity.IsFixedStampIntact() ? 3u : 1u;
            Check(original == expected, name + ": unexpected original managed result");
            Check(DisclosureIntegrity.VerifySessionIntegrityManaged(mod, fingerprint, seal) == original,
                name + ": managed combined entry changed original semantics");
            Check(DisclosureIntegrity.VerifySessionIntegrity(mod, fingerprint, seal) == original,
                name + ": combined backend differs from original methods");
            if (_native)
            {
                uint actual;
                Check(NativeIntegrity.TryVerify(mod, fingerprint, seal, Field("EncodedStamp"),
                    Field("ExpectedStampDigest"), Field("KeyPartA"), Field("KeyPartB"), out actual),
                    name + ": native silently fell back");
                Check(actual == original, name + ": actual native status differs from original methods");
            }
            _cases++;
        }

        private static void Check(bool condition, string message)
        {
            if (!condition) throw new Exception(message);
        }
    }
}
