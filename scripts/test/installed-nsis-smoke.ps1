[CmdletBinding(DefaultParameterSetName = "Smoke")]
param(
    [Parameter(Mandatory = $true, ParameterSetName = "Smoke")]
    [ValidateNotNullOrEmpty()]
    [string]$InstallerPath,

    [Parameter(Mandatory = $true, ParameterSetName = "ChildProcessSelfTest")]
    [switch]$ChildProcessSelfTest,

    [string]$EvidenceRoot = ".tmp\installed-nsis-smoke",

    [string]$FixtureDir = "scripts\test\markdown-import-fixtures",

    [ValidateRange(1024, 65533)]
    [int]$CdpPort = 9471,

    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Resolve-AbsolutePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Value
    )

    $Value | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Get-FileHashOrNull {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

function Join-NativeArguments {
    param([string[]]$Arguments)

    $quotedArguments = foreach ($argument in $Arguments) {
        if ($null -eq $argument -or $argument.Length -eq 0) {
            '""'
            continue
        }
        if ($argument -notmatch '[\s"]') {
            $argument
            continue
        }

        $builder = New-Object System.Text.StringBuilder
        [void]$builder.Append('"')
        $backslashCount = 0
        foreach ($character in $argument.ToCharArray()) {
            if ($character -eq '\') {
                $backslashCount++
                continue
            }
            if ($character -eq '"') {
                [void]$builder.Append(('\' * (($backslashCount * 2) + 1)))
                [void]$builder.Append('"')
            }
            else {
                [void]$builder.Append(('\' * $backslashCount))
                [void]$builder.Append($character)
            }
            $backslashCount = 0
        }
        [void]$builder.Append(('\' * ($backslashCount * 2)))
        [void]$builder.Append('"')
        $builder.ToString()
    }
    return $quotedArguments -join ' '
}

function Start-RedirectedChildProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @(),
        [string]$WorkingDirectory = (Get-Location).Path,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = Join-NativeArguments $Arguments
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start child process: $FilePath"
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    return [pscustomobject]@{
        Process = $process
        StdoutTask = $stdoutTask
        StderrTask = $stderrTask
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
    }
}

function Complete-RedirectedChildProcess {
    param([Parameter(Mandatory = $true)][object]$ChildProcess)

    [System.IO.File]::WriteAllText($ChildProcess.StdoutPath, $ChildProcess.StdoutTask.GetAwaiter().GetResult())
    [System.IO.File]::WriteAllText($ChildProcess.StderrPath, $ChildProcess.StderrTask.GetAwaiter().GetResult())
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $killInfo = New-Object System.Diagnostics.ProcessStartInfo
    $killInfo.FileName = "taskkill.exe"
    $killInfo.Arguments = "/PID $($Process.Id) /T /F"
    $killInfo.UseShellExecute = $false
    $killInfo.CreateNoWindow = $true
    $treeKiller = [System.Diagnostics.Process]::Start($killInfo)
    if (-not $treeKiller.WaitForExit(5000)) {
        $treeKiller.Kill()
        $treeKiller.WaitForExit()
    }
    if (-not $Process.HasExited) {
        $Process.Kill()
    }
}

function Wait-ForChildProcess {
    param(
        [Parameter(Mandatory = $true)][object]$Process,
        [Parameter(Mandatory = $true)][string]$Description,
        [ValidateRange(1, 600)][int]$TimeoutSeconds = 120
    )

    $processInstance = if ($Process -is [System.Diagnostics.Process]) { $Process } else { $Process.Process }
    if ($processInstance.WaitForExit($TimeoutSeconds * 1000)) {
        $processInstance.WaitForExit()
        $processInstance.Refresh()
        if ($Process -isnot [System.Diagnostics.Process]) {
            Complete-RedirectedChildProcess $Process
        }
        return $processInstance.ExitCode
    }

    try {
        $processInstance.Kill($true)
    }
    catch {
        Stop-ProcessTree -Process $processInstance
    }
    if (-not $processInstance.WaitForExit(10000)) {
        throw "$Description timed out after $TimeoutSeconds seconds and did not exit after termination."
    }
    $processInstance.WaitForExit()
    if ($Process -isnot [System.Diagnostics.Process]) {
        Complete-RedirectedChildProcess $Process
    }
    throw "$Description timed out after $TimeoutSeconds seconds and was terminated."
}

function Invoke-ChildProcessSelfTest {
    $testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "installed-nsis-child-process-$PID"
    [System.IO.Directory]::CreateDirectory($testRoot) | Out-Null
    $powershell = (Get-Process -Id $PID).Path
    try {
        foreach ($expectedExitCode in @(0, 7)) {
            $stdoutPath = Join-Path $testRoot "exit-$expectedExitCode.out.log"
            $stderrPath = Join-Path $testRoot "exit-$expectedExitCode.err.log"
            $command = "[Console]::Out.Write('stdout-$expectedExitCode'); [Console]::Error.Write('stderr-$expectedExitCode'); exit $expectedExitCode"
            $encodedCommand = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
            $child = Start-RedirectedChildProcess -FilePath $powershell -Arguments @('-NoProfile', '-EncodedCommand', $encodedCommand) -StdoutPath $stdoutPath -StderrPath $stderrPath
            $actualExitCode = Wait-ForChildProcess -Process $child -Description "exit $expectedExitCode self-test" -TimeoutSeconds 10
            if ($actualExitCode -ne $expectedExitCode) {
                throw "Expected exit code $expectedExitCode, got $actualExitCode."
            }
            if ([System.IO.File]::ReadAllText($stdoutPath) -ne "stdout-$expectedExitCode") {
                throw "stdout capture failed for exit code $expectedExitCode."
            }
            if ([System.IO.File]::ReadAllText($stderrPath) -notlike "*stderr-$expectedExitCode*") {
                throw "stderr capture failed for exit code $expectedExitCode."
            }
        }

        $timeoutStdoutPath = Join-Path $testRoot 'timeout.out.log'
        $timeoutStderrPath = Join-Path $testRoot 'timeout.err.log'
        $timeoutCommand = "[Console]::Out.Write('before-timeout'); Start-Sleep -Seconds 30"
        $encodedTimeoutCommand = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($timeoutCommand))
        $timeoutChild = Start-RedirectedChildProcess -FilePath $powershell -Arguments @('-NoProfile', '-EncodedCommand', $encodedTimeoutCommand) -StdoutPath $timeoutStdoutPath -StderrPath $timeoutStderrPath
        $timeoutStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            Wait-ForChildProcess -Process $timeoutChild -Description 'timeout self-test' -TimeoutSeconds 1 | Out-Null
            throw 'Timeout self-test did not time out.'
        }
        catch {
            if ($_.Exception.Message -notlike 'timeout self-test timed out*') {
                throw
            }
        }
        $timeoutStopwatch.Stop()
        if ($timeoutStopwatch.Elapsed.TotalSeconds -ge 15) {
            throw "Timeout self-test termination took $($timeoutStopwatch.Elapsed.TotalSeconds) seconds."
        }
        if ([System.IO.File]::ReadAllText($timeoutStdoutPath) -ne 'before-timeout') {
            throw 'stdout capture failed for timeout self-test.'
        }
        Write-Output 'PASS redirected child process self-test: exit=0, exit=7, stdout, stderr, timeout'
    }
    finally {
        Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-NodeRunner {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Description
    )

    $process = Start-Process -FilePath node -ArgumentList $Arguments -PassThru
    $exitCode = Wait-ForChildProcess -Process $process -Description $Description
    if ($exitCode -ne 0) {
        throw "$Description failed with exit code $exitCode."
    }
}

function Wait-ForCdp {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][datetime]$Deadline
    )

    $lastError = $null
    while ([DateTime]::UtcNow -lt $Deadline) {
        try {
            $targets = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/json" -TimeoutSec 2
            if (@($targets | Where-Object { $_.type -eq "page" -and $_.webSocketDebuggerUrl }).Count -gt 0) {
                return @($targets)
            }
        }
        catch {
            $lastError = $_.Exception.Message
        }
        Start-Sleep -Milliseconds 250
    }
    throw "CDP did not expose a page target on port $Port. Last error: $lastError"
}

function Start-InstalledApplication {
    param(
        [Parameter(Mandatory = $true)][string]$ExecutablePath,
        [Parameter(Mandatory = $true)][string]$TestRoot,
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $ExecutablePath
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.Environment["TWONOTES_TEST_ROOT"] = $TestRoot
    $startInfo.Environment["RUST_LOG"] = "info"
    $startInfo.Environment["RUST_BACKTRACE"] = "1"
    $startInfo.Environment["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = "--remote-debugging-port=$Port --enable-logging --v=1"

    $process = [System.Diagnostics.Process]::Start($startInfo)
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    return [pscustomobject]@{
        Process = $process
        StdoutTask = $stdoutTask
        StderrTask = $stderrTask
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
    }
}

function Stop-InstalledApplication {
    param([object]$Application)

    if ($null -eq $Application) {
        return
    }
    if (-not $Application.Process.HasExited) {
        $Application.Process.CloseMainWindow() | Out-Null
        if (-not $Application.Process.WaitForExit(8000)) {
            Stop-ProcessTree -Process $Application.Process
            if (-not $Application.Process.WaitForExit(10000)) {
                throw "Installed application did not exit after termination."
            }
            $Application.Process.WaitForExit()
        }
    }
    [System.IO.File]::WriteAllText($Application.StdoutPath, $Application.StdoutTask.GetAwaiter().GetResult())
    [System.IO.File]::WriteAllText($Application.StderrPath, $Application.StderrTask.GetAwaiter().GetResult())
}

if ($ChildProcessSelfTest) {
    Invoke-ChildProcessSelfTest
    return
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\.." )).Path
$installer = Resolve-AbsolutePath $InstallerPath
$fixtureDirectory = Resolve-AbsolutePath $FixtureDir
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
    throw "NSIS installer does not exist: $installer"
}
if (-not (Test-Path -LiteralPath $fixtureDirectory -PathType Container)) {
    throw "Markdown fixture directory does not exist: $fixtureDirectory"
}

$runId = "nsis-$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())-$PID"
$runRoot = Join-Path (Resolve-AbsolutePath $EvidenceRoot) $runId
$evidenceDirectory = Join-Path $runRoot "evidence"
$installDirectory = Join-Path $runRoot "installed"
$testRoot = Join-Path $runRoot "test-root"
$exportDirectory = Join-Path $runRoot "export"
$backupDirectory = Join-Path $testRoot "backups"
foreach ($directory in @($evidenceDirectory, $installDirectory, $testRoot, $exportDirectory)) {
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
}

$manifestPath = Join-Path $evidenceDirectory "manifest.json"
$summaryPath = Join-Path $evidenceDirectory "run-summary.json"
$mainScenarioPath = Join-Path $evidenceDirectory "main-scenario.json"
$application = $null
$runStartedAt = [DateTime]::UtcNow
$summary = [ordered]@{
    runId = $runId
    status = "running"
    startedAtUtc = $runStartedAt.ToString("o")
    installer = [ordered]@{
        path = $installer
        sha256 = Get-FileHashOrNull $installer
    }
    paths = [ordered]@{
        runRoot = $runRoot
        evidenceDirectory = $evidenceDirectory
        installDirectory = $installDirectory
        testRoot = $testRoot
        database = Join-Path $testRoot "data\2notes.sqlite"
        fixtureDirectory = $fixtureDirectory
        exportDirectory = $exportDirectory
        backupDirectory = $backupDirectory
    }
    environment = [ordered]@{
        powershell = $PSVersionTable.PSVersion.ToString()
        node = (node --version)
        python = (python --version 2>&1)
        git = (git -C $repoRoot rev-parse HEAD)
    }
    phases = @()
}

try {
    $installerProcess = Start-Process -FilePath $installer -ArgumentList @("/S", "/D=$installDirectory") -PassThru
    $installedExe = Join-Path $installDirectory "two_notes.exe"
    $summary.installer["exitCode"] = Wait-ForChildProcess -Process $installerProcess -Description "NSIS installer"
    $summary.installer["installedExe"] = $installedExe
    $summary.installer["installedExeSha256"] = Get-FileHashOrNull $installedExe
    if ($installerProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $installedExe -PathType Leaf)) {
        throw "NSIS installation failed: exit=$($installerProcess.ExitCode), exe=$installedExe"
    }
    $fileInfo = Get-Item -LiteralPath $installedExe
    $summary.installer["fileVersion"] = $fileInfo.VersionInfo.FileVersion
    $summary.installer["productVersion"] = $fileInfo.VersionInfo.ProductVersion
    Write-JsonFile -Path $manifestPath -Value $summary

    $quickCaptureScript = Join-Path $repoRoot "scripts\test\2notes-quick-capture-cdp.ps1"
    $quickCaptureStdoutPath = Join-Path $evidenceDirectory "quick-capture.log"
    $quickCaptureStderrPath = Join-Path $evidenceDirectory "quick-capture.err.log"
    $quickCaptureProcess = Start-RedirectedChildProcess -FilePath powershell -Arguments @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $quickCaptureScript,
        "-ExePath", $installedExe, "-AppDataRoot", $testRoot, "-CdpPort", $CdpPort, "-KeepTestData"
    ) -WorkingDirectory $repoRoot -StdoutPath $quickCaptureStdoutPath -StderrPath $quickCaptureStderrPath
    Wait-ForChildProcess -Process $quickCaptureProcess -Description "Installed quick-capture CDP smoke" -TimeoutSeconds 120 | Out-Null
    $quickCaptureExitCode = $quickCaptureProcess.Process.ExitCode
    if ($quickCaptureExitCode -ne 0) {
        throw "Installed quick-capture smoke failed with exit code $quickCaptureExitCode."
    }
    $quickCaptureTitleLine = Get-Content -LiteralPath $quickCaptureStdoutPath -Encoding utf8 |
        Where-Object { $_ -like "QUICK_CAPTURE_TITLE=*" } |
        Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($quickCaptureTitleLine)) {
        throw "Installed quick-capture smoke did not emit QUICK_CAPTURE_TITLE."
    }
    $quickCaptureTitle = $quickCaptureTitleLine.Substring("QUICK_CAPTURE_TITLE=".Length).Trim()
    if ([string]::IsNullOrWhiteSpace($quickCaptureTitle)) {
        throw "Installed quick-capture smoke emitted an empty quick-capture title."
    }
    $summary.quickCaptureTitle = $quickCaptureTitle
    $summary.phases += [ordered]@{ name = "quick_capture"; ok = $true }

    $mainPort = $CdpPort + 1
    $application = Start-InstalledApplication -ExecutablePath $installedExe -TestRoot $testRoot -Port $mainPort -StdoutPath (Join-Path $evidenceDirectory "main-stdout.log") -StderrPath (Join-Path $evidenceDirectory "main-stderr.log")
    $targets = Wait-ForCdp -Port $mainPort -Deadline ([DateTime]::UtcNow.AddSeconds(30))
    Write-JsonFile -Path (Join-Path $evidenceDirectory "main-targets.json") -Value $targets

    $mainRunner = Join-Path $repoRoot "scripts\test\installed-nsis-main-smoke.mjs"
    if (-not (Test-Path -LiteralPath $mainRunner -PathType Leaf)) {
        throw "Main CDP runner is missing: $mainRunner"
    }
    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath) -Description "Installed main-window CDP smoke"
    $scenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    $scenario | Add-Member -NotePropertyName quickCapture -NotePropertyValue ([pscustomobject]@{
        title = $quickCaptureTitle
    }) -Force
    Write-JsonFile -Path $mainScenarioPath -Value $scenario
    $summary.phases += [ordered]@{ name = "main_window"; ok = $true }

    $importSelectLog = Join-Path $evidenceDirectory "import-select.log"
    $importNodeProcess = Start-RedirectedChildProcess -FilePath node -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "import-select") -WorkingDirectory $repoRoot -StdoutPath $importSelectLog -StderrPath (Join-Path $evidenceDirectory "import-select.err.log")
    Start-Sleep -Milliseconds 1200
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\test\installed-nsis-dialog.ps1") -DirectoryPath $fixtureDirectory -EvidencePath (Join-Path $evidenceDirectory "import-dialog.json") -TestProcessId $application.Process.Id
    $importNodeExitCode = Wait-ForChildProcess -Process $importNodeProcess -Description "Installed Markdown import directory selection"
    if ($importNodeExitCode -ne 0) {
        throw "Installed Markdown import directory selection failed with exit code $importNodeExitCode."
    }
    $importScenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    if (-not $importScenario.ok -or -not (@($importScenario.phases | Where-Object { $_.name -eq "import-select" }).Count)) {
        throw "Installed Markdown import directory selection did not produce successful scenario evidence."
    }
    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "import-commit") -Description "Installed Markdown first import"
    $repeatImportSelectLog = Join-Path $evidenceDirectory "import-repeat-select.log"
    $repeatImportNodeProcess = Start-RedirectedChildProcess -FilePath node -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "import-select") -WorkingDirectory $repoRoot -StdoutPath $repeatImportSelectLog -StderrPath (Join-Path $evidenceDirectory "import-repeat-select.err.log")
    Start-Sleep -Milliseconds 1200
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\test\installed-nsis-dialog.ps1") -DirectoryPath $fixtureDirectory -EvidencePath (Join-Path $evidenceDirectory "import-repeat-dialog.json") -TestProcessId $application.Process.Id
    $repeatImportNodeExitCode = Wait-ForChildProcess -Process $repeatImportNodeProcess -Description "Installed Markdown repeat directory selection"
    if ($repeatImportNodeExitCode -ne 0) {
        throw "Installed Markdown repeat directory selection failed with exit code $repeatImportNodeExitCode."
    }
    $repeatImportScenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    if (-not $repeatImportScenario.ok -or -not (@($repeatImportScenario.phases | Where-Object { $_.name -eq "import-select" }).Count)) {
        throw "Installed Markdown repeat directory selection did not produce successful scenario evidence."
    }
    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "import-commit") -Description "Installed Markdown repeat import"
    $summary.phases += [ordered]@{ name = "markdown_import"; ok = $true }

    $exportSelectLog = Join-Path $evidenceDirectory "export-select.log"
    $exportNodeProcess = Start-RedirectedChildProcess -FilePath node -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "export-select") -WorkingDirectory $repoRoot -StdoutPath $exportSelectLog -StderrPath (Join-Path $evidenceDirectory "export-select.err.log")
    Start-Sleep -Milliseconds 1200
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\test\installed-nsis-dialog.ps1") -DirectoryPath $exportDirectory -EvidencePath (Join-Path $evidenceDirectory "export-dialog.json") -TestProcessId $application.Process.Id
    $exportNodeExitCode = Wait-ForChildProcess -Process $exportNodeProcess -Description "Installed Markdown export directory selection"
    if ($exportNodeExitCode -ne 0) {
        throw "Installed Markdown export directory selection failed with exit code $exportNodeExitCode."
    }
    $exportScenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    if (-not $exportScenario.ok -or -not (@($exportScenario.phases | Where-Object { $_.name -eq "export-select" }).Count)) {
        throw "Installed Markdown export directory selection did not produce successful scenario evidence."
    }
    $exportFiles = @(Get-ChildItem -LiteralPath $exportDirectory -Filter "*.md" -Recurse)
    if ($exportFiles.Count -lt 1) {
        throw "Installed Markdown export produced no Markdown files."
    }
    $scenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    $sourceTitle = [string]$scenario.inputs.sourceTitle
    $targetTitle = [string]$scenario.inputs.targetTitle
    $exportContents = @($exportFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding utf8 }) -join "`n"
    if (-not $exportContents.Contains($sourceTitle) -or -not $exportContents.Contains($targetTitle)) {
        throw "Installed Markdown export is missing smoke source or target content."
    }
    $exportManifest = [ordered]@{ files = @($exportFiles | ForEach-Object { @{ path = $_.FullName } }) }
    Write-JsonFile -Path (Join-Path $evidenceDirectory "export-manifest.json") -Value $exportManifest
    if ($null -eq $scenario.stages) {
        $scenario | Add-Member -NotePropertyName stages -NotePropertyValue ([pscustomobject]@{})
    }
    $scenario.stages | Add-Member -NotePropertyName export -NotePropertyValue ([pscustomobject]@{
        manifest = "export-manifest.json"
        sourceTitle = $sourceTitle
        targetTitle = $targetTitle
    }) -Force
    Write-JsonFile -Path $mainScenarioPath -Value $scenario
    $summary.phases += [ordered]@{ name = "markdown_export"; ok = $true }

    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "backup-create") -Description "Installed backup creation smoke"
    $scenario = Get-Content -LiteralPath $mainScenarioPath -Raw -Encoding utf8 | ConvertFrom-Json
    if ($null -eq $scenario.stages -or $null -eq $scenario.stages.backup) {
        throw "Backup creation did not write the expected scenario stage."
    }
    $scenario.stages.backup | Add-Member -NotePropertyName backupDir -NotePropertyValue $backupDirectory -Force
    Write-JsonFile -Path $mainScenarioPath -Value $scenario
    $summary.phases += [ordered]@{ name = "backup_create"; ok = $true }

    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "backup-mutation") -Description "Installed backup mutation smoke"
    $summary.phases += [ordered]@{ name = "backup_mutation"; ok = $true }

    $database = $summary.paths.database
    if (-not (Test-Path -LiteralPath $database -PathType Leaf)) {
        throw "Installed application did not create an isolated database before backup restore: $database"
    }
    & python (Join-Path $repoRoot "scripts\test\installed-nsis-evidence.py") --run-id $runId --db $database --scenario-ids $mainScenarioPath --evidence-dir $evidenceDirectory --mode pre-restore *> (Join-Path $evidenceDirectory "pre-restore-evidence.log")
    if ($LASTEXITCODE -ne 0) {
        throw "Pre-restore SQLite mutation evidence validation failed with exit code $LASTEXITCODE."
    }
    $summary.phases += [ordered]@{ name = "pre_restore_evidence"; ok = $true }

    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $mainPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "backup-restore") -Description "Installed backup restore smoke"
    $summary.phases += [ordered]@{ name = "backup_restore"; ok = $true }

    Stop-InstalledApplication $application
    $application = $null

    $restartPort = $mainPort + 1
    $application = Start-InstalledApplication -ExecutablePath $installedExe -TestRoot $testRoot -Port $restartPort -StdoutPath (Join-Path $evidenceDirectory "restart-stdout.log") -StderrPath (Join-Path $evidenceDirectory "restart-stderr.log")
    $restartTargets = Wait-ForCdp -Port $restartPort -Deadline ([DateTime]::UtcNow.AddSeconds(30))
    Write-JsonFile -Path (Join-Path $evidenceDirectory "restart-targets.json") -Value $restartTargets
    Invoke-NodeRunner -Arguments @($mainRunner, "--port", $restartPort, "--evidence-dir", $evidenceDirectory, "--scenario-path", $mainScenarioPath, "--mode", "restart-verify") -Description "Installed restart verification smoke"
    $summary.phases += [ordered]@{ name = "restart"; ok = $true }

    Stop-InstalledApplication $application
    $application = $null

    $database = $summary.paths.database
    if (-not (Test-Path -LiteralPath $database -PathType Leaf)) {
        throw "Installed application did not create an isolated database: $database"
    }
    & python (Join-Path $repoRoot "scripts\test\installed-nsis-evidence.py") --run-id $runId --db $database --scenario-ids $mainScenarioPath --evidence-dir $evidenceDirectory *> (Join-Path $evidenceDirectory "sqlite-evidence.log")
    if ($LASTEXITCODE -ne 0) {
        throw "SQLite installed-package evidence validation failed with exit code $LASTEXITCODE."
    }
    $summary.phases += [ordered]@{ name = "sqlite_evidence"; ok = $true }

    $summary.status = "passed"
}
catch {
    $summary.status = "failed"
    $summary.error = $_.Exception.Message
    throw
}
finally {
    Stop-InstalledApplication $application
    $summary.completedAtUtc = [DateTime]::UtcNow.ToString("o")
    Write-JsonFile -Path $summaryPath -Value $summary
    Write-Output "NSIS_SMOKE_RUN_ID=$runId"
    Write-Output "NSIS_SMOKE_EVIDENCE=$evidenceDirectory"
    Write-Output "NSIS_SMOKE_STATUS=$($summary.status)"
}
