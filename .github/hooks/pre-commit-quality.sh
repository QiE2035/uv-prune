#!/usr/bin/env bash
# Pre-commit quality gate for uv-prune (Unix/macOS).
# Same contract as the Windows PowerShell twin: blocks `git commit`
# when `cargo fmt --check` or `cargo clippy --all-targets` fails.
set -u

# Read stdin only when it is a pipe (hook payload); never block on a TTY.
stdin=""
if [ ! -t 0 ]; then
  stdin="$(cat)"
fi
case "$stdin" in
  *"git commit"*) : ;;
  *) exit 0 ;;
esac

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
[ -f "$root/Cargo.toml" ] || exit 0
cd "$root" || exit 0

report=""
if ! fmt_out="$(cargo fmt --check 2>&1)"; then
  report="${report}cargo fmt --check FAILED:
${fmt_out}

"
fi
if ! clippy_out="$(cargo clippy --all-targets 2>&1)"; then
  report="${report}cargo clippy --all-targets FAILED:
${clippy_out}

"
fi

if [ -z "$report" ]; then
  exit 0
fi

# Truncate and JSON-escape the report for systemMessage.
report="$(printf '%s' "$report" | head -c 4000)"
escaped="$(printf '%s' "$report" | awk '{ gsub(/\\/, "\\\\"); gsub(/"/, "\\\""); printf "%s\\n", $0 }')"
printf '{"systemMessage": "Pre-commit quality gate failed - fix these, then re-run:\\n\\n%s"}' "$escaped"
exit 2