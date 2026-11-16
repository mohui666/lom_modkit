using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static class NativeAudioMixTests
    {
        private static readonly bool Mono = Type.GetType("Mono.Runtime") != null;
        private static int _cases;
        private static readonly int[] SpecialBits = { 0, unchecked((int)0x80000000), 0x7F800000,
            unchecked((int)0xFF800000), 0x7FC34567, unchecked((int)0xFFC45678), 0x7FA34567,
            unchecked((int)0xFFA45678), 1, unchecked((int)0x80000001) };
        internal static void Run(bool requireNative)
        {
            bool native = NativeAudioMix.IsAvailable;
            if (requireNative && !native) throw new Exception("Audio mix native backend required: " + NativeHostKernel.UnavailableReason);
            Check(Marshal.SizeOf(typeof(NativeAudioMix.State)) == 32, "audio state ABI size");
            _cases = 0;
            foreach (int channels in new[] { 1, 2 })
            foreach (int rate in new[] { 44100, 48000, 96000 })
            foreach (int shape in new[] { 0, 1, 2 })
            foreach (int looping in new[] { 0, 1, 2 })
                Case(channels, rate, 8197, shape, looping, false, false, false, native);
            foreach (int channels in new[] { 1, 2 })
            foreach (int rate in new[] { 44100, 48000, 96000 })
            foreach (int frames in new[] { 0, 1, 2 })
            foreach (int looping in new[] { 0, 2 })
                Case(channels, rate, frames, 1, looping, false, false, false, native);
            Case(2, 96000, 31, 2, 0, false, true, false, native);
            Case(1, 44100, 31, 1, 0, false, false, true, native);
            // Special-value expectations come from the actual installed Unity
            // Mono original loop; CoreCLR's float evaluation is not that oracle.
            if (Mono) foreach (int channels in new[] { 1, 2 })
                foreach (int rate in new[] { 44100, 48000, 96000 })
                    Case(channels, rate, 8197, 1, 2, true, false, false, native);
            Console.WriteLine("PASS native audio mix: " + _cases + " streaming cases; native=" + native
                + "; reference=" + (Mono ? "actual original Unity Mono loop + special floats" : native ? "explicit frozen Mono arithmetic oracle" : "original managed fallback"));
        }

        private static void Case(int channels, int rate, int frames, int shape, int looping,
            bool special, bool throwRead, bool throwDispose, bool native)
        {
            var expectedSource = new Source(channels, rate, frames, shape, looping != 1);
            var actualSource = new Source(channels, rate, frames, shape, looping != 1);
            var expected = new ReferenceChannel(expectedSource, looping != 0, .7f, native && !Mono);
            var actual = new VideoAudioMixer.Channel(actualSource, looping != 0, .7f);
            expectedSource.Owner = expected; actualSource.Owner = actual;
            expectedSource.ThrowRead = actualSource.ThrowRead = throwRead ? 3 : -1;
            expectedSource.ThrowDispose = actualSource.ThrowDispose = throwDispose;
            expectedSource.ChangeVolume = actualSource.ChangeVolume = !special;
            string name = channels + "ch " + rate + " frames=" + frames + " shape=" + shape + " loop=" + looping + " special=" + special;
            Compare(expected, actual, expectedSource, actualSource, name + " ctor");
            int[] lengths = { 0, 1, 2, 7, 34, 1025, 8193, 17, 3200, 12000, 4097 };
            float[] volumes = special ? new[] { 0f, FromBits(unchecked((int)0x80000000)), float.PositiveInfinity,
                float.NegativeInfinity, FromBits(unchecked((int)0x7FC12345)), FromBits(unchecked((int)0xFFC23456)),
                FromBits(0x7FA12345), FromBits(unchecked((int)0xFFA23456)), 1f }
                : new[] { .7f, 0f, 1f, -.75f, .125f, 2f, 1f };
            for (int block = 0; block < lengths.Length; ++block)
            {
                expected.Volume = actual.Volume = volumes[block % volumes.Length];
                var left = new float[lengths[block]];
                for (int i = 0; i < left.Length; ++i)
                    left[i] = special ? FromBits(SpecialBits[i % SpecialBits.Length])
                        : ((i * 37) % 127 - 63) / 19f;
                var right = (float[])left.Clone();
                Exception e1 = Catch(() => expected.Mix(left)), e2 = Catch(() => actual.Mix(right));
                Check((e1 == null ? null : e1.GetType()) == (e2 == null ? null : e2.GetType()), name + " exception order block=" + block);
                for (int i = 0; i < left.Length; ++i)
                    if (Bits(left[i]) != Bits(right[i]))
                        throw new Exception(name + " output block=" + block + " sample=" + i
                            + " expected=" + Bits(left[i]).ToString("X8") + " actual=" + Bits(right[i]).ToString("X8"));
                Compare(expected, actual, expectedSource, actualSource, name + " block=" + block);
            }
            Exception d1 = Catch(expected.Dispose), d2 = Catch(actual.Dispose);
            Check((d1 == null ? null : d1.GetType()) == (d2 == null ? null : d2.GetType()), name + " explicit dispose");
            expected.Dispose(); actual.Dispose();
            Compare(expected, actual, expectedSource, actualSource, name + " disposed");
            _cases++;
        }

        private static void Compare(object expected, object actual, Source left, Source right, string message)
        {
            string expectedState = Snapshot(expected), actualState = Snapshot(actual);
            if (expectedState != actualState) throw new Exception(message + " channel state\n" + expectedState + "\n" + actualState);
            if (!left.Trace.SequenceEqual(right.Trace)) throw new Exception(message + " Read/Rewind/Dispose callback order\n"
                + string.Join("\n", left.Trace.Where((line, index) => index >= right.Trace.Count || line != right.Trace[index]).Take(2).ToArray())
                + "\n" + string.Join("\n", right.Trace.Where((line, index) => index >= left.Trace.Count || line != left.Trace[index]).Take(2).ToArray()));
        }
        private static Exception Catch(Action action) { try { action(); return null; } catch (Exception error) { return error; } }
        private static string Snapshot(object channel)
        {
            Type type = channel.GetType();
            var values = new List<string>();
            foreach (string name in new[] { "_at", "_end", "_phase", "_left", "_right", "_nextLeft", "_nextRight", "_next", "_disposed", "Volume" })
            {
                object value = type.GetField(name, BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic).GetValue(channel);
                values.Add(name + "=" + (value is float ? Bits((float)value).ToString("X8") : value is double ? BitConverter.DoubleToInt64Bits((double)value).ToString("X16") : value.ToString()));
            }
            values.Add("playing=" + type.GetProperty("IsPlaying", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic).GetValue(channel, null));
            return string.Join(";", values.ToArray());
        }
        private static void Check(bool value, string message) { if (!value) throw new Exception(message); }
        private static unsafe int Bits(float value) { return *(int*)&value; }
        private static unsafe float FromBits(int value) { return *(float*)&value; }

        // For the parent's Mono timing harness. Returns [elapsed milliseconds,
        // checksum, ReadPcm16 calls, Rewind calls]. No reflection/tracing in loop.
        internal static double[] Benchmark(bool native, int iterations)
        {
            if (native && !NativeAudioMix.IsAvailable) throw new Exception("native audio benchmark backend missing");
            var source = new Source(2, 44100, 8197, 0, true) { Record = false };
            var output = new float[8192];
            var watch = new Stopwatch();
            uint sum = 0;
            if (native)
            {
                using (var channel = new VideoAudioMixer.Channel(source, true, .43f))
                {
                    watch.Start();
                    for (int i = 0; i < iterations; ++i) { Array.Clear(output, 0, output.Length); channel.Mix(output); sum = unchecked(sum * 16777619u ^ (uint)Bits(output[i % output.Length])); }
                    watch.Stop();
                }
            }
            else
            {
                using (var channel = new ReferenceChannel(source, true, .43f, false))
                {
                    watch.Start();
                    for (int i = 0; i < iterations; ++i) { Array.Clear(output, 0, output.Length); channel.Mix(output); sum = unchecked(sum * 16777619u ^ (uint)Bits(output[i % output.Length])); }
                    watch.Stop();
                }
            }
            return new[] { watch.Elapsed.TotalMilliseconds, (double)sum, source.Reads, source.Rewinds };
        }

        private sealed class Source : NativePcmPlayer.ISampleSource
        {
            readonly byte[] data;
            readonly int shape;
            int at;
            internal object Owner;
            internal bool Record = true, ChangeVolume, ThrowDispose;
            internal int ThrowRead = -1, Reads, Rewinds;
            internal readonly List<string> Trace = new List<string>();
            public int Channels { get; private set; }
            public int SampleRate { get; private set; }
            public bool CanSeek { get; private set; }
            internal Source(int channels, int rate, int frames, int shape, bool seek)
            {
                Channels = channels; SampleRate = rate; CanSeek = seek; this.shape = shape;
                data = new byte[frames * channels * 2];
                for (int i = 0; i < data.Length / 2; ++i) { short sample = unchecked((short)(i * 7919 + channels * 31)); data[i * 2] = (byte)sample; data[i * 2 + 1] = (byte)(sample >> 8); }
            }
            void Observe(string action) { if (Record) Trace.Add(action + " pos=" + at + " " + (Owner == null ? "ctor" : Snapshot(Owner))); }
            public int ReadPcm16(byte[] dest, int offset, int count)
            {
                Reads++; Observe("read" + Reads);
                if (Reads == ThrowRead) throw new IOException("fixture read failure");
                if (ChangeVolume && Owner != null && Reads % 3 == 0) Owner.GetType().GetField("Volume", BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public).SetValue(Owner, Reads % 2 == 0 ? -.4f : .3f);
                int limit = shape == 0 ? count : shape == 1 ? new[] { 1, 5, 2, 17, 63, 7, 4095 }[(Reads - 1) % 7] : new[] { 3, 9, 0, 31, 4, 2 }[(Reads - 1) % 6];
                int n = Math.Min(Math.Min(count, limit), data.Length - at);
                Buffer.BlockCopy(data, at, dest, offset, n); at += n; return n;
            }
            public void Rewind() { Rewinds++; Observe("rewind" + Rewinds); at = 0; }
            public void Dispose() { Observe("dispose"); if (ThrowDispose) { ThrowDispose = false; throw new IOException("fixture dispose failure"); } }
        }

        // Frozen pre-migration Channel. Only CoreCLR native tests opt into the
        // explicitly widened oracle validated against the actual Mono runner.
        private sealed class ReferenceChannel : IDisposable
        {
            private readonly NativePcmPlayer.ISampleSource _source;
            private readonly bool _loop, _oracle;
            private readonly byte[] _buffer;
            private int _at, _end;
            private readonly double _step;
            private double _phase;
            private float _left, _right, _nextLeft, _nextRight;
            private bool _next, _disposed;
            internal bool IsPlaying { get; private set; }
            internal float Volume;
            internal ReferenceChannel(NativePcmPlayer.ISampleSource source, bool loop, float volume, bool oracle)
            {
                _source = source; _loop = loop; Volume = volume; _oracle = oracle;
                if (source.Channels < 1 || source.Channels > 2) { source.Dispose(); throw new NotSupportedException(); }
                _buffer = new byte[4096 * source.Channels * 2]; _step = (double)source.SampleRate / VideoAudioMixer.Rate;
                IsPlaying = Read(out _left, out _right); if (IsPlaying) _next = Read(out _nextLeft, out _nextRight);
            }
            private bool Read(out float left, out float right)
            {
                left = right = 0;
                if (_at >= _end)
                {
                    _at = 0; _end = _source.ReadPcm16(_buffer, 0, _buffer.Length);
                    if (_end == 0 && _loop && _source.CanSeek) { _source.Rewind(); _end = _source.ReadPcm16(_buffer, 0, _buffer.Length); }
                    if (_end == 0) return false;
                }
                left = (short)(_buffer[_at] | _buffer[_at + 1] << 8) / 32768f; _at += 2;
                if (_source.Channels == 1) right = left;
                else { right = (short)(_buffer[_at] | _buffer[_at + 1] << 8) / 32768f; _at += 2; }
                return true;
            }
            internal void Mix(float[] output)
            {
                if (!IsPlaying) return;
                for (int frame = 0; frame < output.Length / 2 && IsPlaying; ++frame)
                {
                    float fraction = (float)_phase;
                    if (_oracle)
                    {
                        output[frame * 2] = (float)((double)output[frame * 2] + ((double)_left + ((double)(_next ? _nextLeft : _left) - _left) * fraction) * Volume);
                        output[frame * 2 + 1] = (float)((double)output[frame * 2 + 1] + ((double)_right + ((double)(_next ? _nextRight : _right) - _right) * fraction) * Volume);
                    }
                    else
                    {
                        output[frame * 2] += (_left + ((_next ? _nextLeft : _left) - _left) * fraction) * Volume;
                        output[frame * 2 + 1] += (_right + ((_next ? _nextRight : _right) - _right) * fraction) * Volume;
                    }
                    _phase += _step;
                    while (_phase >= 1 && IsPlaying)
                    {
                        _phase -= 1;
                        if (!_next) { Dispose(); break; }
                        _left = _nextLeft; _right = _nextRight; _next = Read(out _nextLeft, out _nextRight);
                    }
                }
            }
            public void Dispose() { IsPlaying = false; if (_disposed) return; _disposed = true; _source.Dispose(); }
        }
    }
}
