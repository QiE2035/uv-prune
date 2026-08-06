#!/usr/bin/env python3
"""Tests for make_wheel.py — run with `uv run --no-project python scripts/test_make_wheel.py`."""

from __future__ import annotations

import tempfile
import unittest
import zipfile
from pathlib import Path

from make_wheel import build_wheel, sha256_urlsafe

NAME = "uv-prune"
VERSION = "0.1.0"


class BuildWheelTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.binary = self.tmp / "fake-binary"
        self.payload = b"fake executable payload\x00\x01\x02"
        self.binary.write_bytes(self.payload)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _build(self, platform_tag: str) -> str:
        return build_wheel(
            binary=str(self.binary),
            name=NAME,
            version=VERSION,
            platform_tag=platform_tag,
            summary="test summary",
            project_url="https://example.com/uv-prune",
            out_dir=str(self.tmp),
        )

    def _read(self, wheel: str, entry: str) -> bytes:
        with zipfile.ZipFile(wheel) as zf:
            return zf.read(entry)

    def test_wheel_name_and_script_names(self) -> None:
        cases = {
            "win_amd64": "uv_prune-0.1.0.data/scripts/uv-prune.exe",
            "win_arm64": "uv_prune-0.1.0.data/scripts/uv-prune.exe",
            "musllinux_1_2_x86_64": "uv_prune-0.1.0.data/scripts/uv-prune",
            "musllinux_1_2_aarch64": "uv_prune-0.1.0.data/scripts/uv-prune",
            "macosx_11_0_arm64": "uv_prune-0.1.0.data/scripts/uv-prune",
            "macosx_10_12_x86_64": "uv_prune-0.1.0.data/scripts/uv-prune",
        }
        for tag, script_path in cases.items():
            with self.subTest(tag=tag):
                wheel = self._build(tag)
                self.assertEqual(Path(wheel).name, f"uv_prune-{VERSION}-py3-none-{tag}.whl")
                with zipfile.ZipFile(wheel) as zf:
                    self.assertIn(script_path, zf.namelist())
                    self.assertEqual(zf.read(script_path), self.payload)
                    self.assertEqual(zf.namelist().count(script_path), 1)

    def test_unix_script_has_executable_bit(self) -> None:
        wheel = self._build("musllinux_1_2_x86_64")
        with zipfile.ZipFile(wheel) as zf:
            info = zf.getinfo("uv_prune-0.1.0.data/scripts/uv-prune")
        self.assertEqual(info.external_attr >> 16, 0o100755)

    def test_record_matches_contents(self) -> None:
        wheel = self._build("win_amd64")
        with zipfile.ZipFile(wheel) as zf:
            names = zf.namelist()
            record = zf.read("uv_prune-0.1.0.dist-info/RECORD").decode()
        rows = {}
        for line in record.strip().splitlines():
            path, digest, size = line.split(",")
            rows[path] = (digest, size)
        self.assertEqual(len(rows), 4)  # METADATA, WHEEL, script, RECORD itself
        self.assertEqual(set(rows), set(names))
        with zipfile.ZipFile(wheel) as zf:
            for name in names:
                if name.endswith("RECORD"):
                    continue
                content = zf.read(name)
                digest, size = rows[name]
                self.assertEqual(digest, f"sha256={sha256_urlsafe(content)}", name)
                self.assertEqual(size, str(len(content)), name)

    def test_metadata_and_wheel_tags(self) -> None:
        wheel = self._build("macosx_11_0_arm64")
        wheel_meta = self._read(wheel, "uv_prune-0.1.0.dist-info/WHEEL").decode()
        self.assertIn("Tag: py3-none-macosx_11_0_arm64", wheel_meta)
        self.assertIn("Root-Is-Purelib: false", wheel_meta)
        metadata = self._read(wheel, "uv_prune-0.1.0.dist-info/METADATA").decode()
        self.assertIn("Name: uv-prune", metadata)
        self.assertIn("Version: 0.1.0", metadata)
        self.assertIn("Requires-Python: >=3.7", metadata)


if __name__ == "__main__":
    unittest.main()
