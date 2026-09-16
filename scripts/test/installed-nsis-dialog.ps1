[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [Alias("Directory", "Path")]
    [ValidateNotNullOrEmpty()]
    [string]$DirectoryPath,

    [Parameter(Mandatory = $true)]
    [Alias("Evidence")]
    [ValidateNotNullOrEmpty()]
    [string]$EvidencePath,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, [int]::MaxValue)]
    [int]$TestProcessId,

    [Alias("Timeout")]
    [ValidateRange(1, 300)]
    [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Evidence {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Payload
    )

    $parent = Split-Path -Parent $EvidencePath
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        [System.IO.Directory]::CreateDirectory($parent) | Out-Null
    }

    $json = $Payload | ConvertTo-Json -Depth 12
    $utf8WithoutBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($EvidencePath, $json, $utf8WithoutBom)
}

function Get-ElementProperty {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Element,
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationProperty]$Property
    )

    try {
        return $Element.GetCurrentPropertyValue($Property, $true)
    }
    catch {
        return $null
    }
}

function Get-ElementSummary {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Element
    )

    $rect = Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::BoundingRectangleProperty)
    $bounds = $null
    $rectProperties = $rect.PSObject.Properties
    if (
        $null -ne $rect -and
        $null -ne $rectProperties["Width"] -and
        $null -ne $rectProperties["Height"] -and
        [double]$rect.Width -gt 0 -and
        [double]$rect.Height -gt 0
    ) {
        $bounds = @{
            left = [Math]::Round([double]$rect.Left, 0)
            top = [Math]::Round([double]$rect.Top, 0)
            width = [Math]::Round([double]$rect.Width, 0)
            height = [Math]::Round([double]$rect.Height, 0)
        }
    }

    return [ordered]@{
        name = [string](Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::NameProperty))
        automationId = [string](Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::AutomationIdProperty))
        className = [string](Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::ClassNameProperty))
        controlType = [string](Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::ControlTypeProperty))
        processId = Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::ProcessIdProperty)
        isEnabled = Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::IsEnabledProperty)
        isOffscreen = Get-ElementProperty $Element ([System.Windows.Automation.AutomationElement]::IsOffscreenProperty)
        bounds = $bounds
    }
}

function Get-ControlTree {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Root,
        [int]$MaximumDepth = 6,
        [int]$MaximumNodes = 400
    )

    $nodes = [System.Collections.Generic.List[object]]::new()
    $queue = [System.Collections.Generic.Queue[object]]::new()
    $queue.Enqueue(@($Root, 0))

    while ($queue.Count -gt 0 -and $nodes.Count -lt $MaximumNodes) {
        $item = $queue.Dequeue()
        $element = $item[0]
        $depth = [int]$item[1]
        $summary = Get-ElementSummary $element
        $summary["depth"] = $depth
        $nodes.Add($summary)

        if ($depth -ge $MaximumDepth) {
            continue
        }

        try {
            $children = $element.FindAll(
                [System.Windows.Automation.TreeScope]::Children,
                [System.Windows.Automation.Condition]::TrueCondition
            )
            foreach ($child in $children) {
                $queue.Enqueue(@($child, $depth + 1))
            }
        }
        catch {
            # A native dialog can disappear while its UI Automation tree is enumerated.
        }
    }

    return $nodes.ToArray()
}

function Get-TopLevelWindows {
    $desktop = [System.Windows.Automation.AutomationElement]::RootElement
    $windows = $desktop.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Window
        ))
    )

    return @($windows | ForEach-Object { Get-ElementSummary $_ })
}

function Test-IsFolderDialog {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Window
    )

    $summary = Get-ElementSummary $Window
    if ($summary.isOffscreen -or -not $summary.isEnabled) {
        return $false
    }

    $title = $summary.name
    $titleLooksLikeFolderDialog = $title -match '(?i)(select|browse|choose).{0,24}(folder|directory)|folder.{0,24}(select|browse|choose)|选择.{0,12}(文件夹|目录)|浏览.{0,12}(文件夹|目录)|文件夹|目录'
    $classLooksLikeCommonDialog = $summary.className -eq '#32770'
    if (-not $titleLooksLikeFolderDialog -and -not $classLooksLikeCommonDialog) {
        return $false
    }

    $buttons = $Window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        ))
    )
    foreach ($button in $buttons) {
        $name = [string](Get-ElementProperty $button ([System.Windows.Automation.AutomationElement]::NameProperty))
        if ($name -match '(?i)^(select( folder)?|choose|ok|browse|选择(文件夹)?|确定|浏览)$') {
            return $true
        }
    }

    return $false
}

function Find-FolderDialog {
    param(
        [Parameter(Mandatory = $true)]
        [int]$TestProcessId
    )

    $desktop = [System.Windows.Automation.AutomationElement]::RootElement
    $windows = $desktop.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Window
        ))
    )

    foreach ($window in $windows) {
        $windowProcessId = Get-ElementProperty $window ([System.Windows.Automation.AutomationElement]::ProcessIdProperty)
        if ($windowProcessId -eq $TestProcessId -and (Test-IsFolderDialog $window)) {
            return $window
        }
    }

    return $null
}

function Find-ConfirmationButton {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Dialog
    )

    $buttons = $Dialog.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        ))
    )

    $preferredNames = @(
        'Select Folder', 'Select', 'Choose', 'OK',
        '选择文件夹', '选择', '确定'
    )
    foreach ($preferredName in $preferredNames) {
        foreach ($button in $buttons) {
            $name = [string](Get-ElementProperty $button ([System.Windows.Automation.AutomationElement]::NameProperty))
            $enabled = Get-ElementProperty $button ([System.Windows.Automation.AutomationElement]::IsEnabledProperty)
            if ($enabled -and $name -ieq $preferredName) {
                return $button
            }
        }
    }

    return $null
}

function Set-SelectedFolder {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Dialog,
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $Dialog.SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait('^l')
    Start-Sleep -Milliseconds 150

    $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
    $valuePattern = $null
    if ($focused -and $focused.TryGetCurrentPattern(
            [System.Windows.Automation.ValuePattern]::Pattern,
            [ref]$valuePattern
        )) {
        $valuePattern.SetValue($Path)
        $currentValue = [string]$valuePattern.Current.Value
        if (-not $currentValue.Equals($Path, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "The folder dialog location field did not retain the requested directory. Expected '$Path', got '$currentValue'."
        }
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
        Start-Sleep -Milliseconds 250
        return [ordered]@{
            focusedControl = Get-ElementSummary $focused
            selectedPath = $currentValue
        }
    }
    throw 'The folder dialog location field does not expose the UI Automation ValuePattern after Ctrl+L.'
}

function Wait-Until {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$Condition,
        [Parameter(Mandatory = $true)]
        [datetime]$Deadline,
        [string]$Description = 'condition'
    )

    while ([DateTime]::UtcNow -lt $Deadline) {
        $result = & $Condition
        if ($null -ne $result) {
            return $result
        }
        Start-Sleep -Milliseconds 200
    }

    throw "Timed out waiting for $Description."
}

$startedAt = [DateTime]::UtcNow
$dialog = $null
$resolvedDirectoryPath = $null

try {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Add-Type -AssemblyName System.Windows.Forms

    $resolvedDirectoryPath = (Resolve-Path -LiteralPath $DirectoryPath -ErrorAction Stop).Path
    if (-not (Test-Path -LiteralPath $resolvedDirectoryPath -PathType Container)) {
        throw "DirectoryPath is not a directory: $resolvedDirectoryPath"
    }

    $deadline = $startedAt.AddSeconds($TimeoutSeconds)
    $dialog = Wait-Until -Deadline $deadline -Description 'a native folder selection dialog' -Condition {
        Find-FolderDialog -TestProcessId $TestProcessId
    }

    $dialogBefore = Get-ElementSummary $dialog
    $selection = Set-SelectedFolder -Dialog $dialog -Path $resolvedDirectoryPath

    Start-Sleep -Milliseconds 300
    $confirmationButton = Find-ConfirmationButton $dialog
    if ($null -eq $confirmationButton) {
        throw 'Could not find an enabled confirmation button with a supported Chinese or English label.'
    }

    $invokePattern = $null
    if (-not $confirmationButton.TryGetCurrentPattern(
            [System.Windows.Automation.InvokePattern]::Pattern,
            [ref]$invokePattern
        )) {
        throw 'The folder dialog confirmation button does not expose the UI Automation InvokePattern.'
    }
    $invokePattern.Invoke()

    Wait-Until -Deadline $deadline -Description 'the folder dialog to close' -Condition {
        try {
            if ($dialog.Current.IsOffscreen) {
                return $true
            }
            $null
        }
        catch {
            return $true
        }
    } | Out-Null

    Write-Evidence ([ordered]@{
        status = 'success'
        startedAtUtc = $startedAt.ToString('o')
        completedAtUtc = [DateTime]::UtcNow.ToString('o')
        directoryPath = $resolvedDirectoryPath
        testProcessId = $TestProcessId
        timeoutSeconds = $TimeoutSeconds
        dialog = $dialogBefore
        selection = $selection
        confirmationButton = Get-ElementSummary $confirmationButton
    })
}
catch {
    $dialogSummary = $null
    $controlTree = @()
    if ($null -ne $dialog) {
        try {
            $dialogSummary = Get-ElementSummary $dialog
            $controlTree = Get-ControlTree $dialog
        }
        catch {
            # Preserve the original failure if diagnostic collection also fails.
        }
    }

    $evidenceDirectoryPath = $resolvedDirectoryPath
    if ($null -eq $evidenceDirectoryPath) {
        $evidenceDirectoryPath = $DirectoryPath
    }

    $failureEvidence = [ordered]@{
        status = 'failure'
        startedAtUtc = $startedAt.ToString('o')
        failedAtUtc = [DateTime]::UtcNow.ToString('o')
        directoryPath = $evidenceDirectoryPath
        testProcessId = $TestProcessId
        timeoutSeconds = $TimeoutSeconds
        error = $_.Exception.Message
        dialog = $dialogSummary
        controlTree = $controlTree
        topLevelWindows = @(Get-TopLevelWindows)
    }

    try {
        Write-Evidence $failureEvidence
    }
    catch {
        Write-Error "Could not write failure evidence to '$EvidencePath': $($_.Exception.Message)"
    }

    throw
}
