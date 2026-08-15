# -*- coding: utf-8 -*-
import json
import io
from pathlib import Path
import tempfile
import unittest
from contextlib import redirect_stdout

import numpy as np
from PIL import Image, ImageEnhance

from lomc.watermark_codec import TILE_HEIGHT, TILE_WIDTH, tile_rgba
from lomc.watermark_detector import detect_image
from lomc.watermark_protocol import encode_packet, mod_id_hash
from lomc.__main__ import main as lomc_main


class WatermarkDetectorCorpusTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        width, height = TILE_WIDTH * 2, TILE_HEIGHT * 2
        y, x = np.mgrid[:height, :width]
        random = np.random.default_rng(20260816)
        base_luma = (
            112
            + 36 * np.sin(x / 67.0)
            + 28 * np.cos(y / 49.0)
            + 14 * np.sin((x + y) / 31.0)
            + random.normal(0, 1.2, size=(height, width))
        )
        base = np.empty((height, width, 3), dtype=np.float32)
        base[:, :, 0] = base_luma + 12 * np.sin(y / 23.0)
        base[:, :, 1] = base_luma
        base[:, :, 2] = base_luma - 10 * np.cos(x / 29.0)
        clean = np.clip(base, 0, 255).astype(np.uint8)

        rgba = np.frombuffer(tile_rgba(encode_packet("demo_mod")), dtype=np.uint8)
        tile = rgba.reshape(TILE_HEIGHT, TILE_WIDTH, 4)
        carrier = np.tile(tile, (2, 2, 1)).astype(np.float32)
        alpha = carrier[:, :, 3:4] / 255.0
        marked = np.clip(base * (1.0 - alpha) + carrier[:, :, :3] * alpha, 0, 255)
        marked_image = Image.fromarray(marked.astype(np.uint8), "RGB")

        cls.paths = {}
        cls.paths["original"] = cls.root / "original.png"
        marked_image.save(cls.paths["original"])
        cls.paths["jpeg"] = cls.root / "jpeg.jpg"
        marked_image.save(cls.paths["jpeg"], quality=85, subsampling=1)
        cls.paths["resize"] = cls.root / "resize.png"
        marked_image.resize(
            (round(width * 0.75), round(height * 0.75)),
            Image.Resampling.BICUBIC,
        ).save(cls.paths["resize"])
        cls.paths["mild_crop"] = cls.root / "mild_crop.png"
        marked_image.crop((37, 23, width - 29, height - 17)).save(
            cls.paths["mild_crop"]
        )
        cls.paths["brightness"] = cls.root / "brightness.png"
        ImageEnhance.Brightness(marked_image).enhance(1.12).save(
            cls.paths["brightness"]
        )
        cls.paths["contrast"] = cls.root / "contrast.png"
        ImageEnhance.Contrast(marked_image).enhance(0.85).save(cls.paths["contrast"])
        cls.paths["clean_negative"] = cls.root / "clean_negative.png"
        Image.fromarray(clean, "RGB").save(cls.paths["clean_negative"])

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def _assert_detected(self, case, scale=1.0):
        result = detect_image(self.paths[case], scale_factors=(scale,))
        self.assertTrue(result.detected, (case, result))
        self.assertGreater(result.confidence, 0.0)
        self.assertEqual(result.protocol_version, 1)
        self.assertEqual(result.algorithm_version, 1)
        self.assertEqual(result.mod_hash, mod_id_hash("demo_mod").hex().upper())
        self.assertEqual(result.checksum_status, "valid")
        self.assertIn(result.ecc_status, ("clean", "corrected"))
        self.assertIsInstance(json.loads(result.to_json()), dict)

    def test_original_png(self):
        self._assert_detected("original")

    def test_jpeg_compression(self):
        self._assert_detected("jpeg")

    def test_resize(self):
        self._assert_detected("resize", 0.75)

    def test_mild_crop(self):
        self._assert_detected("mild_crop")

    def test_brightness(self):
        self._assert_detected("brightness")

    def test_contrast(self):
        self._assert_detected("contrast")

    def test_clean_image_is_not_detected(self):
        result = detect_image(self.paths["clean_negative"], scale_factors=(1.0,))
        self.assertFalse(result.detected, result)
        self.assertIsNone(result.mod_hash)
        self.assertNotEqual(result.checksum_status, "valid")

    def test_cli_json_contract(self):
        output = io.StringIO()
        with redirect_stdout(output):
            exit_code = lomc_main(
                ["detect-watermark", str(self.paths["original"]), "--json"]
            )
        self.assertEqual(exit_code, 0)
        payload = json.loads(output.getvalue())
        self.assertTrue(payload["detected"])
        self.assertEqual(payload["protocol_version"], 1)
        self.assertEqual(payload["algorithm_version"], 1)
        self.assertEqual(payload["checksum_status"], "valid")
        self.assertIn(payload["ecc_status"], ("clean", "corrected"))


if __name__ == "__main__":
    unittest.main()
