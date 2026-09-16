$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"

# The Rust test is the canonical renderer. Its GENERATE_TYPES mode writes the
# declarations to the checked-in frontend path instead of only comparing them.
$env:GENERATE_TYPES = "1"
& cargo test --manifest-path $manifestPath types::tests::generated_typescript_is_current
Remove-Item Env:GENERATE_TYPES -ErrorAction SilentlyContinue
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
