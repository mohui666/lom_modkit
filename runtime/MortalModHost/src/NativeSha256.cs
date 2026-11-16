using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using Microsoft.Win32.SafeHandles;

namespace MortalModHost
{
    // The package's framing and safety checks remain in ModLoader. This class
    // only streams bytes through the existing optional native calculation kernel.
    internal sealed unsafe class NativeSha256 : IDisposable
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr NewCallback();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int UpdateCallback(IntPtr context, byte* data, int length);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int UpdatePairCallback(IntPtr first, IntPtr second, byte* data, int length);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int FinishCallback(IntPtr context, byte* output, int capacity);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void FreeCallback(IntPtr context);
        private static readonly NewCallback New = NativeHostKernel.Bind<NewCallback>("lom_sha256_new_v1");
        private static readonly UpdateCallback Update = NativeHostKernel.Bind<UpdateCallback>("lom_sha256_update_v1");
        private static readonly UpdatePairCallback UpdatePair = NativeHostKernel.Bind<UpdatePairCallback>("lom_sha256_update_pair_v1");
        private static readonly FinishCallback Final = NativeHostKernel.Bind<FinishCallback>("lom_sha256_finish_v1");
        private static readonly FreeCallback Free = NativeHostKernel.Bind<FreeCallback>("lom_sha256_free_v1");
        private sealed class Context : SafeHandleZeroOrMinusOneIsInvalid
        {
            internal Context() : base(true) { SetHandle(New()); }
            protected override bool ReleaseHandle() { Free(handle); return true; }
        }
        private readonly Context _context;
        private readonly SHA256 _managed;
        private bool _finished, _disposed;
        internal static bool IsAvailable { get { return New != null; } }
        internal NativeSha256(bool useNative = true)
        {
            if (useNative && IsAvailable)
            {
                _context = new Context();
                if (_context.IsInvalid) throw new InvalidOperationException("原生 SHA-256 上下文创建失败");
            }
            else _managed = SHA256.Create();
        }
        internal void Append(byte[] data, int offset, int count)
        {
            if (data == null) throw new ArgumentNullException(nameof(data));
            if (offset < 0 || count < 0 || offset > data.Length - count) throw new ArgumentOutOfRangeException(nameof(count));
            Check();
            if (_context == null) _managed.TransformBlock(data, offset, count, data, offset);
            else fixed (byte* bytes = data)
            {
                int status = Update(_context.DangerousGetHandle(), bytes + offset, count);
                GC.KeepAlive(_context);
                if (status != 0) throw new InvalidOperationException("原生 SHA-256 更新失败：" + status);
            }
        }
        // This optimization is scoped to two live hashes of the same read.
        // Native errors never retry through the managed implementation.
        internal void AppendWith(NativeSha256 second, byte[] data, int offset, int count)
        {
            if (second == null) { Append(data, offset, count); return; }
            if (data == null) throw new ArgumentNullException(nameof(data));
            if (offset < 0 || count < 0 || offset > data.Length - count) throw new ArgumentOutOfRangeException(nameof(count));
            Check(); second.Check();
            if (ReferenceEquals(this, second)) throw new ArgumentException("SHA-256 双摘要必须使用两个独立上下文", nameof(second));
            if (_context == null || second._context == null)
            {
                Append(data, offset, count); second.Append(data, offset, count);
                return;
            }
            bool heldFirst = false, heldSecond = false;
            try
            {
                _context.DangerousAddRef(ref heldFirst);
                second._context.DangerousAddRef(ref heldSecond);
                int status;
                fixed (byte* bytes = data)
                    status = UpdatePair(_context.DangerousGetHandle(), second._context.DangerousGetHandle(), bytes + offset, count);
                if (status != 0) throw new InvalidOperationException("原生 SHA-256 双摘要更新失败：" + status);
            }
            finally
            {
                if (heldSecond) second._context.DangerousRelease();
                if (heldFirst) _context.DangerousRelease();
            }
        }
        internal byte[] Finish()
        {
            Check();
            _finished = true;
            if (_context == null) { _managed.TransformFinalBlock(new byte[0], 0, 0); return _managed.Hash; }
            var digest = new byte[32];
            fixed (byte* output = digest)
            {
                int status = Final(_context.DangerousGetHandle(), output, digest.Length);
                GC.KeepAlive(_context);
                if (status != 32) throw new InvalidOperationException("原生 SHA-256 结束失败：" + status);
            }
            return digest;
        }
        internal static byte[] Hash(Stream input, long maximum = long.MaxValue, long expected = -1, string path = "")
        {
            using (var digest = new NativeSha256())
            {
                var buffer = new byte[81920];
                long total = 0;
                int read;
                while ((read = input.Read(buffer, 0, buffer.Length)) != 0)
                {
                    if (total > maximum - read) throw new FormatException("条目读取超过大小上限：" + path);
                    if (expected >= 0 && total > expected - read) throw new FormatException("条目读取长度与 ZIP 元数据不一致：" + path);
                    digest.Append(buffer, 0, read);
                    total += read;
                }
                if (expected >= 0 && total != expected) throw new FormatException("条目读取长度与 ZIP 元数据不一致：" + path);
                return digest.Finish();
            }
        }
        private void Check()
        {
            if (_disposed) throw new ObjectDisposedException(nameof(NativeSha256));
            if (_finished) throw new InvalidOperationException("SHA-256 已经结束");
        }
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            if (_context != null) _context.Dispose();
            if (_managed != null) _managed.Dispose();
        }
    }
}
