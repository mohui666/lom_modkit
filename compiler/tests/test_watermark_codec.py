# -*- coding: utf-8 -*-
import hashlib
import unittest

from lomc.errors import LomcError
from lomc.watermark_codec import (
    ECC_BITS,
    TILE_HEIGHT,
    TILE_WIDTH,
    carrier_layout,
    carrier_signs,
    hamming_decode,
    hamming_encode,
    recover_ecc_bits,
    tile_rgba,
)
from lomc.watermark_protocol import decode_packet, encode_packet


class WatermarkCodecTest(unittest.TestCase):
    def test_hamming_corrects_one_bit_per_word(self):
        packet = encode_packet("demo_mod")
        encoded = list(hamming_encode(packet))
        for offset in range(0, len(encoded), 7):
            encoded[offset + (offset // 7) % 7] ^= 1
        decoded, corrections = hamming_decode(encoded)
        self.assertEqual(decoded, packet)
        self.assertEqual(corrections, ECC_BITS // 7)
        self.assertTrue(decode_packet(decoded).checksum_valid)

    def test_keyed_carrier_mapping_roundtrip_and_golden(self):
        packet = encode_packet("demo_mod")
        cells, polarity = carrier_layout()
        self.assertEqual(len(set(cells)), ECC_BITS)
        self.assertEqual(set(polarity), {-1, 1})
        self.assertEqual(cells[:8], (388, 301, 111, 85, 164, 305, 22, 72))
        signs = carrier_signs(packet)
        recovered = recover_ecc_bits(signs)
        decoded, corrections = hamming_decode(recovered)
        self.assertEqual(decoded, packet)
        self.assertEqual(corrections, 0)

    def test_tile_is_balanced_deterministic_mid_frequency_rgba(self):
        packet = encode_packet("demo_mod")
        first = tile_rgba(packet)
        second = tile_rgba(packet)
        self.assertEqual(first, second)
        self.assertEqual(len(first), TILE_WIDTH * TILE_HEIGHT * 4)
        self.assertEqual(
            hashlib.sha256(first).hexdigest().upper(),
            "D075861FB031C39D390AD27C45C4FF3B858E7804CC0BC8510E3B75D5AA68831C",
        )
        self.assertEqual(set(first[3::4]), {4})
        self.assertEqual(first[0:16:4], bytes((255, 255, 0, 0)))

    def test_rejects_wrong_sizes(self):
        with self.assertRaises(LomcError):
            hamming_encode(b"short")
        with self.assertRaises(LomcError):
            hamming_decode([0] * (ECC_BITS - 1))
        with self.assertRaises(LomcError):
            recover_ecc_bits([1] * (ECC_BITS - 1))


if __name__ == "__main__":
    unittest.main()
