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
    $quickLabels = @(
        '1軸入力部（指未同定）', '3軸入力部A・左右', '3軸入力部A・前後',
        '3軸入力部A・上下', '3軸入力部B・左右', '3軸入力部B・前後', '3軸入力部B・上下'
    )
    $quickInstructions = @(
        '1軸の入力部を安全な押込み方向へごく軽く押す。入力部が分からなければスキップ',
        '3軸入力部Aに操作者から見て右向きにごく軽い力を加える。Aが不明・不可能ならスキップ',
        '3軸入力部Aに操作者から見て奥向きにごく軽い力を加える。Aが不明・不可能ならスキップ',
        '3軸入力部Aに鉛直上向きにごく軽い力を加える。Aが不明・不可能ならスキップ',
        'もう一方の3軸入力部Bに操作者から見て右向きにごく軽い力を加える。Bが不明・不可能ならスキップ',
        'もう一方の3軸入力部Bに操作者から見て奥向きにごく軽い力を加える。Bが不明・不可能ならスキップ',
        'もう一方の3軸入力部Bに鉛直上向きにごく軽い力を加える。Bが不明・不可能ならスキップ'
    )
    for ($i = 0; $i -lt 7; $i++) {
        $plan.axes[$i].name = $quickLabels[$i]
        $plan.axes[$i].positive = $quickInstructions[$i]
        $plan.axes[$i].negative = $null
    }
    $mapping = Read-Host 'Optional finger names for 1DOF,A,B (e.g. little,thumb,index; Enter=unknown)'
    if ($mapping -ceq ':cancel') { throw 'Setup cancelled; original plan unchanged' }
    if (-not [string]::IsNullOrWhiteSpace($mapping)) {
        $names = @($mapping.Split(',') | ForEach-Object { $_.Trim() })
        if ($names.Count -ne 3 -or @($names | Where-Object { [string]::IsNullOrWhiteSpace($_) -or [Text.Encoding]::UTF8.GetByteCount($_) -gt 80 }).Count -gt 0) {
            throw 'Supply exactly three comma-separated finger names, or press Enter for unknown'
        }
        $plan.axes[0].name = ($names[0] + ' / 1DOF')
        $plan.axes[0].positive = $plan.axes[0].positive.Replace('1軸の入力部', ($names[0] + 'の1軸入力部'))
        for ($i = 1; $i -le 3; $i++) {
            $plan.axes[$i].name = $plan.axes[$i].name.Replace('3軸入力部A', ($names[1] + ' / 3DOF A'))
            $plan.axes[$i].positive = $plan.axes[$i].positive.Replace('3軸入力部A', ($names[1] + 'の3軸入力部A'))
        }
        for ($i = 4; $i -le 6; $i++) {
            $plan.axes[$i].name = $plan.axes[$i].name.Replace('3軸入力部B', ($names[2] + ' / 3DOF B'))
            $plan.axes[$i].positive = $plan.axes[$i].positive.Replace('3軸入力部B', ($names[2] + 'の3軸入力部B'))
        }
    }
    $plan.combined_instruction = '安全な場合のみ、1軸入力部と3軸指Aを同時に軽く押す。方向が不明なら s+Enter でスキップ'
    Write-Host 'Quick observation is exploratory: A/B finger identity and actual channel mapping are UNVERIFIED.'
    Write-Host 'Each trial will pause for Enter. Use s+Enter if the suggested direction is unsafe or unclear.'
    Write-Host 'Only confirm saving, then select the COM port.'
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
