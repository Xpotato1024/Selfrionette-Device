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
