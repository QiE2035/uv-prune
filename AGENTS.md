# AGENTS.md

## Project

`uv-prune` is a Rust CLI (edition 2024) that cleans the [uv](https://docs.astral.sh/uv/) cache by removing non-hardlinked archive entries in `archive-v0`. Full behavior, CLI options, and output format are documented in [README.md](./README.md) — read it before changing user-facing behavior.

## Build & Test

```bash
cargo build              # debug
cargo build --release    # release (LTO, panic=abort, strip)
cargo test               # run all tests
cargo clippy --all-targets
cargo fmt
```

Two workflows exist: `ci.yml` runs fmt/clippy/tests on push to `main` and on pull requests across Linux, macOS and Windows; `publish.yml` is the tag-triggered release pipeline (build matrix + GitHub Release + PyPI). There is no clippy/fmt config — default rules apply. Tests use the real filesystem with temporary directories; run them on the target platform when platform-specific code changes (CI covers all three platforms).

## Architecture

Modules, each with tests colocated in-module:

| Module | Responsibility |
| --- | --- |
| `main.rs` | Entry point: logging init, rayon global thread pool, timing, exit code |
| `cli.rs` | clap derive `Cli` struct — single source of CLI flags |
| `config.rs` | `Config` aggregated from `Cli` + platform default cache path |
| `error.rs` | `PruneError` via thiserror (typed per-entry failures) |
| `hardlink.rs` | `IsHardLink` trait; Windows via `GetFileInformationByHandle`, Unix via `nlink()` |
| `package.rs` | `Package` type; dist-info name parsing (split on first `-`) |
| `prune.rs` | Core flow: `run` / `process_entry` / `remove_entry` / `classify_archive` / `validate_cache_dir` |
| `report.rs` | `PruneResult`, `EntryReport`, `EntryAction`, aligned table formatting, `report_entries` |

## Conventions

- **Error handling**: `anyhow::Result` for top-level flow (e.g. `main`); `thiserror` (`PruneError`) for typed per-entry errors.
- **Logging**: always use fully-qualified `log::info!` / `log::debug!` / `log::warn!` / `log::error!` — never import the macros. `--verbose` maps to debug level; `RUST_LOG` overrides.
- **Exit codes**: `0` = success (even with warnings/skips); `1` = one or more errors (via `std::process::exit(1)` in `main`).
- **Unsafe**: only 2 sites inside the Windows hardlink impl, each with `SAFETY` comments. Unsupported platforms use `compile_error!`.
- **Parallelism**: archive entries are processed in parallel via Rayon; `--jobs 0` = auto-detect.

## Git Branching

Branch names follow `type/name`, optionally nested as `type/a/b/c`:

- `main` — the only permanent branch; development happens directly on it, and releases are shipped via tags (no `develop`/`release` branches).
- `feat/*` / `fix/*` — new features and bug fixes; merge into `main` and delete after merging.
- `refactor/*` / `perf/*` / `ci/*` / `docs/*` / `chore/*` / `test/*` — same `type/name` pattern per purpose, merged and deleted.
- `archive/*` — shelved branches kept for history (e.g. `archive/modernize`, `archive/prune_by_metadata_file`). Their commits are already part of `main`'s history.

## Pitfalls

- Windows PowerShell reports `NativeCommandError` for cargo stderr output — exit code 1 does not always mean a real failure; read the actual error message.
- Tests use a hand-written `temp_dir(label)` helper (nanosecond-unique names) and manually `remove_dir_all`; never assume a shared temp location.
- `validate_cache_dir` only warns when the directory is missing — it does not error.
- Link-count semantics: count of **1** = safe to delete; **> 1** = still shared (in use). The `METADATA` file inside `.dist-info` is the hard-link probe.
- Changes to CLI flags require updating both `cli.rs` and the README options table.

## Code style preferences

- **Prefer traits over manual wiring**: implement `From` / `Default` instead of
	hand-written conversions or constructors when they fit naturally.
- **Prefer `Option<T>` over sentinel values** like empty `String` for
	"absent" data (e.g. `--dry-run`-style state or optional fields).
- **Extract shared field groups into types**: repeated `name`/`version` pairs
	belong in a shared struct (e.g. `Package`), not duplicated across variants.
- **Keep refactoring until nothing is left**: when asked to find improvements,
	iterate until no further opportunities remain instead of stopping at the
	first fix.
- **Never hard-wrap text**: keep paragraphs, list items and replies as single
	natural lines instead of wrapping at a fixed column width — in docs,
	changelogs and chat output alike.
- **Output-format changes must update test expectations**: the table
	formatter's expected strings (column widths, separators) are easy to get
	wrong — after touching formatting, run `cargo test` and verify the binary's
	real output (e.g. `cargo run -- --dry-run -v`) against the expectations.

## Quality gate

`.github/hooks/quality-gate.json` blocks `git commit` (via PreToolUse) until
`cargo fmt --check` and `cargo clippy --all-targets` pass — the fastest
feedback, with `ci.yml` running the same checks on push/PR as a remote
backstop. If a commit is blocked, the hook's `systemMessage` contains the
failing output; fix the reported issues before committing. The hook scripts
live in `.github/hooks/` (`.ps1` for Windows, `.sh` for Unix) and are
symmetric by design — keep both in sync.

## Delegation

`.github/agents/verify.agent.md` is a read-only verification agent: it runs
`cargo fmt --check`, `cargo clippy --all-targets` and `cargo test`, diagnoses
failures (including table-formatting assertion mismatches), and reports back
without touching code. Delegate the "run tests and verify" loop to it instead
of doing it inline when the surrounding task is long.
