using System;
using System.Security.Cryptography;
using System.Text;

namespace MortalModHost
{
    /// <summary>
    /// 强制披露的纯完整性规则。HMAC 会把活动包身份、固定非官方标记和协议版本
    /// 绑定成会话封印；对象名也按 Host 计算的包指纹派生，避免只靠猜固定名字就
    /// 删除标记。它是纵深防御而不是 DRM：控制宿主 DLL 的人最终仍能修改程序。
    /// </summary>
    internal static class DisclosureIntegrity
    {
        private const byte StampMask = 0x5A;
        private const string Protocol = "lom-disclosure-integrity-v2";

        // "MOD / UNOFFICIAL" 以轻量编码保存，避免二进制里出现一个可直接搜索并
        // 批量替换的唯一明文；真正安全性来自 Host 生命周期与 fail-closed，而非此编码。
        private static readonly byte[] EncodedStamp =
        {
            23, 21, 30, 122, 117, 122, 15, 20, 21, 28, 28, 19, 25, 19, 27, 22
        };

        private static readonly byte[] ExpectedStampDigest =
        {
            0xF7, 0x20, 0x81, 0xB2, 0x9C, 0xFB, 0x0A, 0x8C,
            0x53, 0x80, 0x36, 0xE9, 0x8B, 0x7D, 0xE1, 0x1E,
            0x68, 0x5A, 0xEA, 0x05, 0xEC, 0x9A, 0x4F, 0xE2,
            0xBF, 0xA2, 0x00, 0x60, 0x11, 0x0C, 0xA1, 0xFF
        };

        private static readonly byte[] KeyPartA =
        {
            0x93, 0x21, 0xD7, 0x4B, 0x55, 0xA8, 0x0C, 0xFE,
            0x11, 0x69, 0xB2, 0x38, 0xC4, 0x7D, 0x02, 0xEA
        };

        private static readonly byte[] KeyPartB =
        {
            0x2D, 0xF0, 0x41, 0x98, 0xC3, 0x16, 0x77, 0x5A,
            0xE1, 0x0B, 0x64, 0xAF, 0x39, 0xD2, 0x8C, 0x05
        };

        internal static string FixedStamp()
        {
            byte[] decoded = new byte[EncodedStamp.Length];
            for (int i = 0; i < decoded.Length; i++)
                decoded[i] = (byte)(EncodedStamp[i] ^ StampMask);
            return Encoding.ASCII.GetString(decoded);
        }

        internal static bool IsFixedStampIntact()
        {
            byte[] decoded = Encoding.ASCII.GetBytes(FixedStamp());
            byte[] digest;
            using (var sha = SHA256.Create())
                digest = sha.ComputeHash(decoded);
            int difference = digest.Length ^ ExpectedStampDigest.Length;
            int length = Math.Min(digest.Length, ExpectedStampDigest.Length);
            for (int i = 0; i < length; i++)
                difference |= digest[i] ^ ExpectedStampDigest[i];
            return difference == 0;
        }

        internal static string VisibleWatermarkText(string shortFingerprint)
        {
            string identity = string.IsNullOrEmpty(shortFingerprint)
                ? FixedStamp()
                : FixedStamp() + " / " + shortFingerprint;
            return identity + "     " + identity + "\n" + identity + "     " + identity;
        }

        internal static byte[] CreateSessionSeal(string modId, string fingerprint)
        {
            if (string.IsNullOrEmpty(modId))
                throw new ArgumentException("mod id 缺失", nameof(modId));
            if (!ModDisclosurePolicy.IsValidPackageFingerprint(fingerprint))
                throw new ArgumentException("包指纹无效", nameof(fingerprint));
            byte[] payload = Encoding.UTF8.GetBytes(
                Protocol + "\n" + modId + "\n" + fingerprint.ToUpperInvariant()
                + "\n" + FixedStamp());
            using (var hmac = new HMACSHA256(BuildKey()))
                return hmac.ComputeHash(payload);
        }

        internal static bool VerifySessionSeal(
            string modId, string fingerprint, byte[] candidate)
        {
            if (candidate == null || candidate.Length != 32) return false;
            byte[] expected;
            try
            {
                expected = CreateSessionSeal(modId, fingerprint);
            }
            catch (ArgumentException)
            {
                return false;
            }
            int difference = 0;
            for (int i = 0; i < expected.Length; i++)
                difference |= expected[i] ^ candidate[i];
            return difference == 0;
        }

        internal static string ProtectedObjectName(string role, string fingerprint)
        {
            if (string.IsNullOrEmpty(role)) role = "surface";
            string identity = ModDisclosurePolicy.IsValidPackageFingerprint(fingerprint)
                ? fingerprint.ToUpperInvariant()
                : new string('0', 64);
            byte[] data = Encoding.UTF8.GetBytes(Protocol + "\n" + role + "\n" + identity);
            byte[] digest;
            using (var hmac = new HMACSHA256(BuildKey()))
                digest = hmac.ComputeHash(data);
            var suffix = new StringBuilder(16);
            for (int i = 0; i < 8; i++) suffix.Append(digest[i].ToString("x2"));
            return "_msh_" + suffix;
        }

        private static byte[] BuildKey()
        {
            var key = new byte[KeyPartA.Length + KeyPartB.Length];
            for (int i = 0; i < KeyPartA.Length; i++)
                key[i] = (byte)(KeyPartA[i] ^ (0x37 + i));
            for (int i = 0; i < KeyPartB.Length; i++)
                key[KeyPartA.Length + i] = (byte)(KeyPartB[i] ^ (0xA1 - i));
            return key;
        }
    }
}
