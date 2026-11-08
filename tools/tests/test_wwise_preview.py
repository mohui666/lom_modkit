import math
import random
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audio_preview_reader import extract
from wwise_preview import random_choices, playback_parameters


def pack(fmt, *values):
    return struct.pack('<' + fmt, *values)


def parameters(parent, volume=0, pitch=0):
    # Independent synthetic Wwise 140 bank: no game metadata or samples.
    props = [(key, value) for key, value in [(0, volume), (2, pitch)] if value]
    return (pack('BBBBBII', 0, 0, 0, 0, 0, 0, parent) + b'\0'
            + pack('B', len(props)) + bytes(key for key, _ in props)
            + b''.join(pack('f', value) for _, value in props)
            + b'\0\0\0' + pack('I', 0) + pack('BBHBB', 0, 1, 0, 0, 0)
            + pack('BBH', 0, 0, 0))


def synthetic_bank(path, continuous=False, actions=1):
    first = b'RIFF' + pack('I', 40) + b'WAVEfmt ' + pack('IHHIIHH', 16, 1, 1, 16000, 32000, 2, 16) + b'data' + pack('I', 4) + b'\1\0\2\0'
    second = first[:-4] + b'\3\0\4\0'
    records = [
        (4, pack('IB', 12, actions) + pack('I', 13) * actions),
        (3, pack('IHI', 13, 0x0403, 11) + pack('BBBBI', 0, 0, 0, 4, 99)),
        (5, pack('I', 11) + parameters(20, volume=-6)
         + pack('HHHfffHBBBB', 1, 0, 0, 1000, 0, 0, 1, 0, 0, 0, 1 | (8 if continuous else 0))
         + pack('IIIH', 2, 100, 101, 2) + pack('IiIi', 100, 1, 101, 3)),
        (7, pack('I', 20) + parameters(0, volume=3) + pack('II', 1, 11)),
        (2, pack('IIBIIB', 100, 65537, 0, 7, len(first), 0) + parameters(11, pitch=200)),
        (2, pack('IIBIIB', 101, 65537, 0, 8, len(second), 0) + parameters(11, pitch=-100)),
    ]
    hirc = pack('I', len(records)) + b''.join(pack('BI', kind, len(payload)) + payload for kind, payload in records)
    didx = pack('IIIIII', 7, 0, len(first), 8, len(first), len(second))
    path.write_bytes(b'BKHD' + pack('II', 4, 140) + b'DIDX' + pack('I', len(didx)) + didx
                     + b'DATA' + pack('I', len(first + second)) + first + second
                     + b'HIRC' + pack('I', len(hirc)) + hirc)
    return first, second


class RandomPreviewTests(unittest.TestCase):
    def test_branches_keep_weights_parent_gain_and_per_sound_pitch(self):
        with tempfile.TemporaryDirectory() as folder:
            bank = Path(folder) / 'test.bnk'
            synthetic_bank(bank)
            original = bank.read_bytes()
            choices = random_choices(bank, 12, {7, 8})
            self.assertEqual([c['media_id'] for c in choices], [7, 8])
            self.assertEqual([c['weight'] for c in choices], [1, 3])
            self.assertEqual(playback_parameters(choices[0], random.Random(1)), (-3, 200))
            self.assertEqual(playback_parameters(choices[1], random.Random(1)), (-3, -100))
            self.assertEqual(bank.read_bytes(), original)

    def test_extract_follows_the_event_and_writes_a_decoder_playback_config(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            banks = root / 'game/Mortal_Data/StreamingAssets/Audio/GeneratedSoundBanks/Windows'
            banks.mkdir(parents=True)
            first, _ = synthetic_bank(banks / 'SFX.bnk')
            (banks / 'SoundbanksInfo.xml').write_text(
                '<SoundBanksInfo><SoundBanks><SoundBank><ShortName>SFX</ShortName>'
                '<IncludedEvents><Event Id="12" Name="Sound_Test"><IncludedMemoryFiles>'
                '<File Id="7"/><File Id="8"/></IncludedMemoryFiles></Event></IncludedEvents>'
                '</SoundBank></SoundBanks></SoundBanksInfo>', encoding='utf-8')
            with patch('audio_preview_reader.audio_key', return_value='Test'), patch('audio_preview_reader.random.choices', side_effect=lambda choices, weights: [choices[0]]) as choose:
                output = root / 'preview.wem'
                result = extract(root / 'game', 'sound', 'test', output)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(result['variant_count'], 2)
            self.assertEqual(result['pitch_cents'], 200)
            self.assertEqual(result['volume_db'], -3)
            choose.assert_called_once()
            config = output.with_suffix('.txtp').read_text(encoding='utf-8')
            self.assertIn(f'#h {round(16000 * 2 ** (200 / 1200))}', config)
            self.assertIn(f'#v {math.pow(10, -3 / 20):.9f}', config)

    def test_unhandled_continuous_layered_or_inconsistent_events_stay_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            bank = Path(folder) / 'test.bnk'
            for continuous, actions in [(True, 1), (False, 2)]:
                synthetic_bank(bank, continuous=continuous, actions=actions)
                with self.assertRaisesRegex(ValueError, '动态播放规则'):
                    random_choices(bank, 12, {7, 8})
            synthetic_bank(bank)
            with self.assertRaisesRegex(ValueError, '动态播放规则'):
                random_choices(bank, 12, {7, 9})


if __name__ == '__main__':
    unittest.main()
