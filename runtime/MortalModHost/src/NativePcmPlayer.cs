using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using NVorbis;

namespace MortalModHost
{
    /// <summary>
    /// winmm waveOut 流式播放。不用 CALLBACK_FUNCTION：从驱动线程回调托管代码
    /// 并在回调里 Unprepare/Write，切歌或循环时会把 32 位进程直接打崩。
    /// 改为 CALLBACK_EVENT + 工作线程，停播时先 Reset/Join 再释放。
    /// </summary>
    internal sealed class NativePcmPlayer : IDisposable
    {
        private const uint WAVE_MAPPER = 0xFFFFFFFF;
        private const uint CALLBACK_EVENT = 0x00050000;
        private const uint WHDR_DONE = 0x00000001;
        private const int WAVE_FORMAT_PCM = 1;
        private const int MMSYSERR_NOERROR = 0;
        private const int FramesPerBuffer = 4096;

        [StructLayout(LayoutKind.Sequential)]
        private class WaveFormatEx
        {
            public ushort wFormatTag;
            public ushort nChannels;
            public uint nSamplesPerSec;
            public uint nAvgBytesPerSec;
            public ushort nBlockAlign;
            public ushort wBitsPerSample;
            public ushort cbSize;
        }

        [StructLayout(LayoutKind.Sequential)]
        private class WaveHeader
        {
            public IntPtr lpData;
            public uint dwBufferLength;
            public uint dwBytesRecorded;
            public IntPtr dwUser;
            public uint dwFlags;
            public uint dwLoops;
            public IntPtr lpNext;
            public IntPtr reserved;
        }

        [DllImport("winmm.dll")]
        private static extern int waveOutOpen(out IntPtr handle, uint deviceId, WaveFormatEx format, IntPtr callback, IntPtr instance, uint flags);

        [DllImport("winmm.dll")]
        private static extern int waveOutClose(IntPtr handle);

        [DllImport("winmm.dll")]
        private static extern int waveOutPrepareHeader(IntPtr handle, IntPtr header, uint size);

        [DllImport("winmm.dll")]
        private static extern int waveOutUnprepareHeader(IntPtr handle, IntPtr header, uint size);

        [DllImport("winmm.dll")]
        private static extern int waveOutWrite(IntPtr handle, IntPtr header, uint size);

        [DllImport("winmm.dll")]
        private static extern int waveOutReset(IntPtr handle);

        [DllImport("winmm.dll")]
        private static extern int waveOutSetVolume(IntPtr handle, uint volume);

        [DllImport("kernel32.dll")]
        private static extern IntPtr CreateEvent(IntPtr attr, bool manual, bool initial, string name);

        [DllImport("kernel32.dll")]
        private static extern bool CloseHandle(IntPtr handle);

        [DllImport("kernel32.dll")]
        private static extern uint WaitForSingleObject(IntPtr handle, uint ms);

        private readonly object _gate = new object();
        private IntPtr _wave;
        private IntPtr _event;
        private Thread _thread;
        private volatile bool _stopping;
        private volatile bool _playing;
        private volatile bool _loop;
        private volatile float _volume = 1f;
        private byte[] _encoded;
        private string _ext;

        public bool IsPlaying { get { return _playing && !_stopping; } }

        public void Play(byte[] encoded, string extension, bool loop)
        {
            if (encoded == null || encoded.Length == 0)
                throw new ArgumentException("音频数据为空");
            Stop();
            _encoded = encoded;
            _ext = (extension ?? "").ToLowerInvariant();
            _loop = loop;
            _stopping = false;
            _playing = true;
            _thread = new Thread(Worker)
            {
                IsBackground = true,
                Name = "lom-waveOut"
            };
            _thread.Start();
        }

        public void SetVolume(float linear01)
        {
            if (linear01 < 0f) linear01 = 0f;
            if (linear01 > 1f) linear01 = 1f;
            _volume = linear01;
            uint word = (uint)(linear01 * 0xFFFF);
            uint packed = word | (word << 16);
            lock (_gate)
            {
                if (_wave != IntPtr.Zero)
                    waveOutSetVolume(_wave, packed);
            }
        }

        public void Stop()
        {
            _stopping = true;
            _playing = false;
            IntPtr ev;
            lock (_gate)
            {
                if (_wave != IntPtr.Zero)
                    waveOutReset(_wave);
                ev = _event;
            }
            if (ev != IntPtr.Zero)
                SetEvent(ev);
            Thread thread = _thread;
            if (thread != null && thread.IsAlive && thread != Thread.CurrentThread)
                thread.Join(3000);
            _thread = null;
            _encoded = null;
        }

        [DllImport("kernel32.dll")]
        private static extern bool SetEvent(IntPtr handle);

        public void Dispose()
        {
            Stop();
        }

        private void Worker()
        {
            ISampleSource source = null;
            GCHandle pin0 = default(GCHandle);
            GCHandle pin1 = default(GCHandle);
            GCHandle pinH0 = default(GCHandle);
            GCHandle pinH1 = default(GCHandle);
            WaveHeader hdr0 = null;
            WaveHeader hdr1 = null;
            try
            {
                source = OpenSource(_encoded, _ext);
                if (source == null)
                    return;
                int channels = source.Channels;
                int rate = source.SampleRate;
                int bytesPerBuf = FramesPerBuffer * channels * 2;
                byte[] buf0 = new byte[bytesPerBuf];
                byte[] buf1 = new byte[bytesPerBuf];
                pin0 = GCHandle.Alloc(buf0, GCHandleType.Pinned);
                pin1 = GCHandle.Alloc(buf1, GCHandleType.Pinned);
                hdr0 = NewHeader(pin0.AddrOfPinnedObject());
                hdr1 = NewHeader(pin1.AddrOfPinnedObject());
                pinH0 = GCHandle.Alloc(hdr0, GCHandleType.Pinned);
                pinH1 = GCHandle.Alloc(hdr1, GCHandleType.Pinned);
                IntPtr p0 = pinH0.AddrOfPinnedObject();
                IntPtr p1 = pinH1.AddrOfPinnedObject();

                var format = new WaveFormatEx
                {
                    wFormatTag = WAVE_FORMAT_PCM,
                    nChannels = (ushort)channels,
                    nSamplesPerSec = (uint)rate,
                    wBitsPerSample = 16,
                    nBlockAlign = (ushort)(channels * 2),
                    nAvgBytesPerSec = (uint)(rate * channels * 2),
                    cbSize = 0
                };

                lock (_gate)
                {
                    if (_stopping)
                        return;
                    _event = CreateEvent(IntPtr.Zero, false, false, null);
                    int open = waveOutOpen(out _wave, WAVE_MAPPER, format, _event, IntPtr.Zero, CALLBACK_EVENT);
                    if (open != MMSYSERR_NOERROR || _wave == IntPtr.Zero)
                        throw new InvalidOperationException("waveOutOpen 失败：" + open);
                    ApplyVolumeUnlocked();
                    uint hdrSize = (uint)Marshal.SizeOf(typeof(WaveHeader));
                    if (!Prime(source, buf0, hdr0, p0, hdrSize) && !TryLoopPrime(source, buf0, hdr0, p0, hdrSize))
                        return;
                    Prime(source, buf1, hdr1, p1, hdrSize);
                }

                while (!_stopping)
                {
                    WaitForSingleObject(_event, 250);
                    if (_stopping)
                        break;
                    lock (_gate)
                    {
                        if (_stopping || _wave == IntPtr.Zero)
                            break;
                        uint hdrSize = (uint)Marshal.SizeOf(typeof(WaveHeader));
                        RefillIfDone(source, buf0, hdr0, p0, hdrSize);
                        RefillIfDone(source, buf1, hdr1, p1, hdrSize);
                    }
                }
            }
            catch
            {
                _playing = false;
            }
            finally
            {
                lock (_gate)
                {
                    if (_wave != IntPtr.Zero)
                    {
                        try { waveOutReset(_wave); } catch { }
                        uint hdrSize = (uint)Marshal.SizeOf(typeof(WaveHeader));
                        try { if (pinH0.IsAllocated) waveOutUnprepareHeader(_wave, pinH0.AddrOfPinnedObject(), hdrSize); } catch { }
                        try { if (pinH1.IsAllocated) waveOutUnprepareHeader(_wave, pinH1.AddrOfPinnedObject(), hdrSize); } catch { }
                        try { waveOutClose(_wave); } catch { }
                        _wave = IntPtr.Zero;
                    }
                    if (_event != IntPtr.Zero)
                    {
                        try { CloseHandle(_event); } catch { }
                        _event = IntPtr.Zero;
                    }
                }
                if (pinH0.IsAllocated) pinH0.Free();
                if (pinH1.IsAllocated) pinH1.Free();
                if (pin0.IsAllocated) pin0.Free();
                if (pin1.IsAllocated) pin1.Free();
                if (source != null)
                    source.Dispose();
                _playing = false;
            }
        }

        private void ApplyVolumeUnlocked()
        {
            uint word = (uint)(_volume * 0xFFFF);
            waveOutSetVolume(_wave, word | (word << 16));
        }

        private WaveHeader NewHeader(IntPtr data)
        {
            return new WaveHeader { lpData = data };
        }

        private bool Prime(ISampleSource source, byte[] buf, WaveHeader hdr, IntPtr hdrPtr, uint hdrSize)
        {
            int n = source.ReadPcm16(buf, 0, buf.Length);
            if (n <= 0)
                return false;
            hdr.dwBufferLength = (uint)n;
            hdr.dwFlags = 0;
            waveOutPrepareHeader(_wave, hdrPtr, hdrSize);
            waveOutWrite(_wave, hdrPtr, hdrSize);
            return true;
        }

        private bool TryLoopPrime(ISampleSource source, byte[] buf, WaveHeader hdr, IntPtr hdrPtr, uint hdrSize)
        {
            if (!_loop || !source.CanSeek)
                return false;
            source.Rewind();
            return Prime(source, buf, hdr, hdrPtr, hdrSize);
        }

        private void RefillIfDone(ISampleSource source, byte[] buf, WaveHeader hdr, IntPtr hdrPtr, uint hdrSize)
        {
            if ((hdr.dwFlags & WHDR_DONE) == 0)
                return;
            waveOutUnprepareHeader(_wave, hdrPtr, hdrSize);
            int n = source.ReadPcm16(buf, 0, buf.Length);
            if (n <= 0)
            {
                if (_loop && source.CanSeek)
                {
                    source.Rewind();
                    n = source.ReadPcm16(buf, 0, buf.Length);
                }
                if (n <= 0)
                    return;
            }
            hdr.dwBufferLength = (uint)n;
            hdr.dwFlags = 0;
            waveOutPrepareHeader(_wave, hdrPtr, hdrSize);
            waveOutWrite(_wave, hdrPtr, hdrSize);
        }

        private static ISampleSource OpenSource(byte[] encoded, string ext)
        {
            if (ext == ".ogg")
                return new OggSource(encoded);
            return new WavSource(encoded);
        }

        private interface ISampleSource : IDisposable
        {
            int Channels { get; }
            int SampleRate { get; }
            bool CanSeek { get; }
            void Rewind();
            int ReadPcm16(byte[] dest, int offset, int count);
        }

        private sealed class OggSource : ISampleSource
        {
            private readonly MemoryStream _raw;
            private VorbisReader _reader;
            private float[] _scratch;

            public OggSource(byte[] data)
            {
                _raw = new MemoryStream(data, writable: false);
                _reader = new VorbisReader(_raw, false);
                Channels = _reader.Channels;
                SampleRate = _reader.SampleRate;
                if (Channels <= 0 || SampleRate <= 0)
                    throw new InvalidOperationException("OGG 头无效");
            }

            public int Channels { get; private set; }
            public int SampleRate { get; private set; }
            public bool CanSeek { get { return true; } }

            public void Rewind()
            {
                _reader.DecodedPosition = 0;
            }

            public int ReadPcm16(byte[] dest, int offset, int count)
            {
                int framesWanted = count / (Channels * 2);
                if (framesWanted <= 0)
                    return 0;
                int floatsWanted = framesWanted * Channels;
                if (_scratch == null || _scratch.Length < floatsWanted)
                    _scratch = new float[floatsWanted];
                int got = _reader.ReadSamples(_scratch, 0, floatsWanted);
                if (got <= 0)
                    return 0;
                int bytes = got * 2;
                for (int i = 0; i < got; i++)
                {
                    float f = _scratch[i];
                    if (f > 1f) f = 1f;
                    if (f < -1f) f = -1f;
                    short s = (short)(f * 32767f);
                    dest[offset + i * 2] = (byte)(s & 0xFF);
                    dest[offset + i * 2 + 1] = (byte)((s >> 8) & 0xFF);
                }
                return bytes;
            }

            public void Dispose()
            {
                if (_reader != null)
                {
                    _reader.Dispose();
                    _reader = null;
                }
                _raw.Dispose();
            }
        }

        private sealed class WavSource : ISampleSource
        {
            private readonly byte[] _data;
            private readonly int _dataPos;
            private readonly int _dataLen;
            private readonly int _bytesPerSample;
            private readonly int _formatTag;
            private int _cursor;

            public WavSource(byte[] data)
            {
                _data = data;
                if (data.Length < 44 || ReadFour(data, 0) != "RIFF" || ReadFour(data, 8) != "WAVE")
                    throw new InvalidOperationException("不是 RIFF/WAVE");
                int offset = 12;
                int channels = 0, rate = 0, bits = 0, tag = 0, pos = -1, len = 0;
                while (offset + 8 <= data.Length)
                {
                    string chunk = ReadFour(data, offset);
                    int size = BitConverter.ToInt32(data, offset + 4);
                    if (size < 0 || offset + 8 + size > data.Length)
                        break;
                    if (chunk == "fmt ")
                    {
                        tag = BitConverter.ToInt16(data, offset + 8);
                        channels = BitConverter.ToInt16(data, offset + 10);
                        rate = BitConverter.ToInt32(data, offset + 12);
                        bits = BitConverter.ToInt16(data, offset + 22);
                    }
                    else if (chunk == "data")
                    {
                        pos = offset + 8;
                        len = size;
                    }
                    offset += 8 + size + (size & 1);
                }
                if (pos < 0 || channels <= 0 || rate <= 0 || len <= 0)
                    throw new InvalidOperationException("WAV 缺少 fmt/data");
                if (!((tag == 1 && (bits == 8 || bits == 16)) || ((tag == 1 || tag == 3) && bits == 32)))
                    throw new InvalidOperationException("不支持的 WAV 格式 tag=" + tag + " bits=" + bits);
                Channels = channels;
                SampleRate = rate;
                _dataPos = pos;
                _dataLen = len;
                _bytesPerSample = bits / 8;
                _formatTag = tag;
                _cursor = 0;
            }

            public int Channels { get; private set; }
            public int SampleRate { get; private set; }
            public bool CanSeek { get { return true; } }

            public void Rewind()
            {
                _cursor = 0;
            }

            public int ReadPcm16(byte[] dest, int offset, int count)
            {
                int framesWanted = count / (Channels * 2);
                int framesAvail = (_dataLen - _cursor) / (_bytesPerSample * Channels);
                int frames = framesWanted < framesAvail ? framesWanted : framesAvail;
                if (frames <= 0)
                    return 0;
                int src = _dataPos + _cursor;
                if (_formatTag == 1 && _bytesPerSample == 2)
                {
                    int bytes = frames * Channels * 2;
                    Buffer.BlockCopy(_data, src, dest, offset, bytes);
                    _cursor += bytes;
                    return bytes;
                }
                int written = 0;
                for (int f = 0; f < frames * Channels; f++)
                {
                    float sample;
                    if (_bytesPerSample == 1)
                    {
                        sample = (_data[src++] - 128) / 128f;
                    }
                    else
                    {
                        sample = BitConverter.ToSingle(_data, src);
                        src += 4;
                    }
                    if (sample > 1f) sample = 1f;
                    if (sample < -1f) sample = -1f;
                    short s = (short)(sample * 32767f);
                    dest[offset + written] = (byte)(s & 0xFF);
                    dest[offset + written + 1] = (byte)((s >> 8) & 0xFF);
                    written += 2;
                }
                _cursor = src - _dataPos;
                return written;
            }

            public void Dispose() { }
        }

        private static string ReadFour(byte[] data, int offset)
        {
            return ((char)data[offset]).ToString()
                + (char)data[offset + 1]
                + (char)data[offset + 2]
                + (char)data[offset + 3];
        }
    }
}
