$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"

function Invoke-BlockDocumentScaleTest {
    param([Parameter(Mandatory = $true)][string]$Filter)

    & cargo test --manifest-path $manifestPath --release --lib $Filter -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

$tests = @(
    "migration_v4_to_v5_projects_ten_thousand_entries_under_thirty_seconds",
    "saves_one_hundred_block_document_with_p95_under_one_hundred_milliseconds",
    "opens_five_hundred_block_document_with_p95_under_one_hundred_fifty_milliseconds",
    "loads_ten_thousand_node_knowledge_tree_with_p95_under_two_hundred_milliseconds",
    "health_issue_page_fifty_rows_has_p95_under_one_hundred_fifty_milliseconds",
    "imports_one_thousand_small_markdown_files_under_sixty_seconds"
)

foreach ($test in $tests) {
    Invoke-BlockDocumentScaleTest $test
}

exit 0
