using System;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace MortalModHost
{
    /// <summary>
    /// Fixed provenance payload shared with lomc.watermark_protocol.
    /// This is identification framing, not a signature, DRM, or an embedding algorithm.
    /// </summary>
    public static class ProvenanceWatermarkProtocol
    {
        public const byte ProtocolVersion = 1;
        public const int PacketSize = 28;
        public const int ModIdHashSize = 16;
        public const int BitCount = PacketSize * 8;

        private static readonly byte[] Magic = { 0x4C, 0x4F, 0x4D, 0x57 }; // LOMW
        private static readonly byte[] Domain = Encoding.ASCII.GetBytes(
            "lom_modkit:watermark:mod-id:v1\0");
        private static readonly Regex ModIdPattern = new Regex(
            "^[a-z0-9_-]{1,64}$", RegexOptions.CultureInvariant);

        public sealed class Packet
        {
            public byte Protocol { get; internal set; }
            public byte Algorithm { get; internal set; }
            public byte Flags { get; internal set; }
            public byte[] ModIdHash { get; internal set; }
            public uint Checksum { get; internal set; }
            public bool ChecksumValid { get; internal set; }

            public string ModIdHashHex
            {
                get { return ToHex(ModIdHash); }
            }
        }

        public static byte[] HashModId(string modId)
        {
            if (string.IsNullOrEmpty(modId) || !ModIdPattern.IsMatch(modId))
                throw new ArgumentException(
                    "watermark mod_id must match [a-z0-9_-]{1,64}", "modId");
            byte[] id = Encoding.ASCII.GetBytes(modId);
            byte[] input = new byte[Domain.Length + id.Length];
            Buffer.BlockCopy(Domain, 0, input, 0, Domain.Length);
            Buffer.BlockCopy(id, 0, input, Domain.Length, id.Length);
            byte[] full;
            using (var sha = SHA256.Create())
                full = sha.ComputeHash(input);
            byte[] result = new byte[ModIdHashSize];
            Buffer.BlockCopy(full, 0, result, 0, result.Length);
            return result;
        }

        public static byte[] Encode(string modId, byte algorithmVersion)
        {
            if (algorithmVersion == 0)
                throw new ArgumentOutOfRangeException(
                    "algorithmVersion", "algorithm version must be 1..255");
            byte[] packet = new byte[PacketSize];
            Buffer.BlockCopy(Magic, 0, packet, 0, Magic.Length);
            packet[4] = ProtocolVersion;
            packet[5] = algorithmVersion;
            packet[6] = 0; // flags: protocol v1 reserves all bits
            packet[7] = 0; // reserved
            Buffer.BlockCopy(HashModId(modId), 0, packet, 8, ModIdHashSize);
            WriteUInt32BigEndian(packet, 24, Crc32(packet, 0, 24));
            return packet;
        }

        public static bool TryParse(byte[] bytes, out Packet packet, out string error)
        {
            packet = null;
            error = null;
            if (bytes == null || bytes.Length != PacketSize)
            {
                error = "watermark payload must be exactly 28 bytes";
                return false;
            }
            for (int i = 0; i < Magic.Length; i++)
            {
                if (bytes[i] != Magic[i])
                {
                    error = "watermark magic mismatch";
                    return false;
                }
            }
            if (bytes[4] != ProtocolVersion)
            {
                error = "unsupported watermark protocol version " + bytes[4];
                return false;
            }
            if (bytes[5] == 0)
            {
                error = "watermark algorithm version cannot be zero";
                return false;
            }
            if (bytes[6] != 0 || bytes[7] != 0)
            {
                error = "watermark protocol v1 flags/reserved must be zero";
                return false;
            }
            var identity = new byte[ModIdHashSize];
            Buffer.BlockCopy(bytes, 8, identity, 0, identity.Length);
            uint checksum = ReadUInt32BigEndian(bytes, 24);
            packet = new Packet
            {
                Protocol = bytes[4],
                Algorithm = bytes[5],
                Flags = bytes[6],
                ModIdHash = identity,
                Checksum = checksum,
                ChecksumValid = checksum == Crc32(bytes, 0, 24)
            };
            return true;
        }

        public static bool TryDecode(byte[] bytes, out Packet packet, out string error)
        {
            if (!TryParse(bytes, out packet, out error)) return false;
            if (!packet.ChecksumValid)
            {
                error = "watermark CRC-32 mismatch";
                return false;
            }
            return true;
        }

        public static byte[] ToBits(byte[] packet)
        {
            if (packet == null || packet.Length != PacketSize)
                throw new ArgumentException("watermark payload must be exactly 28 bytes", "packet");
            byte[] bits = new byte[BitCount];
            for (int i = 0; i < packet.Length; i++)
                for (int shift = 7; shift >= 0; shift--)
                    bits[i * 8 + (7 - shift)] = (byte)((packet[i] >> shift) & 1);
            return bits;
        }

        public static byte[] FromBits(byte[] bits)
        {
            if (bits == null || bits.Length != BitCount)
                throw new ArgumentException("watermark bit sequence must contain 224 bits", "bits");
            byte[] packet = new byte[PacketSize];
            for (int i = 0; i < bits.Length; i++)
            {
                if (bits[i] != 0 && bits[i] != 1)
                    throw new ArgumentException("watermark bits must be zero or one", "bits");
                packet[i / 8] |= (byte)(bits[i] << (7 - i % 8));
            }
            return packet;
        }

        private static uint Crc32(byte[] data, int offset, int count)
        {
            uint crc = 0xFFFFFFFFu;
            for (int i = offset; i < offset + count; i++)
            {
                crc ^= data[i];
                for (int bit = 0; bit < 8; bit++)
                    crc = (crc >> 1) ^ ((crc & 1) != 0 ? 0xEDB88320u : 0u);
            }
            return crc ^ 0xFFFFFFFFu;
        }

        private static uint ReadUInt32BigEndian(byte[] data, int offset)
        {
            return ((uint)data[offset] << 24)
                | ((uint)data[offset + 1] << 16)
                | ((uint)data[offset + 2] << 8)
                | data[offset + 3];
        }

        private static void WriteUInt32BigEndian(byte[] data, int offset, uint value)
        {
            data[offset] = (byte)(value >> 24);
            data[offset + 1] = (byte)(value >> 16);
            data[offset + 2] = (byte)(value >> 8);
            data[offset + 3] = (byte)value;
        }

        private static string ToHex(byte[] value)
        {
            if (value == null) return "";
            var builder = new StringBuilder(value.Length * 2);
            for (int i = 0; i < value.Length; i++)
                builder.Append(value[i].ToString("X2"));
            return builder.ToString();
        }
    }
}
