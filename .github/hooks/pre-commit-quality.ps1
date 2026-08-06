# Pre-commit quality gate for uv-prune.
# Triggered by PreToolUse hook when an agent runs `git commit`.
#
# Behavior:
#   - Any command not containing "git commit" -> exit 0 (let it through)
#   - Runs `cargo fmt --check` + `cargo clippy --all-targets` from the repo root
#   - Any failure -> emits a systemMessage + exits 2 (blocks the commit)
#   - Also exits 0 if cargo is unavailable or the repo lacks Cargo.toml

$stdin = [Console]::In.ReadToEnd()
if ($stdin -notmatch "git commit") { exit 0 }

$root = git rev-parse --show-toplevel 2>$null
if (-not $root -or -not (Test-Path (Join-Path $root "Cargo.toml"))) { exit 0 }

Push-Location $root
$failures = @()

$fmtExit = 0
$fmtOut = ""
& cargo fmt --check 2>&1 | Out-String | ForEach-Object { $fmtOut = $_ }
$fmtExit = $LASTEXITCODE
if ($fmtExit -ne 0) {
    $failures += "cargo fmt --check FAILED (exit $fmtExit):`r`n$fmtOut"
}

$clippyExit = 0
$clippyOut = ""
& cargo clippy --all-targets 2>&1 | Out-String | ForEach-Object { $clippyOut = $_ }
$clippyExit = $LASTEXITCODE
if ($clippyExit -ne 0) {
    $failures += "cargo clippy --all-targets FAILED (exit $clippyExit):`r`n$clippyOut"
}
Pop-Location

if ($failures.Count -eq 0) { exit 0 }

$body = "Pre-commit quality gate failed - fix these, then commit again:`r`n`r`n" + ($failures -join "`r`n`r`n")
# Cap the payload so a noisy lint dump cannot bloat the hook output.
if ($body.Length -gt 4000) { $body = $body.Substring(0, 4000) + "...`r`n(truncated)" }
@{ systemMessage = $body } | ConvertTo-Json -Compress
exit 2