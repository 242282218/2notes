$ErrorActionPreference = "Stop"

# Regenerates the schema-equivalent 0.3.0 upgrade baseline fixture.
# This is NOT installer-captured data; it seeds migration v4 via app repos.

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$fixtureDir = Join-Path $repoRoot "scripts\test\fixtures"
$fixtureDb = Join-Path $fixtureDir "v0.3.0-upgrade-baseline.sqlite"
$metricsPath = Join-Path $fixtureDir "v0.3.0-upgrade-baseline.metrics.json"

$env:GENERATE_V030_FIXTURE = "1"
& cargo test --manifest-path $manifestPath --lib v030_upgrade_baseline_fixture_seeds_and_restores -- --nocapture
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if (-not (Test-Path $fixtureDb)) {
    Write-Error "Fixture DB was not written: $fixtureDb"
    exit 1
}
if (-not (Test-Path $metricsPath)) {
    Write-Error "Fixture metrics were not written: $metricsPath"
    exit 1
}

Write-Host "Fixture ready:"
Write-Host "  DB      $fixtureDb"
Write-Host "  Metrics $metricsPath"
exit 0
