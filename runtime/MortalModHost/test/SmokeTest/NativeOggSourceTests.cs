using System;
using System.IO;
using System.Runtime.InteropServices;
using NVorbis;

namespace MortalModHost
{
    internal static unsafe class NativeOggSourceTests
    {
        // Self-generated one-frame 8 kHz libvorbis fixture. It is deliberately
        // shorter than one codec block to catch untrimmed end padding.
        private const string OneFrameOgg = "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";
        private const string OneFramePcm = "/n8=";

        internal static void Run(bool requireNative)
        {
            bool native = NativeOggSource.IsAvailable;
            if (requireNative && !native) throw new Exception("OGG decoder requires actual native: " + NativeHostKernel.UnavailableReason);
            byte[] encoded = Convert.FromBase64String(OneFrameOgg), expected = Convert.FromBase64String(OneFramePcm);
            using (NativePcmPlayer.ISampleSource source = NativePcmPlayer.OpenSource(encoded, ".ogg"))
            {
                Check(!native || source is NativeOggSource, "available native OGG must be the selected decoder");
                if (native) Array.Clear(encoded, 0, encoded.Length); // The handle owns its own copy.
                Check(source.Channels == 1 && source.SampleRate == 8000 && source.CanSeek, "one-frame OGG metadata");
                for (int loop = 0; loop < 30; loop++)
                {
                    var destination = new byte[] { 0x5a, 0x5a, 0x5a, 0x5a, 0x5a };
                    Check(source.ReadPcm16(destination, 1, 1) == 0, "partial frame must not advance decoder");
                    Check(source.ReadPcm16(destination, 1, 3) == 2, "short OGG must remain exactly one frame");
                    Check(destination[0] == 0x5a && destination[3] == 0x5a && destination[4] == 0x5a, "OGG output bounds");
                    Check(Math.Abs(Sample(destination, 1) - Sample(expected, 0)) <= 1, "OGG matches frozen Unity Mono within 1 PCM unit");
                    Check(source.ReadPcm16(destination, 0, 4) == 0, "codec padding must not leak after EOF");
                    source.Rewind();
                }
                source.Dispose(); source.Dispose();
                bool closed = false;
                try { source.ReadPcm16(new byte[4], 0, 4); } catch (ObjectDisposedException) { closed = true; }
                catch (NullReferenceException) { if (!native) closed = true; else throw; }
                Check(closed, "read after OGG dispose must fail");
            }
            if (native)
            {
                bool rejected = false;
                try { using (var source = NativePcmPlayer.OpenSource(new byte[] { 1, 2, 3 }, ".ogg")) { } }
                catch (InvalidDataException error) { rejected = error.Message.Contains("原生状态"); }
                Check(rejected, "native OGG rejection must not fall back to NVorbis");
            }
            Console.WriteLine("PASS OGG source: metadata, exact short duration, 30 rewinds, ownership, bounds, dispose; actual native=" + native);
        }

        internal static void RunDirectory(string inputs, string references)
        {
            Check(NativeOggSource.IsAvailable, "matrix requires actual native OGG");
            foreach (string input in Directory.GetFiles(inputs, "*.ogg"))
            {
                string name = Path.GetFileNameWithoutExtension(input);
                string reference = Path.Combine(references, name + ".0.pcm");
                byte[] encoded = File.ReadAllBytes(input);
                if (!File.Exists(reference))
                {
                    // The reference decoder rejected this input before samples.
                    // Report support differences explicitly rather than silently
                    // calling it a numerical/duration comparison.
                    try
                    {
                        using (var source = new NativeOggSource(encoded))
                        { Console.WriteLine("OGG support difference: " + name + "; old NVorbis rejected, native opens " + source.Channels + "ch"); }
                    }
                    catch (InvalidDataException) { Console.WriteLine("OGG rejected by both: " + name); }
                    continue;
                }
                byte[] expected = File.ReadAllBytes(reference);
                NativeOggSource decoder;
                try { decoder = new NativeOggSource(encoded); }
                catch (InvalidDataException)
                {
                    if (expected.Length != 0) throw new Exception("native rejected playable old prefix: " + name);
                    Console.WriteLine("OGG support difference: " + name + "; old emitted 0 frames, native rejects at open");
                    continue;
                }
                using (decoder)
                {
                    using (var old = new VorbisReader(new MemoryStream(encoded, false), true))
                        Check(decoder.Channels == old.Channels && decoder.SampleRate == old.SampleRate, name + ": metadata differs");
                    byte[] first = null;
                    int maxDelta = 0;
                    for (int loop = 0; loop < 3; loop++)
                    {
                        byte[] actual;
                        using (var output = new MemoryStream())
                        {
                            int call = 0;
                            int[] sizes = { 1, 7, 4096, 17, 1023 };
                            while (true)
                            {
                                int capacity = sizes[call++ % sizes.Length] * decoder.Channels * 2;
                                var buffer = new byte[capacity + 8];
                                for (int i = 0; i < buffer.Length; i++) buffer[i] = 0x5a;
                                int got = decoder.ReadPcm16(buffer, 3, capacity + 1);
                                Check(got >= 0 && got <= capacity && got % (decoder.Channels * 2) == 0, name + ": invalid frame count");
                                for (int i = 0; i < 3; i++) Check(buffer[i] == 0x5a, name + ": output prefix changed");
                                for (int i = 3 + got; i < buffer.Length; i++) Check(buffer[i] == 0x5a, name + ": output suffix changed");
                                if (got == 0) break;
                                output.Write(buffer, 3, got);
                            }
                            actual = output.ToArray();
                        }
                        Check(actual.Length == expected.Length, name + ": duration differs; old bytes=" + expected.Length + " native=" + actual.Length);
                        for (int i = 0; i < actual.Length; i += 2)
                        {
                            int delta = Math.Abs(Sample(actual, i) - Sample(expected, i));
                            maxDelta = Math.Max(maxDelta, delta);
                            Check(delta <= 1, name + ": PCM differs by " + delta + " at sample " + (i / 2));
                            if (first != null) Check(actual[i] == first[i] && actual[i + 1] == first[i + 1], name + ": rewind changed decoder output");
                        }
                        if (first == null) first = actual;
                        decoder.Rewind();
                    }
                    Console.WriteLine("PASS OGG matrix: " + name + "; channels=" + decoder.Channels + "; rate=" + decoder.SampleRate
                        + "; samples=" + (expected.Length / 2) + "; cycles=3; PCM max_delta=" + maxDelta + " (not bit-equivalent)");
                }
            }
        }

        private static int Sample(byte[] bytes, int offset) { return (short)(bytes[offset] | bytes[offset + 1] << 8); }
        private static void Check(bool value, string message) { if (!value) throw new Exception(message); }
    }
}
