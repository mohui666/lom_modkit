using System;
using System.Security.Cryptography;
using System.Text;

namespace MortalModHost
{
    /// <summary>Algorithm v1 Hamming ECC and public keyed carrier layout.</summary>
    public static class ProvenanceWatermarkCodec
    {
        public const byte AlgorithmVersion = 1;
        public const int DataBits = ProvenanceWatermarkProtocol.BitCount;
        public const int EccBits = DataBits / 4 * 7;
        public const int GridColumns = 28;
        public const int GridRows = 14;
        public const int CellSize = 16;
        public const int TileWidth = GridColumns * CellSize;
        public const int TileHeight = GridRows * CellSize;
        public const byte OverlayAlpha = 4;

        private static readonly byte[] CarrierKey = Encoding.ASCII.GetBytes(
            "lom_modkit:watermark:carrier-prng:algorithm:1");

        private sealed class XorShift32
        {
            private uint _state;

            public XorShift32(uint seed)
            {
                _state = seed == 0 ? 0x6D2B79F5u : seed;
            }

            public uint Next()
            {
                uint value = _state;
                value ^= value << 13;
                value ^= value >> 17;
                value ^= value << 5;
                _state = value;
                return value;
            }
        }

        public static byte[] HammingEncode(byte[] payload)
        {
            if (payload == null || payload.Length != ProvenanceWatermarkProtocol.PacketSize)
                throw new ArgumentException("watermark payload must be exactly 28 bytes", "payload");
            byte[] data = ProvenanceWatermarkProtocol.ToBits(payload);
            byte[] encoded = new byte[EccBits];
            int output = 0;
            for (int offset = 0; offset < data.Length; offset += 4)
            {
                byte d1 = data[offset];
                byte d2 = data[offset + 1];
                byte d3 = data[offset + 2];
                byte d4 = data[offset + 3];
                encoded[output++] = (byte)(d1 ^ d2 ^ d4);
                encoded[output++] = (byte)(d1 ^ d3 ^ d4);
                encoded[output++] = d1;
                encoded[output++] = (byte)(d2 ^ d3 ^ d4);
                encoded[output++] = d2;
                encoded[output++] = d3;
                encoded[output++] = d4;
            }
            return encoded;
        }

        public static byte[] HammingDecode(byte[] encoded, out int corrections)
        {
            if (encoded == null || encoded.Length != EccBits)
                throw new ArgumentException("ECC sequence must contain 392 bits", "encoded");
            byte[] data = new byte[DataBits];
            int output = 0;
            corrections = 0;
            for (int offset = 0; offset < encoded.Length; offset += 7)
            {
                byte[] word = new byte[7];
                for (int i = 0; i < 7; i++)
                {
                    if (encoded[offset + i] != 0 && encoded[offset + i] != 1)
                        throw new ArgumentException("ECC bits must be zero or one", "encoded");
                    word[i] = encoded[offset + i];
                }
                int syndrome = (word[0] ^ word[2] ^ word[4] ^ word[6])
                    | ((word[1] ^ word[2] ^ word[5] ^ word[6]) << 1)
                    | ((word[3] ^ word[4] ^ word[5] ^ word[6]) << 2);
                if (syndrome != 0)
                {
                    word[syndrome - 1] ^= 1;
                    corrections++;
                }
                data[output++] = word[2];
                data[output++] = word[4];
                data[output++] = word[5];
                data[output++] = word[6];
            }
            return ProvenanceWatermarkProtocol.FromBits(data);
        }

        public static void CarrierLayout(out int[] cells, out sbyte[] polarity)
        {
            if (GridColumns * GridRows != EccBits)
                throw new InvalidOperationException("carrier grid/ECC size mismatch");
            uint seed;
            using (var sha = SHA256.Create())
            {
                byte[] hash = sha.ComputeHash(CarrierKey);
                seed = ((uint)hash[0] << 24) | ((uint)hash[1] << 16)
                    | ((uint)hash[2] << 8) | hash[3];
            }
            var random = new XorShift32(seed);
            cells = new int[EccBits];
            for (int i = 0; i < cells.Length; i++) cells[i] = i;
            for (int i = cells.Length - 1; i > 0; i--)
            {
                int other = (int)(random.Next() % (uint)(i + 1));
                int swap = cells[i];
                cells[i] = cells[other];
                cells[other] = swap;
            }
            polarity = new sbyte[EccBits];
            for (int i = 0; i < polarity.Length; i++)
                polarity[i] = (random.Next() & 1) != 0 ? (sbyte)1 : (sbyte)-1;
        }

        public static sbyte[] CarrierSigns(byte[] payload)
        {
            byte[] encoded = HammingEncode(payload);
            int[] cells;
            sbyte[] polarity;
            CarrierLayout(out cells, out polarity);
            sbyte[] signs = new sbyte[EccBits];
            for (int i = 0; i < encoded.Length; i++)
                signs[cells[i]] = (sbyte)((encoded[i] != 0 ? 1 : -1) * polarity[i]);
            return signs;
        }

        public static byte[] RecoverEccBits(sbyte[] cellSigns)
        {
            if (cellSigns == null || cellSigns.Length != EccBits)
                throw new ArgumentException("carrier decisions must contain 392 signs", "cellSigns");
            int[] cells;
            sbyte[] polarity;
            CarrierLayout(out cells, out polarity);
            byte[] encoded = new byte[EccBits];
            for (int i = 0; i < encoded.Length; i++)
            {
                sbyte sign = cellSigns[cells[i]];
                if (sign != -1 && sign != 1)
                    throw new ArgumentException("carrier signs must be -1 or +1", "cellSigns");
                encoded[i] = (byte)(sign * polarity[i] > 0 ? 1 : 0);
            }
            return encoded;
        }

        public static byte[] BuildTileRgba(byte[] payload)
        {
            sbyte[] signs = CarrierSigns(payload);
            byte[] pixels = new byte[TileWidth * TileHeight * 4];
            for (int y = 0; y < TileHeight; y++)
            {
                int cellY = y / CellSize;
                int localY = y % CellSize;
                for (int x = 0; x < TileWidth; x++)
                {
                    int cellX = x / CellSize;
                    int localX = x % CellSize;
                    int checker = (((localX / 2) + (localY / 2)) & 1) == 0 ? 1 : -1;
                    byte value = signs[cellY * GridColumns + cellX] * checker > 0
                        ? (byte)255 : (byte)0;
                    int offset = (y * TileWidth + x) * 4;
                    pixels[offset] = value;
                    pixels[offset + 1] = value;
                    pixels[offset + 2] = value;
                    pixels[offset + 3] = OverlayAlpha;
                }
            }
            return pixels;
        }
    }
}
