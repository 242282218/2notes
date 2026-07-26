$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$capabilitiesDir = Join-Path $repoRoot "src-tauri\capabilities"
$allowlistPath = Join-Path $PSScriptRoot "tauri-permissions.allowlist.json"

if (-not (Test-Path $allowlistPath)) {
  Write-Error "Missing allowlist: $allowlistPath"
  exit 1
}

$allowlistDoc = Get-Content -Raw -Encoding UTF8 $allowlistPath | ConvertFrom-Json
$allowed = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($name in @($allowlistDoc.permissions)) {
  if ([string]::IsNullOrWhiteSpace($name)) {
    Write-Error "Allowlist contains empty permission name."
    exit 1
  }
  if ($name -match '[\*\?]' -or $name.Contains("..")) {
    Write-Error "Allowlist permission must be exact (no wildcards): $name"
    exit 1
  }
  [void]$allowed.Add($name)
}

$capabilityFiles = Get-ChildItem -Path $capabilitiesDir -Filter *.json -File
if (-not $capabilityFiles) {
  Write-Error "No capability JSON files found under $capabilitiesDir"
  exit 1
}

$failures = New-Object System.Collections.Generic.List[string]

function Get-PermissionNames {
  param([object]$Permissions)

  $names = New-Object System.Collections.Generic.List[string]
  foreach ($entry in @($Permissions)) {
    if ($null -eq $entry) {
      continue
    }
    if ($entry -is [string]) {
      $names.Add($entry)
      continue
    }
    if ($entry.PSObject.Properties.Name -contains "identifier") {
      $names.Add([string]$entry.identifier)
      continue
    }
    $failures.Add("Unsupported permission entry shape: $($entry | ConvertTo-Json -Compress)")
  }
  return $names
}

foreach ($file in $capabilityFiles) {
  $doc = Get-Content -Raw -Encoding UTF8 $file.FullName | ConvertFrom-Json
  $identifier = [string]$doc.identifier
  $windows = @($doc.windows)
  $permissionNames = @(Get-PermissionNames -Permissions $doc.permissions)

  if ($windows -contains "*") {
    $failures.Add("$($file.Name): windows must not use wildcards")
  }

  foreach ($perm in $permissionNames) {
    if ([string]::IsNullOrWhiteSpace($perm)) {
      $failures.Add("$($file.Name): empty permission entry")
      continue
    }
    if ($perm -match '[\*\?]') {
      $failures.Add("$($file.Name): wildcard permission is forbidden: $perm")
    }
    if (-not $allowed.Contains($perm)) {
      $failures.Add("$($file.Name): permission not in allowlist: $perm")
    }
  }

  $isQuickCapture = ($identifier -eq "quick-capture") -or ($windows -contains "quick-capture")
  if ($isQuickCapture) {
    foreach ($perm in $permissionNames) {
      if ($perm -like "opener:*" -or $perm -like "dialog:*" -or $perm -like "window-state:*") {
        $failures.Add("$($file.Name): quick-capture must not receive $perm")
      }
      if ($perm -eq "core:default") {
        $failures.Add("$($file.Name): quick-capture must use minimal permissions, not core:default")
      }
    }
  }

  if ($identifier -eq "default") {
    foreach ($perm in $permissionNames) {
      if ($perm -eq "opener:default") {
        $failures.Add("$($file.Name): opener:default is unused; keep only opener:allow-open-path")
      }
      if ($perm -like "dialog:*") {
        $failures.Add("$($file.Name): dialog permissions must not be granted to frontend")
      }
    }
  }
}

if ($failures.Count -gt 0) {
  Write-Host "FAIL tauri capability checks:"
  foreach ($failure in $failures) {
    Write-Host " - $failure"
  }
  exit 1
}

Write-Output "PASS tauri capabilities checked ($($capabilityFiles.Count) files, $($allowed.Count) allowlisted permissions)"
