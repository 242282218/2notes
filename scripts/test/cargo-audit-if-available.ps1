$ErrorActionPreference = "Stop"

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
Push-Location src-tauri
try {
  # GTK3 is a Linux-only transitive dependency in this Windows desktop project.
  cargo audit -D unsound --ignore RUSTSEC-2024-0429
} finally {
  Pop-Location
}
