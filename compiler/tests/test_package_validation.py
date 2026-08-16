# -*- coding: utf-8 -*-
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lomc.package_validation import ArchiveValidationError, canonical_archive_name


class WindowsAmbiguousZipPathTest(unittest.TestCase):
    def test_rejects_ads_trailing_dot_space_and_dos_devices(self):
        for name in (
            "story/main.json:evil", "story/main. /x", "story/name. ",
            "CON", "con.txt", "assets/AUX.png", "x/COM1.json", "x/lpt9.bin",
        ):
            with self.subTest(name=name), self.assertRaises(ArchiveValidationError):
                canonical_archive_name(name)

    def test_similar_ordinary_names_remain_valid(self):
        for name in ("console.txt", "com10.json", "auxiliary.png", "story/main.json"):
            with self.subTest(name=name):
                self.assertEqual(canonical_archive_name(name), name)


if __name__ == "__main__":
    unittest.main()
