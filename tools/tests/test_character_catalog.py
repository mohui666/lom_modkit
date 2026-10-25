"""Synthetic serialized configuration, without any official game data."""
from pathlib import Path
from types import SimpleNamespace
import struct
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import _probe_assets as probe


def string(value):
    encoded = value.encode("utf-8")
    raw = struct.pack("<i", len(encoded)) + encoded
    return raw + b"\0" * (-len(raw) % 4)


def header(name):
    return b"\0" * 28 + string(name)


def pointer(path_id):
    return struct.pack("<iq", 0, path_id)


def mapping(name, value):
    return header(name) + string(name) + string(value)


class CharacterCatalogTest(unittest.TestCase):
    def test_duplicate_expression_uses_first_address_like_game_load_portrait(self):
        character = (header("Fixture character") + struct.pack("<2f", 0, 0)
                     + pointer(2) + struct.pack("<i", 4)
                     + pointer(3) + string("test/normal.png")
                     + pointer(4) + string("test/laugh-first.png")
                     + pointer(4) + string("test/laugh-later.png")
                     + pointer(5) + string("test/unused-expression.png"))
        raws = {
            1: header("StoryCharacterConfig") + struct.pack("<i", 1) + pointer(6),
            2: mapping("character", "fixture"),
            3: mapping("normal", "normal"),
            4: mapping("laugh", "laugh2"),
            5: mapping("unused", "special99"),
            6: character,
        }
        objects = [SimpleNamespace(type=SimpleNamespace(name="MonoBehaviour"),
                                   path_id=pid, get_raw_data=lambda raw=raw: raw)
                   for pid, raw in raws.items()]
        with patch.object(probe.UnityPy, "load", return_value=SimpleNamespace(objects=objects)):
            characters, _ = probe.load_sa2()
        self.assertEqual(characters["fixture"]["first"], "test/normal.png")
        self.assertEqual(characters["fixture"]["portraits"], {
            "normal": "test/normal.png",
            "laugh2": "test/laugh-first.png",
            "special99": "test/unused-expression.png",
        })


if __name__ == "__main__":
    unittest.main()
