param(
  [string]$Tag = $env:GITHUB_REF_NAME
)

$ErrorActionPreference = "Stop"

# Resolve the repository root from the script location so this runs from any
# working directory, not only from the CI checkout root.
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

if ([string]::IsNullOrWhiteSpace($Tag)) {
  Write-Error "Release tag is required."
  exit 1
}

if ($Tag -notmatch '^v\d+\.\d+\.\d+$') {
  Write-Error "Release tag must match vMAJOR.MINOR.PATCH: $Tag"
  exit 1
}

$expected = $Tag.TrimStart("v")
$packageVersion = (Get-Content -Raw (Join-Path $repoRoot "package.json") | ConvertFrom-Json).version
$tauriVersion = (Get-Content -Raw (Join-Path $repoRoot "src-tauri\tauri.conf.json") | ConvertFrom-Json).version
$cargoContent = Get-Content -Raw (Join-Path $repoRoot "src-tauri\Cargo.toml")
$cargoMatch = [regex]::Match($cargoContent, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $cargoMatch.Success) {
  Write-Error "Cargo package version was not found."
  exit 1
}
$cargoVersion = $cargoMatch.Groups[1].Value

$versions = [ordered]@{
  tag = $expected
  package = $packageVersion
  cargo = $cargoVersion
  tauri = $tauriVersion
}

$mismatched = $versions.GetEnumerator() | Where-Object Value -ne $expected
if ($mismatched) {
  $summary = ($versions.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ", "
  Write-Error "Release versions do not match: $summary"
  exit 1
}

$changelog = Get-Content -Raw (Join-Path $repoRoot "CHANGELOG.md")
if ($changelog -notmatch "(?m)^##\s*\[$([regex]::Escape($expected))") {
  Write-Error "CHANGELOG.md is missing a section for [$expected]."
  exit 1
}

Write-Output "PASS release_version=$expected"
