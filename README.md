# uv-prune

Clean [uv](https://docs.astral.sh/uv/) cache by removing non-hardlinked archive entries.

## Problem

uv uses [hard links](https://en.wikipedia.org/wiki/Hard_link) to deduplicate package data across different Python versions and projects. When a package is no longer used by any project, its archive entry remains on disk but is no longer referenced by any other hard link.

`uv-prune` scans the `archive-v0` directory in uv's cache and removes entries whose files have a link count of 1 — meaning they are no longer shared and can be safely deleted.

## Installation

```bash
cargo install uv-prune
```

Or build from source:

```bash
git clone https://github.com/QiE2035/uv-prune
cd uv-prune
cargo build --release
```

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
[DEBUG uv_prune::prune] Keeping (hardlinked): abc123def456
[DEBUG uv_prune::prune] Keeping (hardlinked): 789ghi012jkl
[INFO  uv_prune::prune] Done: 2 checked, 0 removed, 2 skipped, 0 no-dist-info, 0 errors
[INFO  uv_prune] Elapsed time: 15.2ms
```

## Platform Support

| Platform | Hard link detection                                      |
| -------- | -------------------------------------------------------- |
| Windows  | `GetFileInformationByHandle` → `nNumberOfLinks`          |
| Linux    | `stat` → `st_nlink` via `std::os::unix::fs::MetadataExt` |
| macOS    | `stat` → `st_nlink` via `std::os::unix::fs::MetadataExt` |
