# Changelog

## [0.1.2] — 2026-08-07

### Added

- CI workflow (`.github/workflows/ci.yml`): fmt, clippy and tests on push to `main` and on pull requests, across Linux, macOS and Windows — the local commit hook only covers the development platform, so this is the only place platform-specific code (`hardlink.rs`) is verified everywhere.
- Release automation via GitHub Actions (`.github/workflows/publish.yml`): pushing a `v*` tag builds binaries for Linux (musl), macOS and Windows (x86_64 / arm64), attaches them to a GitHub Release with `SHA256SUMS`, and publishes `py3-none-{platform}` wheels to PyPI (trusted publishing or `PYPI_API_TOKEN`).
- `scripts/make_wheel.py` — builds a platform wheel that ships the compiled binary under `.data/scripts/`, enabling `uv tool install` / `pipx install` / `pip install`. Python is managed by uv both locally and on CI.
- Shared `setup-rust` composite action (`.github/actions/setup-rust`) — the checkout + toolchain + cache steps are defined once and reused by both workflows; the PyPI job also gained the `contents: read` permission its `checkout` step needs.
- Git branches reorganized: default branch renamed `master` → `main`, development branches archived under `archive/` (`archive/modernize`, `archive/prune_by_metadata_file`).

### Changed

- `scripts/make_wheel.py` now ships the binary inside a thin Python launcher package (`uv_prune`) wired up via a `console_scripts` entry point. `uv tool install` / `pipx` / `pip` therefore only place a small launcher in the bin directory instead of copying the whole binary there, keeping a single copy of the executable inside the environment.

- `build.rs` embeds a full version string at compile time, so `--version` distinguishes build provenance: CI builds get `+ci.<run-number>.<commit-sha-prefix>` (e.g. `uv-prune 0.1.0+ci.42.a1b2c3d4`), local development builds get `+dev.<short-sha>`, and tagged releases stay clean (`uv-prune 0.1.0`) — the publish workflow sets `UV_PRUNE_RELEASE_BUILD` so git state on CI can never leak a dev marker into release artifacts.

### Fixed

- Report table column widths now only account for the rows actually rendered at the current log level: verbose-only entries (`DEBUG` `Keeping`, `Skipping` without `.dist-info`) no longer inflate the `ID` / `Name` / `Detail` columns of non-verbose output. Column widths are computed from the reports filtered by `log::max_level()` instead of the full report set.

## [0.1.1] — 2026-08-06

### Fixed

- `scripts/make_wheel.py` embeds the repository `README.md` as the wheel's `Description` — previously the PyPI project page rendered an empty description because no long description was shipped in `METADATA`.
- Publish workflow (`publish.yml`): the `github-release` job was missing `actions/checkout@v4`, so `gh release create --generate-notes` failed with `not a git repository`.
- Publish workflow (`publish.yml`): the `pypi` job ran `actions/checkout@v4` *after* `download-artifact`, and checkout's `git clean -ffdx` wiped the downloaded artifacts — no wheels were ever built.
- Publish workflow (`publish.yml`): Windows archives were packaged with Git Bash's `tar -a -cf x.zip`, which is GNU tar and does not recognize the `.zip` suffix — it silently emitted a plain tar archive. Packaging now goes through `scripts/package-zip.ps1` (`Compress-Archive`), producing real zip files.

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
