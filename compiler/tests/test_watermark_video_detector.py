# -*- coding: utf-8 -*-
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
from PIL import Image

from lomc.errors import LomcError
from lomc.watermark_codec import TILE_HEIGHT, TILE_WIDTH, tile_rgba
from lomc.watermark_protocol import encode_packet, mod_id_hash
from lomc.watermark_video_detector import (
    _extract_frames,
    detect_video,
    detect_video_frames,
)


class WatermarkVideoDetectorTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        width, height = TILE_WIDTH * 2, TILE_HEIGHT * 2
        rgba = np.frombuffer(tile_rgba(encode_packet("demo_mod")), dtype=np.uint8)
        carrier = np.tile(
            rgba.reshape(TILE_HEIGHT, TILE_WIDTH, 4), (2, 2, 1)
        ).astype(np.float32)
        alpha = carrier[:, :, 3:4] / 255.0
        cls.marked = []
        cls.clean = []
        y, x = np.mgrid[:height, :width]
        for index in range(4):
            random = np.random.default_rng(9000 + index)
            scene = (
                118
                + 38 * np.sin((x + index * 41) / 59.0)
                + 31 * np.cos((y - index * 27) / 43.0)
                + 18 * np.sin((x + y + index * 19) / 35.0)
                + random.normal(0, 2.0, size=(height, width))
            )
            rgb = np.stack((scene + 8, scene, scene - 7), axis=2)
            clean = np.clip(rgb, 0, 255).astype(np.uint8)
            marked = np.clip(
                rgb * (1.0 - alpha) + carrier[:, :, :3] * alpha, 0, 255
            ).astype(np.uint8)
            clean_path = cls.root / ("clean-%02d.png" % index)
            marked_path = cls.root / ("marked-%02d.png" % index)
            Image.fromarray(clean, "RGB").save(clean_path)
            Image.fromarray(marked, "RGB").save(marked_path)
            cls.clean.append(clean_path)
            cls.marked.append(marked_path)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_multi_frame_correlation_recovers_identity(self):
        result = detect_video_frames(self.marked, interval=2.0, scale_factors=(1.0,))
        self.assertTrue(result.detected, result)
        self.assertEqual(result.frames_sampled, 4)
        self.assertEqual(result.mod_hash, mod_id_hash("demo_mod").hex().upper())
        self.assertEqual(result.checksum_status, "valid")
        self.assertIn(result.ecc_status, ("clean", "corrected"))
        self.assertIn("correlation", result.method)

    def test_multi_frame_negative(self):
        result = detect_video_frames(self.clean, interval=2.0, scale_factors=(1.0,))
        self.assertFalse(result.detected, result)
        self.assertIsNone(result.mod_hash)

    def test_ffmpeg_command_is_bounded_and_argument_safe(self):
        output = self.root / "extract"
        output.mkdir(exist_ok=True)
        video = self.root / "fixture.mp4"
        video.write_bytes(b"not a real video")

        def fake_run(command, **kwargs):
            self.assertEqual(command[0], "trusted-ffmpeg")
            self.assertEqual(command[command.index("-vf") + 1], "fps=1/2")
            self.assertEqual(command[command.index("-frames:v") + 1], "3")
            (output / "frame-00001.png").write_bytes(b"fixture")
            return type("Completed", (), {"returncode": 0, "stdout": "", "stderr": ""})()

        with patch("lomc.watermark_video_detector.subprocess.run", side_effect=fake_run):
            frames = _extract_frames(video, output, "trusted-ffmpeg", 2.0, 3)
        self.assertEqual(frames, [output / "frame-00001.png"])

    def test_missing_ffmpeg_is_explicit(self):
        video = self.root / "missing-tool.mp4"
        video.write_bytes(b"fixture")
        with self.assertRaisesRegex(LomcError, "FFmpeg"):
            detect_video(video, ffmpeg=str(self.root / "definitely-missing.exe"))


if __name__ == "__main__":
    unittest.main()
