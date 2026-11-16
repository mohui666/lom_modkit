using System;
using System.Runtime.InteropServices;

namespace MortalModHost
{
    internal static unsafe class NativeIntegrity
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint VerifyCallback(char* mod, int modLength,
            char* fingerprint, int fingerprintLength, byte* seal, int sealLength,
            byte* stamp, byte* expectedDigest, byte* keyA, byte* keyB);
        private static readonly VerifyCallback Verify = NativeHostKernel.Bind<VerifyCallback>("lom_integrity_verify_v1");
        internal static bool IsAvailable { get { return Verify != null; } }
        internal static string LoadedPath { get { return NativeHostKernel.LoadedPath; } }
        internal static string UnavailableReason { get { return NativeHostKernel.UnavailableReason; } }
        internal static bool FallbackNoticePending { get { return NativeHostKernel.FallbackNoticePending; } }
        internal static void ReportFallbackOnce(Action<string> warning) { NativeHostKernel.ReportFallbackOnce(warning); }

        internal static bool TryVerify(string modId, string fingerprint, byte[] seal,
            byte[] stamp, byte[] expectedDigest, byte[] keyA, byte[] keyB, out uint status)
        {
            VerifyCallback verify = Verify;
            if (verify == null) { status = 0; return false; }
            // These are the owning class's fixed-size arrays, not native copies
            // of the constants. Guard direct callers before pinning an ABI with
            // no length parameters for these four fixed-size arguments.
            if (stamp == null || stamp.Length != 16
                || expectedDigest == null || expectedDigest.Length != 32
                || keyA == null || keyA.Length != 16 || keyB == null || keyB.Length != 16)
                throw new ArgumentException("原生身份校验的固定数组长度必须为 16/32/16/16");
            fixed (char* modPointer = modId)
            fixed (char* fingerprintPointer = fingerprint)
            fixed (byte* sealPointer = seal)
            fixed (byte* stampPointer = stamp)
            fixed (byte* digestPointer = expectedDigest)
            fixed (byte* keyAPointer = keyA)
            fixed (byte* keyBPointer = keyB)
                status = verify(modPointer, modId == null ? 0 : modId.Length,
                    fingerprintPointer, fingerprint == null ? 0 : fingerprint.Length,
                    sealPointer, seal == null ? 0 : seal.Length, stampPointer,
                    digestPointer, keyAPointer, keyBPointer);
            return true;
        }

    }
}
