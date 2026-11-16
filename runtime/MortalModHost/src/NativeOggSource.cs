using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;

namespace MortalModHost
{
    // The decoder owns one encoded-input copy and stream state. Managed code
    // owns worker/device/loop timing; errors never switch an active stream to
    // NVorbis. All callbacks are bound before any handle can be allocated.
    internal sealed unsafe class NativeOggSource : NativePcmPlayer.ISampleSource
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int OpenCallback(byte* source, int length, out IntPtr handle, out int channels, out int rate);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int ReadCallback(IntPtr handle, byte* destination, int capacity);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int RewindCallback(IntPtr handle);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void DropCallback(IntPtr handle);
        private sealed class Backend
        {
            internal readonly OpenCallback Open;
            internal readonly ReadCallback Read;
            internal readonly RewindCallback Rewind;
            internal readonly DropCallback Drop;
            internal Backend(OpenCallback open)
            {
                Open = open;
                if (open == null) return;
                Read = NativeHostKernel.Bind<ReadCallback>("lom_ogg_read_pcm16_v1");
                Rewind = NativeHostKernel.Bind<RewindCallback>("lom_ogg_rewind_v1");
                Drop = NativeHostKernel.Bind<DropCallback>("lom_ogg_drop_v1");
            }
        }
        private static readonly Lazy<Backend> Current = new Lazy<Backend>(
            () => new Backend(NativeHostKernel.Bind<OpenCallback>("lom_ogg_open_v1")),
            LazyThreadSafetyMode.ExecutionAndPublication);
        private sealed class DecoderHandle : SafeHandle
        {
            private readonly DropCallback _drop;
            internal DecoderHandle(IntPtr value, DropCallback drop) : base(IntPtr.Zero, true)
            { _drop = drop; SetHandle(value); }
            public override bool IsInvalid { get { return handle == IntPtr.Zero; } }
            protected override bool ReleaseHandle() { _drop(handle); return true; }
        }
        private readonly object _gate = new object();
        private readonly Backend _backend;
        private readonly DecoderHandle _handle;
        internal static bool IsAvailable { get { return Current.Value.Open != null; } }
        public int Channels { get; private set; }
        public int SampleRate { get; private set; }
        public bool CanSeek { get { return true; } }

        internal NativeOggSource(byte[] encoded)
        {
            if (encoded == null) throw new ArgumentNullException("encoded");
            _backend = Current.Value;
            if (_backend.Open == null) throw new NotSupportedException("原生 OGG 解码组件不可用");
            IntPtr handle;
            int channels, rate, status;
            fixed (byte* source = encoded) status = _backend.Open(source, encoded.Length, out handle, out channels, out rate);
            if (status != 0)
            {
                if (handle != IntPtr.Zero) _backend.Drop(handle);
                throw new InvalidDataException("OGG 头无效或解码器无法读取，原生状态 " + status);
            }
            if (handle == IntPtr.Zero || channels <= 0 || rate <= 0)
            {
                if (handle != IntPtr.Zero) _backend.Drop(handle);
                throw new InvalidDataException("OGG 解码器返回无效的声道或采样率");
            }
            try { _handle = new DecoderHandle(handle, _backend.Drop); }
            catch { _backend.Drop(handle); throw; }
            Channels = channels;
            SampleRate = rate;
        }

        public int ReadPcm16(byte[] destination, int offset, int count)
        {
            int frames = count / (Channels * 2);
            if (frames <= 0) return 0;
            int capacity = frames * Channels * 2;
            if (destination == null) throw new ArgumentNullException("destination");
            if (offset < 0 || offset > destination.Length - capacity) throw new ArgumentOutOfRangeException("offset");
            lock (_gate)
            {
                RequireOpen();
                int written;
                bool held = false;
                try
                {
                    _handle.DangerousAddRef(ref held);
                    fixed (byte* output = destination) written = _backend.Read(_handle.DangerousGetHandle(), output + offset, capacity);
                }
                finally { if (held) _handle.DangerousRelease(); }
                if (written < 0) throw new InvalidDataException("OGG 解码失败，原生状态 " + written);
                if (written > capacity || written % (Channels * 2) != 0)
                    throw new InvalidDataException("OGG 解码器返回无效的 PCM 帧长度");
                return written;
            }
        }

        public void Rewind()
        {
            lock (_gate)
            {
                RequireOpen();
                int status;
                bool held = false;
                try { _handle.DangerousAddRef(ref held); status = _backend.Rewind(_handle.DangerousGetHandle()); }
                finally { if (held) _handle.DangerousRelease(); }
                if (status != 0) throw new InvalidDataException("OGG 无法回到开头，原生状态 " + status);
            }
        }
        public void Dispose() { lock (_gate) { if (_handle != null) _handle.Dispose(); } }
        private void RequireOpen()
        {
            if (_handle == null || _handle.IsClosed || _handle.IsInvalid) throw new ObjectDisposedException("NativeOggSource");
        }
    }
}
