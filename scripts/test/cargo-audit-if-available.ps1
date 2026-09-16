$ErrorActionPreference = "Stop"

# Resolve the repository root from the script location so this runs from any
# working directory, not only from the CI checkout root.
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

$auditCommand = Get-Command cargo-audit -ErrorAction SilentlyContinue
if (-not $auditCommand) {
  if ($env:CI -eq "true") {
    Write-Error "cargo-audit is required in CI but is not installed."
    exit 1
  }
  Write-Warning "cargo-audit is not installed; skipping Rust dependency audit."
  exit 0
}

cargo audit --version
$versionExitCode = $LASTEXITCODE
if ($versionExitCode -ne 0) {
  exit $versionExitCode
}

$auditExitCode = 1
Push-Location (Join-Path $repoRoot "src-tauri")
try {
  # RUSTSEC-2024-0429 is GTK3-only and unreachable from this Windows NSIS target.
  # Keep unsound advisories denied; RUSTSEC-2026-0221 requires event-listener >= 5.4.2 in Cargo.lock.
  # Yanked-status lookups make one request per crate and add no vulnerability
  # coverage here; Cargo's locked build already protects reproducibility.
  $auditArgs = @("-D", "unsound", "--no-yanked", "--ignore", "RUSTSEC-2024-0429")
  foreach ($attempt in 1..2) {
    cargo audit @auditArgs
    $auditExitCode = $LASTEXITCODE
    if ($auditExitCode -eq 0) {
      break
    }
    if ($attempt -eq 1) {
      Write-Warning "cargo-audit attempt 1 failed; retrying once after 5 seconds."
      Start-Sleep -Seconds 5
    }
  }

  # The cached database is still sufficient to reject known vulnerabilities when
  # GitHub's advisory repository is temporarily unreachable. A stale cache is
  # only a fallback after both online attempts fail; a nonzero offline audit is
  # still propagated as a hard failure.
  if ($auditExitCode -ne 0) {
    Write-Warning "cargo-audit could not refresh the advisory DB; validating against the local cache."
    cargo audit @auditArgs --no-fetch --stale
    $auditExitCode = $LASTEXITCODE
  }
} finally {
  Pop-Location
}

exit $auditExitCode
