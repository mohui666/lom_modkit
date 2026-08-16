# -*- coding: utf-8 -*-
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

from app_version import EDITOR_VERSION


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "build-windows.ps1"


class WindowsReleasePackagingTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if os.name != "nt":
            raise unittest.SkipTest("Windows release packager only runs on Windows")
        cls.powershell = shutil.which("pwsh") or shutil.which("powershell")
        if not cls.powershell:
            raise unittest.SkipTest("PowerShell is unavailable")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.bundle = self.root / "lom_modkit"
        self.output = self.root / "release"
        required = {
            "lom_editor.exe": b"editor-v1",
            "story_api_cli.exe": b"cli-v1",
            "_internal/runtime/MortalModHost.dll": b"runtime-v1",
            "_internal/runtime/NVorbis.dll": b"nvorbis-v1",
            "_internal/assets/doorstop/win-x86-doorstop.dll": b"doorstop-v1",
            "_internal/data/editor_data.json": b"{}",
            "_internal/data/preview_map.json": b"{}",
        }
        for name, data in required.items():
            target = self.bundle / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)

    def tearDown(self):
        self.temp.cleanup()

    def run_packager(
        self, *extra: str, output_directory: Path | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                self.powershell,
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(SCRIPT),
                "-Version",
                EDITOR_VERSION,
                "-BundleDirectory",
                str(self.bundle),
                "-OutputDirectory",
                str(output_directory or self.output),
                *extra,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )

    @property
    def archive(self) -> Path:
        return self.output / f"lom_modkit-v{EDITOR_VERSION}_windows_x64.zip"

    def test_packages_only_bundle_and_writes_matching_checksum(self):
        result = self.run_packager()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        checksum = Path(str(self.archive) + ".sha256")
        self.assertTrue(self.archive.is_file())
        self.assertTrue(checksum.is_file())
        expected = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.assertEqual(
            checksum.read_text(encoding="utf-8"),
            f"{expected}  {self.archive.name}\n",
        )
        with zipfile.ZipFile(self.archive) as package:
            names = set(package.namelist())
        self.assertIn("lom_modkit/lom_editor.exe", names)
        self.assertIn("lom_modkit/_internal/runtime/MortalModHost.dll", names)
        self.assertTrue(all(name.startswith("lom_modkit/") for name in names))

    def test_refuses_overwrite_by_default_and_force_replaces_atomically(self):
        first = self.run_packager()
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        original = self.archive.read_bytes()
        refused = self.run_packager()
        self.assertNotEqual(refused.returncode, 0)
        self.assertEqual(self.archive.read_bytes(), original)

        (self.bundle / "lom_editor.exe").write_bytes(b"editor-v2")
        replaced = self.run_packager("-Force")
        self.assertEqual(replaced.returncode, 0, replaced.stdout + replaced.stderr)
        self.assertNotEqual(self.archive.read_bytes(), original)
        digest = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.assertEqual(
            Path(str(self.archive) + ".sha256").read_text(encoding="utf-8"),
            f"{digest}  {self.archive.name}\n",
        )

    def test_rejects_user_packages_and_build_cache(self):
        (self.bundle / "leaked.lommod").write_bytes(b"private mod")
        result = self.run_packager()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.archive.exists())
        (self.bundle / "leaked.lommod").unlink()
        cache = self.bundle / "__pycache__"
        cache.mkdir()
        (cache / "secret.pyc").write_bytes(b"cache")
        result = self.run_packager()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.archive.exists())

    def test_rejects_wrong_version_and_incomplete_bundle(self):
        wrong = subprocess.run(
            [
                self.powershell,
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(SCRIPT),
                "-Version",
                "9.9.9",
                "-BundleDirectory",
                str(self.bundle),
                "-OutputDirectory",
                str(self.output),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
        self.assertNotEqual(wrong.returncode, 0)
        self.assertFalse(self.archive.exists())

        (self.bundle / "_internal/runtime/MortalModHost.dll").unlink()
        incomplete = self.run_packager()
        self.assertNotEqual(incomplete.returncode, 0)
        self.assertFalse(self.archive.exists())

    def test_rejects_output_inside_bundle_and_junction(self):
        inside = self.run_packager(output_directory=self.bundle / "release")
        self.assertNotEqual(inside.returncode, 0)

        outside = self.root / "outside"
        outside.mkdir()
        (outside / "private.txt").write_text("do not package", encoding="utf-8")
        link = self.bundle / "linked-outside"
        created = subprocess.run(
            ["cmd.exe", "/d", "/c", "mklink", "/J", str(link), str(outside)],
            capture_output=True,
            text=True,
            check=False,
        )
        if created.returncode != 0:
            self.skipTest("This Windows host cannot create a test junction")
        try:
            escaped = self.run_packager()
            self.assertNotEqual(escaped.returncode, 0)
            self.assertFalse(self.archive.exists())
        finally:
            os.rmdir(link)


if __name__ == "__main__":
    unittest.main()
