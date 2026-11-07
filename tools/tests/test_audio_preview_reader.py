import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audio_preview_reader import extract, extract_media


class AudioPreviewTests(unittest.TestCase):
    def test_reads_only_the_requested_media_from_the_bank(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            first, second = b"RIFFfirst-audio", b"RIFFsecond-audio"
            index = struct.pack("<IIIIII", 12, 0, len(first), 99, len(first), len(second))
            bank = root / "sound.bnk"
            bank.write_bytes(b"DIDX" + struct.pack("<I", len(index)) + index
                             + b"DATA" + struct.pack("<I", len(first + second)) + first + second)
            original = bank.read_bytes()
            output = root / "selected.wem"
            extract_media(bank, 99, output)
            self.assertEqual(output.read_bytes(), second)
            self.assertEqual(bank.read_bytes(), original)

    def test_rejects_a_media_range_outside_the_data_chunk(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            bank = root / "sound.bnk"
            bank.write_bytes(b"DIDX" + struct.pack("<I", 12) + struct.pack("<III", 99, 0, 1000)
                             + b"DATA" + struct.pack("<I", 4) + b"RIFF")
            output = root / "selected.wem"
            with self.assertRaisesRegex(ValueError, "范围"):
                extract_media(bank, 99, output)
            self.assertFalse(output.exists())

    def test_dynamic_event_does_not_silently_choose_one_source(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            game = root / "game"
            banks = game / "Mortal_Data/StreamingAssets/Audio/GeneratedSoundBanks/Windows"
            banks.mkdir(parents=True)
            (banks / "SoundbanksInfo.xml").write_text(
                '<SoundBanksInfo><SoundBanks><SoundBank><ShortName>SFX</ShortName>'
                '<IncludedEvents><Event Name="Sound_Test"><IncludedMemoryFiles>'
                '<File Id="1"/><File Id="2"/></IncludedMemoryFiles></Event></IncludedEvents>'
                '</SoundBank></SoundBanks></SoundBanksInfo>', encoding="utf-8")
            with patch("audio_preview_reader.audio_key", return_value="Test"):
                with self.assertRaisesRegex(ValueError, "动态混音"):
                    extract(game, "sound", "test", root / "selected.wem")
            self.assertFalse((root / "selected.wem").exists())


if __name__ == "__main__":
    unittest.main()
