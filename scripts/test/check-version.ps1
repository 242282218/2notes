param(
  [string]$Tag = $env:GITHUB_REF_NAME
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Tag)) {
  Write-Error "Release tag is required."
  exit 1
}

$expected = $Tag.TrimStart("v")
$packageVersion = (Get-Content -Raw package.json | ConvertFrom-Json).version
$tauriVersion = (Get-Content -Raw src-tauri\tauri.conf.json | ConvertFrom-Json).version
$cargoContent = Get-Content -Raw src-tauri\Cargo.toml
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

Write-Output "PASS release_version=$expected"