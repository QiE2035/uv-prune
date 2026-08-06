//! Injects the full version string into the build via `UV_PRUNE_FULL_VERSION`.
//!
//! Version resolution, highest priority first:
//!   1. `UV_PRUNE_RELEASE_BUILD` non-empty → plain `{version}` — an explicit
//!      request for a release-clean version, set by the publish workflow so
//!      git state (shallow clones, detached heads) can never leak a dev
//!      marker into release artifacts.
//!   2. `UV_PRUNE_BUILD_META` non-empty → `{version}+{meta}` (CI builds set
//!      e.g. `ci.42.a1b2c3d4`).
//!   3. Neither set (local) → if `git describe --exact-match --tags`
//!      matches, HEAD is a tag and the build is a release source, so the
//!      plain version is used; otherwise the build is a development build
//!      and gets `{version}+dev.{short-sha}`.
//!
//! No `rerun-if-*` directives are emitted intentionally: git state
//! (commits, tags, branch switches) cannot be tracked reliably via
//! `rerun-if-changed` (packed refs, detached HEADs), so the script runs on
//! every build — a few millisecond git calls, and a stale marker after a
//! commit/tag change costs a recompile, which is exactly what we want.

use std::process::Command;

fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is always set");
    let full_version = if is_release_build() {
        version
    } else if let Ok(meta) = std::env::var("UV_PRUNE_BUILD_META") {
        if meta.is_empty() {
            version
        } else {
            format!("{version}+{meta}")
        }
    } else {
        local_build_version(&version)
    };
    println!("cargo:rustc-env=UV_PRUNE_FULL_VERSION={full_version}");
}

/// True when the publish workflow explicitly requests a release-clean
/// version. A dedicated flag is used (rather than an empty
/// `UV_PRUNE_BUILD_META`) because empty environment variables are not
/// reliably propagated to build scripts on all platforms.
fn is_release_build() -> bool {
    std::env::var("UV_PRUNE_RELEASE_BUILD").is_ok_and(|v| !v.is_empty())
}

/// Version for builds without an explicit meta: `+dev.{short-sha}` unless
/// HEAD is exactly a tag (a release source), in which case the plain version.
fn local_build_version(version: &str) -> String {
    if git_ok(&["describe", "--exact-match", "--tags", "HEAD"]) {
        return version.to_string();
    }
    match git_output(&["rev-parse", "--short=8", "HEAD"]) {
        Some(sha) => format!("{version}+dev.{sha}"),
        None => format!("{version}+dev"),
    }
}

fn git_ok(args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .output()
        .is_ok_and(|out| out.status.success())
}

fn git_output(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}
