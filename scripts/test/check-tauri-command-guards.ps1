$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$commandsDir = Join-Path $repoRoot "src-tauri\src\commands"

# Explicit classification matrix. Unknown #[tauri::command] functions fail the check.
$mainOnly = [ordered]@{
  "entries_create"             = "entries.rs"
  "entries_list"               = "entries.rs"
  "entries_get"                = "entries.rs"
  "entries_update"             = "entries.rs"
  "entries_move_to_trash"      = "entries.rs"
  "entries_restore_from_trash" = "entries.rs"
  "entries_delete_forever"     = "entries.rs"
  "knowledge_suggest"          = "knowledge.rs"
  "knowledge_tree_get"         = "knowledge.rs"
  "knowledge_breadcrumbs_get"  = "knowledge.rs"
  "knowledge_relations_get"    = "knowledge.rs"
  "knowledge_rebuild_index"    = "knowledge.rs"
  "knowledge_promote"          = "knowledge.rs"
  "knowledge_move"             = "knowledge.rs"
  "knowledge_demote"           = "knowledge.rs"
  "tags_suggest"               = "tags.rs"
  "tags_list"                  = "tags.rs"
  "settings_get"               = "settings.rs"
  "settings_update"            = "settings.rs"
  "export_markdown"            = "export_markdown.rs"
  "backups_create"             = "backups.rs"
  "backups_list"               = "backups.rs"
  "backups_restore"            = "backups.rs"
  "window_open_quick_capture"  = "windows.rs"
}

# Commands intentionally callable from quick-capture (or both windows).
# They must NOT be forced through require_main_window by this script.
$sharedAllowlist = [ordered]@{
  "draft_get"                 = "drafts.rs"
  "draft_update"              = "drafts.rs"
  "quick_capture_submit"      = "drafts.rs"
  "window_hide_quick_capture" = "windows.rs"
  "database_restore_ready"    = "backups.rs"
  "app_quit_ready"            = "windows.rs"
}

$classified = @{}
foreach ($key in $mainOnly.Keys) { $classified[$key] = "main-only" }
foreach ($key in $sharedAllowlist.Keys) {
  if ($classified.ContainsKey($key)) {
    Write-Error "Command classified twice: $key"
    exit 1
  }
  $classified[$key] = "shared"
}

$sideEffectPattern = [regex]'(\bstate\b|\brun_blocking\b|\bshow_quick_capture_window\b|\bhide_quick_capture_window\b|\bcreate_backup\b|\blist_backups\b|\brestore_backup\b|\bmark_app_quit_ready\b|\.dialog\s*\(|\.autolaunch\s*\(|\.emit_to\s*\(|\.emit\s*\(|\bread_conn\s*\(|\bwrite_conn\s*\(|\bwith_write_tx\s*\()'
$guardPattern = [regex]'require_main_window\s*\(\s*window\.label\s*\(\s*\)\s*\)'
$commandAttrPattern = [regex]'#\s*\[\s*tauri\s*::\s*command[^\]]*\]'
$fnPattern = [regex]'(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\('

function Get-BraceBlockEnd {
  param(
    [string]$Text,
    [int]$OpenIndex
  )

  $depth = 0
  for ($i = $OpenIndex; $i -lt $Text.Length; $i++) {
    $ch = $Text[$i]
    if ($ch -eq '{') {
      $depth++
    }
    elseif ($ch -eq '}') {
      $depth--
      if ($depth -eq 0) {
        return $i
      }
    }
  }
  return -1
}

$discovered = @{}
$failures = New-Object System.Collections.Generic.List[string]
$rustFiles = Get-ChildItem -Path $commandsDir -Filter *.rs -File

foreach ($file in $rustFiles) {
  $content = Get-Content -Raw -Encoding UTF8 $file.FullName
  $attrMatches = $commandAttrPattern.Matches($content)
  foreach ($attr in $attrMatches) {
    $searchFrom = $attr.Index + $attr.Length
    $fnMatch = $fnPattern.Match($content, $searchFrom)
    if (-not $fnMatch.Success) {
      $failures.Add("$($file.Name): found #[tauri::command] without following fn")
      continue
    }

    $name = $fnMatch.Groups[1].Value
    $relative = $file.Name
    if ($discovered.ContainsKey($name)) {
      $failures.Add("Duplicate command definition: $name")
      continue
    }
    $discovered[$name] = $relative

    $braceOpen = $content.IndexOf('{', $fnMatch.Index + $fnMatch.Length)
    if ($braceOpen -lt 0) {
      $failures.Add("$relative::$name missing function body")
      continue
    }
    $braceClose = Get-BraceBlockEnd -Text $content -OpenIndex $braceOpen
    if ($braceClose -lt 0) {
      $failures.Add("$relative::$name could not parse function body braces")
      continue
    }

    $body = $content.Substring($braceOpen + 1, $braceClose - $braceOpen - 1)
    $class = $classified[$name]

    if (-not $class) {
      $failures.Add("$relative::$name is not in the command classification matrix (main-only or shared allowlist)")
      continue
    }

    $expectedFile = if ($class -eq "main-only") { $mainOnly[$name] } else { $sharedAllowlist[$name] }
    if ($expectedFile -ne $relative) {
      $failures.Add("$name expected in $expectedFile but found in $relative")
    }

    if ($class -eq "shared") {
      if ($guardPattern.IsMatch($body)) {
        $failures.Add("$relative::$name is shared/allowlisted and must not call require_main_window")
      }
      continue
    }

    # main-only: require_main_window(window.label()) must run before first side effect / state access
    $guardMatch = $guardPattern.Match($body)
    if (-not $guardMatch.Success) {
      $failures.Add("$relative::$name is main-only but missing require_main_window(window.label())")
      continue
    }

    $sideMatch = $sideEffectPattern.Match($body)
    if ($sideMatch.Success -and $sideMatch.Index -lt $guardMatch.Index) {
      $failures.Add("$relative::$name performs state access/side effect before require_main_window")
    }
  }
}

foreach ($name in $mainOnly.Keys) {
  if (-not $discovered.ContainsKey($name)) {
    $failures.Add("Matrix main-only command not found in source: $name ($($mainOnly[$name]))")
  }
}
foreach ($name in $sharedAllowlist.Keys) {
  if (-not $discovered.ContainsKey($name)) {
    $failures.Add("Matrix shared command not found in source: $name ($($sharedAllowlist[$name]))")
  }
}

if ($failures.Count -gt 0) {
  Write-Host "FAIL tauri command guard checks:"
  foreach ($failure in $failures) {
    Write-Host " - $failure"
  }
  exit 1
}

Write-Output "PASS tauri command guards checked ($($discovered.Count) commands; $($mainOnly.Count) main-only; $($sharedAllowlist.Count) shared)"
