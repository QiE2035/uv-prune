#!/usr/bin/env python3
"""Build a PEP 427 wheel that ships the compiled `uv-prune` binary.

The wheel contains a thin Python launcher package (`uv_prune`) plus the
compiled binary under `uv_prune/_bin/`, wired up via a `console_scripts`
entry point. Installers (`pip`, `pipx`, `uv tool install`) therefore only
create a small launcher in the bin directory and keep a single copy of the
binary inside the environment — a bare binary under `.data/scripts/` would
otherwise be copied wholesale into the bin directory.

Windows wheels ship the executable as `uv-prune.exe` (a bare PE file
without the `.exe` extension cannot be launched by the OS), Unix wheels
ship it as `uv-prune`. Only the Python standard library is used.

Usage:
    python scripts/make_wheel.py --binary <path-to-binary> \\
        --name uv-prune --version 0.1.0 \\
    --platform-tag musllinux_1_2_x86_64 [--readme <path>] [--out <dir>]

The platform tag is the PEP 425 tag of the target platform, e.g.
`musllinux_1_2_x86_64`, `macosx_11_0_arm64` or `win_amd64`.
`--readme` defaults to `README.md` at the repository root; its content is
embedded as the wheel's `Description` so PyPI shows a project page instead
of an empty description.

Tests:
    uv run --no-project python scripts/test_make_wheel.py
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import io
import os
import sys
import zipfile
from pathlib import Path

# Unix file type (regular) + rwxr-xr-x — needed so pip does not have to
# re-mark the script executable on install.
UNIX_EXECUTABLE_ATTR = (0o100755 << 16)

# The launcher module shipped inside the wheel. `main()` finds the compiled
# binary next to it and execs it, so the launcher itself is tiny.
INIT_PY = '''\
"""uv-prune — thin launcher that starts the compiled Rust binary.

The real program is the binary in `_bin/`; this module just locates and
execs it. Installers (uv, pip, pipx) turn the `console_scripts` entry point
into a small launcher in the bin directory instead of copying the whole
binary there.
"""

import os
import subprocess
import sys
from pathlib import Path

_BINARY_NAME = "uv-prune.exe" if os.name == "nt" else "uv-prune"


def main() -> int:
    binary = Path(__file__).resolve().parent / "_bin" / _BINARY_NAME
    return subprocess.call([str(binary), *sys.argv[1:]])


if __name__ == "__main__":
    raise SystemExit(main())
'''


def sha256_urlsafe(data: bytes) -> str:
    digest = hashlib.sha256(data).digest()
    return base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")


def build_wheel(
    binary: str,
    name: str,
    version: str,
    platform_tag: str,
    summary: str,
    project_url: str,
    out_dir: str,
    readme: str | None = None,
) -> str:
    """Assemble the wheel file and return its path."""
    # A tag like `cp312-abi3-manylinux` is ABI-specific; a plain binary is
    # Python-version agnostic, so `py3-none-{platform}` is correct.
    name_safe = name.replace("-", "_")
    wheel_name = f"{name_safe}-{version}-py3-none-{platform_tag}.whl"
    wheel_path = os.path.join(out_dir, wheel_name)

    dist_info = f"{name_safe}-{version}.dist-info"
    # The binary lives inside the package (`uv_prune/_bin/`), so it lands in
    # site-packages and is found via `__file__`. The console_scripts entry
    # point below is what installers expose as the `uv-prune` command.
    # On Windows the script must carry the `.exe` extension — a PE file
    # named `uv-prune` without it cannot be executed by the OS.
    binary_name = f"{name}.exe" if platform_tag.startswith("win") else name
    binary_rel = f"{name_safe}/_bin/{binary_name}"
    init_rel = f"{name_safe}/__init__.py"
    entry_points_rel = f"{dist_info}/entry_points.txt"

    metadata_lines = [
            "Metadata-Version: 2.1",
            f"Name: {name}",
            f"Version: {version}",
            f"Summary: {summary}",
            f"Home-page: {project_url}",
            f"Project-URL: Repository, {project_url}",
            "Requires-Python: >=3.7",
            "Description-Content-Type: text/markdown",
        ]
    # The description body starts after a blank line following the headers
    # (email-header convention, same layout setuptools emits). Without it
    # PyPI has no long description for the project page.
    if readme is not None:
        metadata_lines.append("")
        metadata_lines.append(readme.rstrip())
    metadata = "\n".join(metadata_lines).encode("utf-8")

    wheel_meta = "\n".join(
        [
            "Wheel-Version: 1.0",
            f"Generator: uv-prune release ({version})",
            "Root-Is-Purelib: true",
            f"Tag: py3-none-{platform_tag}",
            "",
        ]
    ).encode("utf-8")

    entry_points = f"[console_scripts]\n{name} = {name_safe}:main\n"

    with open(binary, "rb") as fh:
        binary_bytes = fh.read()

    entries = [
        (f"{dist_info}/METADATA", metadata, None),
        (f"{dist_info}/WHEEL", wheel_meta, None),
        (f"{dist_info}/entry_points.txt", entry_points.encode("utf-8"), None),
        (init_rel, INIT_PY.encode("utf-8"), None),
        (binary_rel, binary_bytes, UNIX_EXECUTABLE_ATTR),
    ]

    with zipfile.ZipFile(wheel_path, "w", zipfile.ZIP_DEFLATED) as zf:
        records = []
        for path, content, attr in entries:
            info = zipfile.ZipInfo(path)
            info.date_time = (1980, 1, 1, 0, 0, 0)
            info.compress_type = zipfile.ZIP_DEFLATED
            if attr is not None:
                info.external_attr = attr
            zf.writestr(info, content)
            records.append((path, sha256_urlsafe(content), len(content)))

        record_path = f"{dist_info}/RECORD"
        record_io = io.StringIO()
        writer = csv.writer(record_io, lineterminator="\n")
        for path, digest, size in records:
            writer.writerow([path, f"sha256={digest}", str(size)])
        writer.writerow([record_path, "", ""])
        zf.writestr(record_path, record_io.getvalue().encode("utf-8"))

    return wheel_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, help="Path to the compiled uv-prune binary")
    parser.add_argument("--name", default="uv-prune")
    parser.add_argument("--version", required=True, help="Release version, e.g. 0.1.0")
    parser.add_argument("--platform-tag", required=True, help="PEP 425 platform tag, e.g. win_amd64")
    parser.add_argument("--summary", default="Clean uv cache by removing non-hardlinked archive entries")
    parser.add_argument("--project-url", default="https://github.com/QiE2035/uv-prune")
    parser.add_argument(
        "--readme",
        default=str(Path(__file__).resolve().parent.parent / "README.md"),
        help="Path to the project README to embed as the wheel Description (default: repo root README.md)",
    )
    parser.add_argument("--out", default="dist")
    args = parser.parse_args()

    if not os.path.isfile(args.binary):
        parser.error(f"--binary: file not found: {args.binary}")
    if not args.platform_tag:
        parser.error("--platform-tag must not be empty")

    readme: str | None = None
    if args.readme and os.path.isfile(args.readme):
        with open(args.readme, encoding="utf-8") as fh:
            readme = fh.read()

    os.makedirs(args.out, exist_ok=True)
    wheel_path = build_wheel(
        binary=args.binary,
        name=args.name,
        version=args.version,
        platform_tag=args.platform_tag,
        summary=args.summary,
        project_url=args.project_url,
        readme=readme,
        out_dir=args.out,
    )
    print(f"Built {wheel_path} ({os.path.getsize(wheel_path)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
