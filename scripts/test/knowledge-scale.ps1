$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"

function Invoke-KnowledgeScaleTest {
    param([Parameter(Mandatory = $true)][string]$Filter)

    & cargo test --manifest-path $manifestPath --release --lib $Filter -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Invoke-KnowledgeScaleTest "knowledge_scale_10000_entries"
Invoke-KnowledgeScaleTest "saves_one_hundred_wiki_links"
exit 0
