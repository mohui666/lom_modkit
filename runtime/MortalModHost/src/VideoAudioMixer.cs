using System;
using System.Collections.Generic;
using System.IO;

namespace MortalModHost
{
    // Custom package audio uses the same decoders and gains as waveOut playback.
    // During a video session its sample clock advances with each captured game frame.
    internal static class VideoAudioMixer
    {
        internal const int Rate = 48000;
        private static readonly List<Channel> Channels = new List<Channel>();
        private static BinaryWriter _writer;
        private static float[] _mixed;
        private static byte[] _bytes;
        private static long _samples;
        internal static bool Active { get { return _writer != null; } }

        internal static void Begin(string path, int fps)
        {
            if (Active) throw new InvalidOperationException("已有视频音轨正在生成");
            _mixed = new float[Rate / fps * 2];
            _bytes = new byte[_mixed.Length * 4];
            _samples = 0;
            _writer = new BinaryWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write));
            Header(_writer, 0);
        }

        internal static Channel Play(byte[] data, string extension, bool loop, float volume)
        {
            var channel = new Channel(NativePcmPlayer.OpenSource(data, extension), loop, volume);
            Channels.Add(channel);
            return channel;
        }

        internal static void Frame()
        {
            Array.Clear(_mixed, 0, _mixed.Length);
            foreach (var channel in Channels) channel.Mix(_mixed);
            Channels.RemoveAll(channel => { if (channel.IsPlaying) return false; channel.Dispose(); return true; });
            Buffer.BlockCopy(_mixed, 0, _bytes, 0, _bytes.Length);
            _writer.Write(_bytes);
            _samples += _mixed.Length;
        }

        internal static void End()
        {
            foreach (var channel in Channels) channel.Dispose();
            Channels.Clear();
            if (_writer != null)
            {
                var writer = _writer;
                _writer = null;
                try { writer.BaseStream.Position = 0; Header(writer, checked((uint)(_samples * 4))); }
                finally { writer.Dispose(); }
            }
            _mixed = null;
            _bytes = null;
        }

        internal static void Header(BinaryWriter writer, uint bytes, int rate = Rate, int channels = 2)
        {
            writer.Write(System.Text.Encoding.ASCII.GetBytes("RIFF")); writer.Write(checked(bytes + 36));
            writer.Write(System.Text.Encoding.ASCII.GetBytes("WAVEfmt ")); writer.Write(16);
            writer.Write((ushort)3); writer.Write((ushort)channels); writer.Write(rate); writer.Write(rate * channels * 4);
            writer.Write((ushort)(channels * 4)); writer.Write((ushort)32);
            writer.Write(System.Text.Encoding.ASCII.GetBytes("data")); writer.Write(bytes);
        }

        internal sealed class Channel : IDisposable
        {
            private readonly NativePcmPlayer.ISampleSource _source;
            private readonly bool _loop;
            private readonly byte[] _buffer;
            private int _at, _end;
            private readonly double _step;
            private double _phase;
            private float _left, _right, _nextLeft, _nextRight;
            private bool _next;
            private bool _disposed;
            internal bool IsPlaying { get; private set; }
            internal float Volume;

            internal Channel(NativePcmPlayer.ISampleSource source, bool loop, float volume)
            {
                _source = source;
                _loop = loop;
                Volume = volume;
                if (source.Channels < 1 || source.Channels > 2)
                {
                    source.Dispose();
                    throw new NotSupportedException("视频中的作品音频需要单声道或双声道");
                }
                _buffer = new byte[4096 * source.Channels * 2];
                _step = (double)source.SampleRate / Rate;
                IsPlaying = Read(out _left, out _right);
                if (IsPlaying) _next = Read(out _nextLeft, out _nextRight);
            }

            private bool Read(out float left, out float right)
            {
                left = right = 0;
                if (_at >= _end)
                {
                    _at = 0;
                    _end = _source.ReadPcm16(_buffer, 0, _buffer.Length);
                    if (_end == 0 && _loop && _source.CanSeek)
                    {
                        _source.Rewind();
                        _end = _source.ReadPcm16(_buffer, 0, _buffer.Length);
                    }
                    if (_end == 0) return false;
                }
                left = (short)(_buffer[_at] | _buffer[_at + 1] << 8) / 32768f;
                _at += 2;
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
                    output[frame * 2] += (_left + ((_next ? _nextLeft : _left) - _left) * fraction) * Volume;
                    output[frame * 2 + 1] += (_right + ((_next ? _nextRight : _right) - _right) * fraction) * Volume;
                    _phase += _step;
                    while (_phase >= 1 && IsPlaying)
                    {
                        _phase -= 1;
                        if (!_next) { Dispose(); break; }
                        _left = _nextLeft; _right = _nextRight;
                        _next = Read(out _nextLeft, out _nextRight);
                    }
                }
            }

            public void Dispose()
            {
                IsPlaying = false;
                if (_disposed) return;
                _disposed = true;
                _source.Dispose();
            }
        }
    }
}
