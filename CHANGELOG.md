# Changelog

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
