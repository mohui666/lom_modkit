using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativePcmConvertTests
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int FloatCallback(float* source, int count, byte* destination, int capacity);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int WavCallback(byte* source, int length, int width, byte* destination, int capacity);
        private static int _cases;
        private static long _samples;
        private static bool _native, _mono;

        internal static void Run(bool requireNative)
        {
            _cases = 0;
            _samples = 0;
            _native = NativePcmConvert.IsAvailable;
            _mono = Type.GetType("Mono.Runtime") != null;
            if (requireNative && !_native)
                throw new Exception("PCM differential requires actual native: " + NativeHostKernel.UnavailableReason);
            var special = new List<float>();
            foreach (uint bits in new uint[] { 0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x807fffff,
                0x7fc00000, 0xffc00000, 0x7f800001, 0xff800001, 0x7fffffff, 0xffffffff,
                0x7f800000, 0xff800000, 0x7f7fffff, 0xff7fffff, 0x3f800000, 0xbf800000,
                0x3f7fffff, 0xbf7fffff, 0x3f800001, 0xbf800001, 0x3f000000, 0xbf000000 })
                special.Add(FromBits(bits));
            FloatCase("special float bit patterns", special.ToArray(), special.Count);
            FloatCase("empty float input", new float[0], 0);
            FloatCase("prefix of decoder scratch", special.ToArray(), 7);

            var thresholds = new List<float>();
            for (int value = -32767; value <= 32767; value++)
            {
                float sample = (float)(value / 32767.0);
                uint bits = BitConverter.ToUInt32(BitConverter.GetBytes(sample), 0);
                thresholds.Add(FromBits(unchecked(bits - 1)));
                thresholds.Add(sample);
                thresholds.Add(FromBits(unchecked(bits + 1)));
            }
            FloatCase("all integer quantization thresholds and adjacent float bits", thresholds.ToArray(), thresholds.Count);

            var random = new float[65536];
            uint state = 0x12345678;
            for (int i = 0; i < random.Length; i++)
            {
                state = unchecked(state * 1664525u + 1013904223u);
                random[i] = FromBits(state);
            }
            FloatCase("deterministic complete-range float bit patterns", random, random.Length);
            WavCase("WAV32 keeps float interpretation for tags 1 and 3", FloatBytes(random), 4);
            WavCase("WAV32 special floats, unaligned source", FloatBytes(special.ToArray()), 4);
            var pcm8 = new byte[256];
            for (int i = 0; i < pcm8.Length; i++) pcm8[i] = (byte)i;
            WavCase("every unsigned PCM8 input", pcm8, 1);
            var pcm16 = new byte[65536 * 2];
            for (int i = 0; i < 65536; i++) { pcm16[i * 2] = (byte)i; pcm16[i * 2 + 1] = (byte)(i >> 8); }
            WavCase("every PCM16 bit pattern is copied unchanged", pcm16, 2);
            foreach (int width in new[] { 1, 2, 4 }) WavCase("empty WAV span", new byte[0], width);
            OverlapPcm16(0, 2);
            OverlapPcm16(2, 0);
            WavFramesAndCursors();
            WrapperRejectsInvalidSpans();
            if (_native) NativeRejectionsDoNotWrite();
            Console.WriteLine("PASS PCM differential: " + _cases + " cases; " + _samples
                + " samples; actual native=" + _native
                + "; original Unity Mono differential=" + (_native && _mono)
                + "; native oracle=Unity Mono double intermediate");
        }

        private static float FromBits(uint bits) { return BitConverter.ToSingle(BitConverter.GetBytes(bits), 0); }
        private static byte[] FloatBytes(float[] samples)
        {
            var bytes = new byte[samples.Length * 4];
            Buffer.BlockCopy(samples, 0, bytes, 0, bytes.Length);
            return bytes;
        }
        private static byte[] Sentinel(int size)
        {
            var bytes = new byte[size];
            for (int i = 0; i < bytes.Length; i++) bytes[i] = 0x5a;
            return bytes;
        }

        private static void FloatCase(string name, float[] samples, int count)
        {
            byte[] originalSource = FloatBytes(samples);
            byte[] expected = Sentinel(count * 2 + 11), monoExpected = (byte[])expected.Clone(),
                managed = (byte[])expected.Clone(), actual = (byte[])expected.Clone();
            // Independent copy of the old OggSource loop, run by the actual
            // runtime under test (including unchecked conv.i2 for NaNs).
            for (int i = 0; i < count; i++)
            {
                float f = samples[i];
                if (f > 1f) f = 1f;
                if (f < -1f) f = -1f;
                short s = (short)(f * 32767f);
                expected[5 + i * 2] = (byte)(s & 0xff);
                expected[5 + i * 2 + 1] = (byte)((s >> 8) & 0xff);
                WriteMonoSample(monoExpected, 5 + i * 2, samples[i]);
            }
            Check(NativePcmConvert.Float32ToPcm16Managed(samples, count, managed, 5) == count * 2, name + ": managed size");
            Check(NativePcmConvert.Float32ToPcm16(samples, count, actual, 5) == count * 2, name + ": backend size");
            Equal(expected, managed, name + ": managed reference");
            if (_mono) Equal(expected, monoExpected, name + ": actual original Mono versus independent oracle");
            Equal(_native ? monoExpected : expected, actual, name + ": backend reference and output guards");
            Equal(originalSource, FloatBytes(samples), name + ": input must not change");
            _cases++;
            _samples += count;
        }

        private static void WavCase(string name, byte[] payload, int width)
        {
            var source = Sentinel(payload.Length + 8);
            Buffer.BlockCopy(payload, 0, source, 3, payload.Length);
            var originalSource = (byte[])source.Clone();
            int count = payload.Length / width;
            byte[] expected = Sentinel(count * 2 + 11), monoExpected = (byte[])expected.Clone(),
                managed = (byte[])expected.Clone(), actual = (byte[])expected.Clone();
            if (width == 2)
            {
                Buffer.BlockCopy(source, 3, expected, 5, payload.Length);
                Buffer.BlockCopy(source, 3, monoExpected, 5, payload.Length);
            }
            else
            {
                int src = 3, written = 0;
                // Independent original WavSource sample loop. PCM32 tag 1 was
                // already read as float, which this migration must preserve.
                for (int i = 0; i < count; i++)
                {
                    float sample;
                    if (width == 1) sample = (source[src++] - 128) / 128f;
                    else { sample = BitConverter.ToSingle(source, src); src += 4; }
                    if (sample > 1f) sample = 1f;
                    if (sample < -1f) sample = -1f;
                    short s = (short)(sample * 32767f);
                    expected[5 + written] = (byte)(s & 0xff);
                    expected[5 + written + 1] = (byte)((s >> 8) & 0xff);
                    WriteMonoSample(monoExpected, 5 + written, sample);
                    written += 2;
                }
            }
            Check(NativePcmConvert.WavToPcm16Managed(source, 3, count, width, managed, 5) == count * 2, name + ": managed size");
            Check(NativePcmConvert.WavToPcm16(source, 3, count, width, actual, 5) == count * 2, name + ": backend size");
            Equal(expected, managed, name + ": managed reference");
            if (_mono) Equal(expected, monoExpected, name + ": actual original Mono versus independent oracle");
            Equal(_native ? monoExpected : expected, actual, name + ": backend reference and output guards");
            Equal(originalSource, source, name + ": encoded source must not change");
            _cases++;
            _samples += count;
        }

        private static void OverlapPcm16(int source, int destination)
        {
            var actual = new byte[] { 1, 2, 3, 4, 5, 6, 7, 8 };
            var expected = (byte[])actual.Clone();
            Buffer.BlockCopy(expected, source, expected, destination, 6);
            Check(NativePcmConvert.WavToPcm16(actual, source, 3, 2, actual, destination) == 6, "overlapping PCM16 size");
            Equal(expected, actual, "overlapping PCM16 retains BlockCopy");
            _cases++;
        }

        // .NET 8's local managed reference can differ from Unity Mono at float
        // multiplication boundaries. Keep both strict oracles, and additionally
        // require their complete byte equality in the actual Unity Mono run.
        private static void WriteMonoSample(byte[] destination, int offset, float sample)
        {
            if (sample > 1f) sample = 1f;
            if (sample < -1f) sample = -1f;
            short value = float.IsNaN(sample) ? (short)0 : (short)((double)sample * 32767.0);
            destination[offset] = (byte)(value & 0xff);
            destination[offset + 1] = (byte)((value >> 8) & 0xff);
        }

        private static void WavFramesAndCursors()
        {
            foreach (int width in new[] { 1, 2, 4 })
            foreach (int tag in width == 4 ? new[] { 1, 3 } : new[] { 1 })
            {
                byte[] payload;
                if (width == 1) payload = new byte[] { 0, 1, 127, 128, 254, 255, 64 };
                else if (width == 2) payload = new byte[] { 0, 0, 0, 128, 255, 127, 255, 255, 1, 0, 52, 18, 247, 255, 42 };
                else
                {
                    byte[] floats = FloatBytes(new[] { FromBits(0xbf7fbfff), float.NaN,
                        float.PositiveInfinity, float.NegativeInfinity, 0.5f, -0.5f, 0.75f });
                    payload = new byte[floats.Length + 3];
                    Buffer.BlockCopy(floats, 0, payload, 0, floats.Length);
                }
                byte[] encoded = Wave(payload, width, tag), original = (byte[])encoded.Clone();
                byte[] expected = Sentinel(20);
                for (int i = 0; i < 6; i++)
                {
                    if (width == 2) { expected[3 + i * 2] = payload[i * 2]; expected[4 + i * 2] = payload[i * 2 + 1]; }
                    else
                    {
                        float sample = width == 1 ? (payload[i] - 128) / 128f : BitConverter.ToSingle(payload, i * 4);
                        if (_native) WriteMonoSample(expected, 3 + i * 2, sample);
                        else
                        {
                            if (sample > 1f) sample = 1f;
                            if (sample < -1f) sample = -1f;
                            short s = (short)(sample * 32767f);
                            expected[3 + i * 2] = (byte)(s & 0xff);
                            expected[4 + i * 2] = (byte)((s >> 8) & 0xff);
                        }
                    }
                }
                using (NativePcmPlayer.ISampleSource source = NativePcmPlayer.OpenSource(encoded, ".wav"))
                {
                    Check(source.Channels == 2 && source.SampleRate == 8000 && source.CanSeek, "WAV source properties");
                    byte[] actual = Sentinel(20);
                    Check(source.ReadPcm16(actual, 3, 5) == 4, "WAV reads whole first frame only");
                    Check(source.ReadPcm16(actual, 7, 11) == 8, "WAV keeps cursor across the next two frames");
                    Equal(expected, actual, "WAV tag=" + tag + " width=" + width + " frame conversion");
                    Check(source.ReadPcm16(actual, 0, 16) == 0, "WAV ignores incomplete trailing channel/frame");
                    Equal(expected, actual, "WAV tail must not write");
                    source.Rewind();
                    byte[] replay = Sentinel(17), expectedReplay = Sentinel(17);
                    Buffer.BlockCopy(expected, 3, expectedReplay, 0, 12);
                    Check(source.ReadPcm16(replay, 0, 13) == 12, "WAV rewind and odd requested count");
                    Equal(expectedReplay, replay, "WAV replay must retain complete sample bytes");
                }
                Equal(original, encoded, "WAV encoded source stays unchanged");
                _cases++;
                _samples += 6;
            }
        }

        private static byte[] Wave(byte[] payload, int width, int tag)
        {
            using (var stream = new MemoryStream())
            using (var writer = new BinaryWriter(stream))
            {
                writer.Write(System.Text.Encoding.ASCII.GetBytes("RIFF")); writer.Write(36 + payload.Length + (payload.Length & 1));
                writer.Write(System.Text.Encoding.ASCII.GetBytes("WAVEfmt ")); writer.Write(16);
                writer.Write((ushort)tag); writer.Write((ushort)2); writer.Write(8000);
                writer.Write(8000 * 2 * width); writer.Write((ushort)(2 * width)); writer.Write((ushort)(width * 8));
                writer.Write(System.Text.Encoding.ASCII.GetBytes("data")); writer.Write(payload.Length); writer.Write(payload);
                if ((payload.Length & 1) != 0) writer.Write((byte)0);
                writer.Flush();
                return stream.ToArray();
            }
        }

        private static void WrapperRejectsInvalidSpans()
        {
            byte[] destination = Sentinel(16);
            foreach (Action invalid in new Action[] {
                () => NativePcmConvert.Float32ToPcm16(new float[1], 2, destination, 0),
                () => NativePcmConvert.Float32ToPcm16Managed(new float[1], -1, destination, 0),
                () => NativePcmConvert.Float32ToPcm16(new float[1], 1, destination, 15),
                () => NativePcmConvert.WavToPcm16(new byte[4], 0, 1, 3, destination, 0),
                () => NativePcmConvert.WavToPcm16(new byte[4], 1, 1, 4, destination, 0),
                () => NativePcmConvert.WavToPcm16Managed(new byte[4], -1, 1, 4, destination, 0),
            })
            {
                bool rejected = false;
                try { invalid(); } catch (ArgumentException) { rejected = true; }
                Check(rejected, "invalid managed span must fail before pinning");
                Equal(Sentinel(16), destination, "rejected managed span must not write");
                _cases++;
            }
        }

        private static void NativeRejectionsDoNotWrite()
        {
            FloatCallback floats = NativeHostKernel.Bind<FloatCallback>("lom_pcm_f32_to_i16_v1");
            WavCallback wav = NativeHostKernel.Bind<WavCallback>("lom_pcm_wav_to_i16_v1");
            float sample = 0.5f;
            byte[] output = Sentinel(16);
            fixed (byte* destination = output)
            {
                foreach (int count in new[] { -1, int.MaxValue, 1 })
                {
                    Check(floats(&sample, count, destination, count == 1 ? 1 : 16) == -1, "native float argument rejection");
                    _cases++;
                }
                Check(floats(null, 1, destination, 16) == -1, "native null float rejection");
                Check(floats(&sample, 1, null, 16) == -1, "native null output rejection");
                Check(floats(null, 0, null, 0) == 0, "native zero float span");
                foreach (int width in new[] { 0, 3, 8 })
                {
                    Check(wav((byte*)&sample, 4, width, destination, 16) == -1, "native unsupported sample width");
                    _cases++;
                }
                Check(wav((byte*)&sample, 3, 4, destination, 16) == -1, "native incomplete float sample");
                Check(wav((byte*)&sample, 4, 4, destination, 1) == -1, "native short destination");
                Check(wav(null, 4, 4, destination, 16) == -1, "native null WAV source");
                Check(wav(null, 0, 4, null, 0) == 0, "native empty WAV span");
            }
            Equal(Sentinel(16), output, "native rejection must not write or retry");
            _cases += 7;
        }

        private static void Equal(byte[] expected, byte[] actual, string message)
        {
            Check(expected.Length == actual.Length, message + ": length");
            for (int i = 0; i < expected.Length; i++)
                if (expected[i] != actual[i])
                    throw new Exception(message + ": byte " + i + " expected " + expected[i] + " actual " + actual[i]);
        }
        private static void Check(bool value, string message) { if (!value) throw new Exception(message); }
    }
}
