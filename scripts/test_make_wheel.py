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

    def test_wheel_name_and_binary_paths(self) -> None:
        cases = {
            "win_amd64": "uv_prune/_bin/uv-prune.exe",
            "win_arm64": "uv_prune/_bin/uv-prune.exe",
            "musllinux_1_2_x86_64": "uv_prune/_bin/uv-prune",
            "musllinux_1_2_aarch64": "uv_prune/_bin/uv-prune",
            "macosx_11_0_arm64": "uv_prune/_bin/uv-prune",
            "macosx_10_12_x86_64": "uv_prune/_bin/uv-prune",
        }
        for tag, binary_path in cases.items():
            with self.subTest(tag=tag):
                wheel = self._build(tag)
                self.assertEqual(Path(wheel).name, f"uv_prune-{VERSION}-py3-none-{tag}.whl")
                with zipfile.ZipFile(wheel) as zf:
                    self.assertIn(binary_path, zf.namelist())
                    self.assertEqual(zf.read(binary_path), self.payload)
                    self.assertEqual(zf.namelist().count(binary_path), 1)

    def test_entry_points_and_launcher_module(self) -> None:
        wheel = self._build("win_amd64")
        entry_points = self._read(wheel, "uv_prune-0.1.0.dist-info/entry_points.txt").decode()
        self.assertEqual(entry_points, "[console_scripts]\nuv-prune = uv_prune:main\n")
        init_py = self._read(wheel, "uv_prune/__init__.py").decode()
        self.assertIn("def main()", init_py)
        self.assertIn("subprocess.call", init_py)
        self.assertIn('"_bin"', init_py)

    def test_unix_binary_has_executable_bit(self) -> None:
        wheel = self._build("musllinux_1_2_x86_64")
        with zipfile.ZipFile(wheel) as zf:
            info = zf.getinfo("uv_prune/_bin/uv-prune")
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
        # METADATA, WHEEL, entry_points.txt, __init__.py, binary, RECORD itself
        self.assertEqual(len(rows), 6)
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
        self.assertIn("Root-Is-Purelib: true", wheel_meta)
        metadata = self._read(wheel, "uv_prune-0.1.0.dist-info/METADATA").decode()
        self.assertIn("Name: uv-prune", metadata)
        self.assertIn("Version: 0.1.0", metadata)
        self.assertIn("Requires-Python: >=3.7", metadata)

    def test_readme_embedded_as_description(self) -> None:
        readme = "# uv-prune\n\nDescription for PyPI page.\n"
        wheel = build_wheel(
            binary=str(self.binary),
            name=NAME,
            version=VERSION,
            platform_tag="win_amd64",
            summary="test summary",
            project_url="https://example.com/uv-prune",
            readme=readme,
            out_dir=str(self.tmp),
        )
        metadata = self._read(wheel, "uv_prune-0.1.0.dist-info/METADATA").decode()
        self.assertIn("Description-Content-Type: text/markdown\n", metadata)
        # The description body follows the blank line after the headers.
        self.assertIn("\n\n# uv-prune\n\nDescription for PyPI page.", metadata)

    def test_metadata_without_readme(self) -> None:
        wheel = self._build("win_amd64")
        metadata = self._read(wheel, "uv_prune-0.1.0.dist-info/METADATA").decode()
        self.assertNotIn("# uv-prune", metadata)


if __name__ == "__main__":
    unittest.main()
