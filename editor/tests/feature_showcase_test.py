# -*- coding: utf-8 -*-
from pathlib import Path
import json
import sys
import tempfile
import unittest
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[2]
COMPILER = ROOT / "compiler"
sys.path.insert(0, str(COMPILER))

from lomc import pack_mod, validate_manifest, validate_story
from lomc.content import collect_story_content_refs, load_content_metadata


class FeatureShowcaseTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sample = ROOT / "samples" / "showcase3"
        cls.manifest = json.loads((cls.sample / "manifest.json").read_text(encoding="utf-8"))
        cls.stories = [
            json.loads(path.read_text(encoding="utf-8"))
            for path in sorted((cls.sample / "story").glob("*.json"))
        ]
        cls.story = next(story for story in cls.stories if story["id"] == "main")
        cls.nodes = [node for story in cls.stories for node in story["nodes"]]

    def test_story_uses_every_showcase_capability_with_real_schema_nodes(self):
        validate_manifest(self.manifest)
        for story in self.stories:
            validate_story(story)
        nodes = self.nodes
        node_types = {node["type"] for node in nodes}
        self.assertTrue({
            "background", "music", "overlay", "show", "say", "sound", "choice",
            "flag", "game_flag", "branch", "custom_cg", "goto_scene",
        }.issubset(node_types))
        characters = {node.get("character") for node in nodes}
        self.assertIn("player", characters)
        self.assertIn("user:showcase.lin_deng", characters)
        character_meta = load_content_metadata(
            str(self.sample / "assets/user/character/showcase.lin_deng/content.json")
        )
        self.assertTrue({"normal", "happy"}.issubset(character_meta["portraits"]))
        self.assertTrue(any(node.get("voice") == "user:showcase.lin_greeting" for node in nodes))
        self.assertTrue(any(node.get("source") == "mod" for node in nodes))
        ending = next(node for node in nodes if node["type"] == "goto_scene")
        self.assertEqual(ending["scene"], "Free")

    def test_all_user_references_have_valid_metadata_and_committed_files(self):
        refs = [ref for story in self.stories for ref in collect_story_content_refs(story)]
        identities = {(item["expected_type"], item["ref"].content_id) for item in refs}
        self.assertEqual(identities, {
            ("character", "showcase.lin_deng"),
            ("audio", "showcase.lin_greeting"),
            ("audio", "showcase.lantern_theme"),
            ("audio", "showcase.lantern_chime"),
            ("image", "showcase.courier_station"),
            ("image", "showcase.departure_cg"),
            ("image", "showcase.lantern_overlay"),
        })
        for content_type, content_id in identities:
            folder = self.sample / "assets" / "user" / content_type / content_id
            metadata = load_content_metadata(str(folder / "content.json"))
            self.assertEqual((metadata["type"], metadata["id"]), (content_type, content_id))
            for filename in metadata["files"].values():
                self.assertTrue((folder / filename).is_file())

    def test_transparent_sprite_assets_and_pcm_audio_are_real_files(self):
        for relative in (
            "assets/user/character/showcase.lin_deng/normal.png",
            "assets/user/character/showcase.lin_deng/happy.png",
            "assets/user/image/showcase.lantern_overlay/lantern_overlay.png",
        ):
            payload = (self.sample / relative).read_bytes()
            self.assertEqual(payload[:8], b"\x89PNG\r\n\x1a\n")
            self.assertIn(payload[25], (4, 6), relative)  # PNG grayscale/RGB + alpha
            self.assertLess(len(payload), 8 * 1024 * 1024)
        for relative in (
            "assets/user/audio/showcase.lantern_theme/lantern_theme.wav",
            "assets/user/audio/showcase.lantern_chime/lantern_chime.wav",
            "assets/user/audio/showcase.lin_greeting/lin_greeting.wav",
        ):
            with wave.open(str(self.sample / relative), "rb") as audio:
                self.assertEqual((audio.getnchannels(), audio.getsampwidth()), (1, 2))
                self.assertGreater(audio.getnframes(), 1000)

    def test_pack_is_self_contained_and_does_not_write_into_sample(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "showcase3.lommod"
            pack_mod(str(self.sample), output=str(output))
            with zipfile.ZipFile(output) as archive:
                names = set(archive.namelist())
                self.assertIn("lua/main.lua", names)
                self.assertIn("lua/gameplay.lua", names)
                self.assertIn("story/battle_demo.json", names)
                self.assertIn(
                    "assets/user/character/showcase.lin_deng/normal.png", names
                )
                self.assertIn(
                    "assets/user/audio/showcase.lin_greeting/lin_greeting.wav", names
                )
                self.assertIn(
                    "assets/user/image/showcase.departure_cg/departure_cg.png", names
                )
        forbidden = {".lommod", ".tmp", ".bak"}
        self.assertFalse(any(path.suffix.lower() in forbidden for path in self.sample.rglob("*")))


if __name__ == "__main__":
    unittest.main()
