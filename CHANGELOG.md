# Changelog

## [Unreleased]

### Added

- Release automation via GitHub Actions (`.github/workflows/publish.yml`): pushing a `v*` tag builds binaries for Linux (musl), macOS and Windows (x86_64 / arm64), attaches them to a GitHub Release with `SHA256SUMS`, and publishes `py3-none-{platform}` wheels to PyPI (trusted publishing or `PYPI_API_TOKEN`).
- `scripts/make_wheel.py` — builds a platform wheel that ships the compiled binary under `.data/scripts/`, enabling `uv tool install` / `pipx install` / `pip install`. Python is managed by uv both locally and on CI.

## [0.1.0] — 2026-07-30

### Added

- Complete project restructuring with modular architecture:
  - `cli.rs` — CLI argument parsing via `clap` (derive API)
  - `config.rs` — Configuration aggregation from CLI args and environment
  - `error.rs` — Typed error enum via `thiserror`
  - `hardlink.rs` — Cross-platform hard link detection trait
  - `prune.rs` — Core pruning logic with statistics
  - `main.rs` — Slim entry point
- Cross-platform support: Unix hard link detection via `nlink()`
- CLI arguments: `--dry-run`, `--verbose`, `--cache-dir`, `--jobs`, `--no-timing`, `--include-no-dist-info`
- Structured logging via `log` + `env_logger` with `RUST_LOG` support
- Safer default: entries without `.dist-info` only warn; `--include-no-dist-info` to delete
- Dry-run mode to preview changes without deleting
- Automatic timing measurement (configurable via `--no-timing`)
- Summary statistics after each run
- Proper error types and context propagation via `anyhow`

### Changed

- Replaced `#[cfg(feature = "time")]` with runtime `--no-timing` flag
- Migrated from `println`/`eprintln` to structured logging with severity levels
- Unified hard link trait to always return `io::Result<bool>`

### Removed

- `time` Cargo feature (replaced by `--no-timing` CLI flag)
- Monolithic `main.rs` — all logic split into dedicated modules
