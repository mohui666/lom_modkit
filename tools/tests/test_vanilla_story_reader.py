import struct
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from vanilla_story_reader import translations


def language_environment(entries):
    def string(value):
        encoded = value.encode("utf-8")
        data = struct.pack("<i", len(encoded)) + encoded
        return data + b"\0" * (-len(data) % 4)

    raw = b"\0" * 28 + string("Language") + b"\0" * 12
    raw += string("ChineseSimplified") + string(",") + string("\n") + string("#")
    raw += struct.pack("<ii", 0, len(entries))
    raw += b"".join(string(key) + string(value) for key, value in entries.items())
    script = SimpleNamespace(m_ClassName="LeanLanguageCSV", m_Namespace="Lean.Localization")
    header = SimpleNamespace(m_Script=SimpleNamespace(read=lambda: script))
    asset = SimpleNamespace(type=SimpleNamespace(name="MonoBehaviour"),
                            read=lambda **kwargs: header, get_raw_data=lambda: raw)
    return SimpleNamespace(objects=[asset])


class VanillaStoryTranslationTests(unittest.TestCase):
    def test_dice_options_localize_base_keys_and_preserve_condition_suffixes(self):
        source = '''
options[1] = "O_HIGH|>90"
options[2] = 'O_LOW|<=20'
options[3] = "O_HIGH|>= 50|extra"
options[4] = "O_PLAIN"
'''
        environment = language_environment({
            "Story/O_HIGH": "通过检定", "Story/O_LOW": "未通过检定", "Story/O_PLAIN": "普通选项",
        })
        unity = SimpleNamespace(load=lambda path: environment)
        with patch.dict(sys.modules, {"UnityPy": unity}):
            result = translations(Path("game"), source)
        self.assertEqual(result["O_HIGH"], "通过检定")
        self.assertEqual(result["O_HIGH|>90"], "通过检定|>90")
        self.assertEqual(result["O_LOW|<=20"], "未通过检定|<=20")
        self.assertEqual(result["O_HIGH|>= 50|extra"], "通过检定|>= 50|extra")
        self.assertEqual(result["O_PLAIN"], "普通选项")

    def test_unknown_option_keys_stay_untranslated_and_dialogue_keys_still_load(self):
        source = 'say(luamanager.GetStoryText("N_TEST"))\noptions[1] = "O_MISSING|>90"'
        environment = language_environment({"Story/N_TEST": "原版对白"})
        unity = SimpleNamespace(load=lambda path: environment)
        with patch.dict(sys.modules, {"UnityPy": unity}):
            result = translations(Path("game"), source)
        self.assertEqual(result, {"N_TEST": "原版对白"})


if __name__ == "__main__":
    unittest.main()
