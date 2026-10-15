param(
    [Parameter(Mandatory = $true)]
    [string]$PlanPath
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
if (-not (Test-Path -LiteralPath $PlanPath -PathType Leaf)) {
    throw "Plan file not found: $PlanPath"
}
$plan = Get-Content -LiteralPath $PlanPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($plan.schema_version -ne 1 -or $plan.axes.Count -ne 7) {
    throw 'Unsupported plan schema. Exactly seven axes are required.'
}
function Read-Required([string]$Prompt, [string]$Existing = '') {
    while ($true) {
        $hint = if ([string]::IsNullOrWhiteSpace($Existing)) { '' } else { " [$Existing]" }
        $value = Read-Host "$Prompt$hint"
        if ($value -ceq ':cancel') { throw 'Setup cancelled; plan was not modified' }
        if ([string]::IsNullOrWhiteSpace($value)) { $value = $Existing }
        $bad = [string]::IsNullOrWhiteSpace($value) -or
            [System.Text.Encoding]::UTF8.GetByteCount($value) -gt 512 -or
            ($value -match '[\x00-\x1F\x7F]')
        if (-not $bad) { return $value.Trim() }
        Write-Host 'Enter nonempty, single-line text under 512 UTF-8 bytes.'
    }
}
Write-Host 'Selfrionette field-test setup: offline only; no COM port is opened.'
Write-Host 'Enter :cancel at any prompt to abort without saving.'
Write-Host 'Quick mode: provisional 1+3+3 labels, no verified channel/direction assignments.'
$mode = Read-Host 'Quick initial observation with no metadata entry? [Y/n]'
if ($mode -ceq ':cancel') { throw 'Setup cancelled; original plan unchanged' }
if ([string]::IsNullOrWhiteSpace($mode) -or $mode -match '^(y|yes)$') {
    $plan.device_label = 'PROVISIONAL-DEVICE-ID-UNKNOWN'
    $plan.operator = 'not-recorded'
    $plan.posture = 'not-recorded'
    $plan.board = 'unknown'
    $plan.firmware_source = 'unknown'
    $quickAxes = @('1DOF finger', '3DOF finger A - axis 1', '3DOF finger A - axis 2', '3DOF finger A - axis 3',
        '3DOF finger B - axis 1', '3DOF finger B - axis 2', '3DOF finger B - axis 3')
    for ($i = 0; $i -lt 7; $i++) {
        $plan.axes[$i].name = ('PROVISIONAL ' + $quickAxes[$i])
        $plan.axes[$i].positive = 'Gently move ' + $quickAxes[$i] + ' in ONE SAFE direction. Physical direction NOT YET DEFINED.'
        $plan.axes[$i].negative = $null
    }
    $plan.combined_instruction = 'If safe, gently operate axis 0 and axis 1 together; otherwise note that the step was skipped.'
    Write-Host 'Quick plan is for provisional diagnostics only, NOT polarity or operator-intent calibration.'
    Write-Host 'The script cannot know your wiring or physical positive directions. Record actual motions in notes.txt.'
    Write-Host 'You will only need to confirm saving, then select the COM port.'
} elseif ($mode -match '^(n|no)$') {
$plan.device_label = Read-Required 'Device label (e.g. A)' $plan.device_label
$plan.operator = Read-Required 'Operator label (pseudonym is fine)' $plan.operator
$plan.posture = Read-Required 'Posture (e.g. seated, arm supported)' $plan.posture
$plan.board = Read-Required 'Board (type unknown if not verified)' $plan.board
if ([string]::IsNullOrWhiteSpace($plan.firmware_source)) {
    $plan.firmware_source = Read-Required 'Firmware source (unknown is acceptable)'
}
Write-Host 'Axes 0-6 are OPERATOR INTENT labels, not necessarily wire ch0-ch6.'
Write-Host 'Describe the actual finger and movement for each axis. Do not guess wiring.'
for ($i = 0; $i -lt 7; $i++) {
    $axis = $plan.axes[$i]
    $currentName = [string]$axis.name
    if ($currentName -like '???*ch*') { $currentName = '' }
    if ($currentName -match '^.*ch.*\)$') { $currentName = '' }
    $axis.name = Read-Required "Axis $i label (finger and intended motion)" $currentName
    $axis.positive = Read-Required "Axis $i positive-direction instruction" $axis.positive
    if ([string]::IsNullOrWhiteSpace($axis.negative)) {
        $answer = Read-Host "Axis ${i}: can you safely apply reverse force? [y/N]"
        if ($answer -ceq ':cancel') { throw 'Setup cancelled; original plan unchanged' }
        if ($answer -match '^(y|yes)$') {
            $axis.negative = Read-Required "Axis $i reverse-direction instruction"
        } elseif (-not [string]::IsNullOrWhiteSpace($answer) -and $answer -notmatch '^(n|no)$') {
            throw 'Answer reverse question with y/n; original plan unchanged'
        }
    } else {
        $axis.negative = Read-Required "Axis $i reverse-direction instruction" $axis.negative
    }
}
$plan.combined_instruction = Read-Required 'Simultaneous two-axis instruction' $plan.combined_instruction
} else {
    throw 'Choose Y or N; original plan unchanged'
}
$names = @($plan.axes | ForEach-Object { $_.name })
if ((@($names | Select-Object -Unique)).Count -ne 7) {
    throw 'Axis labels must be unique; original plan unchanged'
}
Write-Host 'Review planned axes:'
for ($i = 0; $i -lt 7; $i++) {
    Write-Host ("  {0}: {1} | positive: {2} | reverse: {3}" -f $i, $plan.axes[$i].name, $plan.axes[$i].positive, $plan.axes[$i].negative)
}
Write-Host "Combined: $($plan.combined_instruction)"
$confirm = Read-Host 'Save configured plan? [y/N]'
if ($confirm -notmatch '^(y|yes)$') { throw 'Setup cancelled; original plan unchanged' }
$full = [System.IO.Path]::GetFullPath($PlanPath)
$backup = $full + '.backup-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
Copy-Item -LiteralPath $full -Destination $backup -ErrorAction Stop
$json = ($plan | ConvertTo-Json -Depth 8) + [Environment]::NewLine
[System.IO.File]::WriteAllText($full, $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Saved: $full"
Write-Host "Original backup: $backup"
Write-Host 'This does not establish axis-to-channel mapping or hardware calibration.'
