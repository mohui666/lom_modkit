using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using System.Threading;

namespace MortalModHost
{
    internal static unsafe class NativeTextScan
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int CountCallback(char* text, int length, byte* bmp, byte* letters,
            uint* rangeEnds, byte* rangeFlags, int ranges);
        private static readonly CountCallback CountNative = NativeHostKernel.Bind<CountCallback>("lom_text_count_v1");
        private static readonly Lazy<Tables> Unicode = new Lazy<Tables>(() => new Tables(), LazyThreadSafetyMode.ExecutionAndPublication);
        internal static bool IsAvailable { get { return CountNative != null; } }
        internal static int RetainedTableBytes { get { var tables = Unicode.Value; return tables.Bmp.Length + tables.Letters.Length + tables.Ends.Length * 4 + tables.Flags.Length; } }

        internal static int Count(string text)
        {
            if (string.IsNullOrEmpty(text)) return 0;
            if (CountNative == null) return CountManaged(text);
            var tables = Unicode.Value;
            int count;
            fixed (char* input = text)
            fixed (byte* bmp = tables.Bmp)
            fixed (byte* letters = tables.Letters)
            fixed (uint* ends = tables.Ends)
            fixed (byte* flags = tables.Flags)
                count = CountNative(input, text.Length, bmp, letters, ends, flags, tables.Ends.Length);
            if (count < 0 || count > text.Length) throw new InvalidOperationException("原生对白文本扫描拒绝输入或返回无效计数：" + count);
            return count;
        }

        internal static int CountManaged(string text) { return AutoDialogueTiming.CountCharactersManaged(text); }

        private sealed class Tables
        {
            internal readonly byte[] Bmp = new byte[65536], Letters = new byte[65536];
            internal readonly uint[] Ends;
            internal readonly byte[] Flags;
            internal Tables()
            {
                const string tagLetters = "abcdefghijklmnopqrstuvwxyz";
                var patterns = new Regex[26];
                for (int i = 0; i < patterns.Length; i++)
                    patterns[i] = new Regex("^" + tagLetters[i] + "$", RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
                for (int i = 0; i < Bmp.Length; i++)
                {
                    char unit = (char)i;
                    Bmp[i] = CategoryFlags(CharUnicodeInfo.GetUnicodeCategory(unit));
                    if (char.IsWhiteSpace(unit)) Bmp[i] |= 4;
                    // A few ASCII case equivalents are verified against the
                    // real regex engine, without allocating 65K test strings.
                    char lower = char.ToLowerInvariant(unit), upper = char.ToUpperInvariant(unit);
                    int letter = lower >= 'a' && lower <= 'z' ? lower - 'a'
                        : upper >= 'A' && upper <= 'Z' ? upper - 'A' : -1;
                    if (letter >= 0 && patterns[letter].IsMatch(new string(unit, 1))) Letters[i] = (byte)tagLetters[letter];
                }
                // Batch supplementary category queries through public BCL APIs.
                // Reuse a 16 KiB character block; never allocate one string/CP.
                var chars = new char[8192];
                var ends = new List<uint>();
                var flags = new List<byte>();
                byte previous = 255;
                for (int block = 0x10000; block < 0x110000; block += 4096)
                {
                    int count = Math.Min(4096, 0x110000 - block);
                    for (int n = 0; n < count; n++)
                    {
                        int scalar = block + n - 0x10000;
                        chars[n * 2] = (char)(0xD800 + (scalar >> 10));
                        chars[n * 2 + 1] = (char)(0xDC00 + (scalar & 1023));
                    }
                    string batch = new string(chars, 0, count * 2);
                    for (int n = 0; n < count; n++)
                    {
                        byte value = CategoryFlags(CharUnicodeInfo.GetUnicodeCategory(batch, n * 2));
                        if (value != previous)
                        {
                            if (previous != 255) { ends.Add((uint)(block + n - 1)); flags.Add(previous); }
                            previous = value;
                        }
                    }
                }
                ends.Add(0x10FFFF); flags.Add(previous);
                Ends = ends.ToArray(); Flags = flags.ToArray();
            }
        }
        private static byte CategoryFlags(UnicodeCategory category)
        {
            if (category == UnicodeCategory.NonSpacingMark || category == UnicodeCategory.SpacingCombiningMark
                || category == UnicodeCategory.EnclosingMark) return 1;
            return category == UnicodeCategory.Format || category == UnicodeCategory.Control
                || category == UnicodeCategory.OtherNotAssigned || category == UnicodeCategory.Surrogate ? (byte)0 : (byte)2;
        }
    }
}
