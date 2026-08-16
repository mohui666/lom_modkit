# -*- coding: utf-8 -*-
import unittest

from lomc.errors import LomcError
from lomc.watermark_protocol import (
    MOD_ID_HASH_SIZE,
    PAYLOAD_SIZE,
    PROTOCOL_VERSION,
    bits_to_packet,
    decode_packet,
    encode_packet,
    mod_id_hash,
    packet_to_bits,
    parse_packet,
)


GOLDEN_PACKET = "4C4F4D5701010000720435D441F942141A10BE8AA833C8741C08EE6D"
GOLDEN_MOD_HASH = "720435D441F942141A10BE8AA833C874"


class WatermarkProtocolTest(unittest.TestCase):
    def test_cross_language_golden_vector(self):
        packet = encode_packet("demo_mod", algorithm_version=1)
        self.assertEqual(packet.hex().upper(), GOLDEN_PACKET)
        self.assertEqual(mod_id_hash("demo_mod").hex().upper(), GOLDEN_MOD_HASH)
        self.assertEqual(len(packet), PAYLOAD_SIZE)
        decoded = decode_packet(packet)
        self.assertEqual(decoded.protocol_version, PROTOCOL_VERSION)
        self.assertEqual(decoded.algorithm_version, 1)
        self.assertEqual(decoded.mod_id_hash_hex, GOLDEN_MOD_HASH)
        self.assertTrue(decoded.checksum_valid)
        self.assertEqual(len(decoded.mod_id_hash), MOD_ID_HASH_SIZE)

    def test_msb_bit_order_roundtrip(self):
        packet = encode_packet("demo_mod", 7)
        bits = packet_to_bits(packet)
        self.assertEqual(len(bits), PAYLOAD_SIZE * 8)
        self.assertEqual(bits[:8], (0, 1, 0, 0, 1, 1, 0, 0))  # ASCII L
        self.assertEqual(bits_to_packet(bits), packet)

    def test_structural_parse_retains_bad_crc_for_detector(self):
        packet = bytearray(encode_packet("demo_mod"))
        packet[12] ^= 1
        parsed = parse_packet(packet)
        self.assertFalse(parsed.checksum_valid)
        with self.assertRaises(LomcError):
            decode_packet(packet)

    def test_rejects_unknown_structure_and_invalid_inputs(self):
        for bad_id in ("", "Official.Mod", "../evil", "a" * 65):
            with self.subTest(mod_id=bad_id), self.assertRaises(LomcError):
                encode_packet(bad_id)
        for version in (0, 256, True, "1"):
            with self.subTest(version=version), self.assertRaises(LomcError):
                encode_packet("demo_mod", version)
        bad = bytearray(encode_packet("demo_mod"))
        bad[0] = 0
        with self.assertRaises(LomcError):
            parse_packet(bad)
        with self.assertRaises(LomcError):
            bits_to_packet([0] * (PAYLOAD_SIZE * 8 - 1))


if __name__ == "__main__":
    unittest.main()
