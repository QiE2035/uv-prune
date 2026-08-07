# uv-prune

Clean [uv](https://docs.astral.sh/uv/) cache by removing non-hardlinked archive entries.

## Problem

uv uses [hard links](https://en.wikipedia.org/wiki/Hard_link) to deduplicate package data across different Python versions and projects. When a package is no longer used by any project, its archive entry remains on disk but is no longer referenced by any other hard link.

`uv-prune` scans the `archive-v0` directory in uv's cache and removes entries whose files have a link count of 1 — meaning they are no longer shared and can be safely deleted.

## How it works

For each entry in `archive-v0`, `uv-prune`:

1. Looks for a `.dist-info` subdirectory to identify the package name and version (e.g. `anyio-4.0.0` from `anyio-4.0.0.dist-info`).
2. Checks the link count of the `METADATA` file inside that directory — uv hard-links this file across all environments that share the package.
3. If the link count is **1**, the archive entry is no longer shared and is **removed**. If it is **> 1**, the entry is still in use and is **kept**.
4. Entries **without** a `.dist-info` directory are only warned about by default; pass `--include-no-dist-info` to remove them too.

All entries are processed in parallel (via Rayon) and reported as a table sorted by package name and version.

## Installation

### From PyPI (pre-built binary)

Every tagged release publishes platform wheels to PyPI (the compiled binary ships inside a thin Python launcher package, so the bin directory gets a small launcher instead of a full copy of the executable), so install with any Python package manager:

```bash
uv tool install uv-prune   # recommended — managed by uv
pipx install uv-prune
pip install uv-prune       # then run `uv-prune`
```

### From GitHub Releases

Pre-built binaries for Linux (x86_64 / aarch64, static musl), macOS (arm64 / x86_64) and Windows (x86_64 / arm64) are attached to each [release](https://github.com/QiE2035/uv-prune/releases) as `.tar.gz` / `.zip` archives together with a `SHA256SUMS` checksum file.

### From source

```bash
cargo install uv-prune
```

Or build manually:

```bash
git clone https://github.com/QiE2035/uv-prune
cd uv-prune
cargo build --release
```

## Releases

Pushing a `v*` tag triggers the [publish workflow](.github/workflows/publish.yml), which:

1. Builds release binaries for Linux (musl), macOS and Windows (see the platform table in the workflow).
2. Creates (or updates) a GitHub Release with the archives and `SHA256SUMS`.
3. Builds `py3-none-{platform}` wheels from those binaries and publishes them to PyPI — via [trusted publishing](https://docs.pypi.org/trusted-publishers/) (recommended), or the `PYPI_API_TOKEN` repository secret as fallback.

```bash
git tag v0.1.2
git push origin v0.1.2
```

The workflow can also be run manually via `workflow_dispatch` to smoke-test the build matrix without publishing.

`--version` reports build provenance: local development builds get `+dev.<short-sha>` (e.g. `uv-prune 0.1.0+dev.a1b2c3d4`), CI builds of `main` get `+ci.<run-number>.<commit-sha-prefix>` (e.g. `uv-prune 0.1.0+ci.42.a1b2c3d4`), and tagged releases stay clean (`uv-prune 0.1.0`).

## Usage

```bash
# Prune with default cache path
uv-prune

# Dry-run — see what would be deleted without actually deleting
uv-prune --dry-run

# Verbose output
uv-prune --verbose

# Custom cache directory
uv-prune --cache-dir /path/to/uv/cache

# Also remove entries without .dist-info (default: warn only)
uv-prune --include-no-dist-info

# Disable timing measurement
uv-prune --no-timing

# Set number of parallel workers
uv-prune --jobs 4
```

### Options

| Option                   | Short | Description                                             |
| ------------------------ | ----- | ------------------------------------------------------- |
| `--cache-dir <DIR>`      | `-c`  | UV cache directory (overrides `UV_CACHE_DIR` env var)   |
| `--dry-run`              | `-d`  | Show what would be deleted without actually deleting    |
| `--verbose`              | `-v`  | Enable verbose (debug) output                           |
| `--include-no-dist-info` | `-i`  | Also remove entries without a `.dist-info` directory    |
| `--jobs <N>`             | `-j`  | Number of parallel workers (`0` = auto-detect, default) |
| `--no-timing`            | `-n`  | Disable timing measurement                              |

### Exit Codes

| Code | Meaning                                       |
| ---- | --------------------------------------------- |
| `0`  | Success (even if entries were skipped/warned) |
| `1`  | One or more errors occurred while pruning     |

### Environment Variables

| Variable       | Description                                                |
| -------------- | ---------------------------------------------------------- |
| `UV_CACHE_DIR` | Override the uv cache directory path                       |
| `RUST_LOG`     | Control log level (e.g., `debug`, `info`, `warn`, `error`) |

### Cache Directory Resolution

1. `--cache-dir` CLI argument
2. `UV_CACHE_DIR` environment variable
3. Platform default:
   - **Windows**: `%LOCALAPPDATA%\uv\cache`
   - **Linux / macOS**: `~/.cache/uv` (or `$XDG_CACHE_HOME/uv`)

## Output Example

```
$ uv-prune --dry-run --verbose
[INFO  uv_prune] uv-prune v0.1.0 — cache: /home/user/.cache/uv
[INFO  uv_prune::prune] Pruning archive directory: /home/user/.cache/uv/archive-v0
[INFO  uv_prune::prune] Dry-run mode — no files will be deleted
[INFO  uv_prune::prune] [DRY-RUN] Action    ID                     Version   Name       Detail
[INFO  uv_prune::prune] [DRY-RUN] --------  ---------------------  --------  ---------  -----------------------------------------
[INFO  uv_prune::prune] [DRY-RUN] Deleting  abc123def456789abc     4.0.0     anyio
[DEBUG uv_prune::prune] [DRY-RUN] Keeping   ghi012jkl345mnopqr     2.31.0    requests
[WARN  uv_prune::prune] [DRY-RUN] Skipping  deadbeef               -         -          no .dist-info (use --include-no-dist-info to remove)
[INFO  uv_prune::prune] Done: 3 checked, 1 removed, 2 skipped, 1 no-dist-info, 0 errors
[INFO  uv_prune] Elapsed time: 15.2ms
```

The report is a table with the following columns:

- **Action** — `Deleting`, `Keeping`, `Skipping` or `Failed`.
- **ID** — the uv cache archive directory name; `-` when it could not be read.
- **Version / Name** — parsed from the `.dist-info` directory name; `-` when there is no `.dist-info`.
- **Detail** — the reason for `Skipping` / `Failed` entries, if any.

In dry-run mode every table line is prefixed with `[DRY-RUN]` and nothing is actually deleted. `Keeping` rows are logged at debug level, so pass `--verbose` to see them, and column widths adapt to the longest entry.

## Platform Support

| Platform | Hard link detection                                      |
| -------- | -------------------------------------------------------- |
| Windows  | `GetFileInformationByHandle` → `nNumberOfLinks`          |
| Linux    | `stat` → `st_nlink` via `std::os::unix::fs::MetadataExt` |
| macOS    | `stat` → `st_nlink` via `std::os::unix::fs::MetadataExt` |
