param([string]$KitDirectory = '')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
if (-not $KitDirectory) { $KitDirectory = Join-Path $repo 'target\field-test-kit' }
$kit = (Resolve-Path -LiteralPath $KitDirectory).Path
$qa = Join-Path $repo ('target\field-test-plan-qa-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $qa) { throw 'Existing test QA folder; refusing overwrite' }
New-Item -ItemType Directory -Path $qa -ErrorAction Stop | Out-Null
$template = Join-Path $kit 'test-plan.json'
$exe = Join-Path $kit 'selfrionettectl.exe'
$setup = Join-Path $kit 'setup-plan.ps1'
function Scripted-Setup([string]$Name, [string[]]$Answers) {
    $target = Join-Path $qa ($Name + '.json')
    Copy-Item -LiteralPath $template -Destination $target
    $global:answers = New-Object 'System.Collections.Generic.Queue[string]'
    foreach ($answer in $Answers) { $global:answers.Enqueue($answer) }
    function global:Read-Host {
        param([string]$Prompt)
        if ($global:answers.Count -eq 0) { throw "Unexpected prompt: $Prompt" }
        return $global:answers.Dequeue()
    }
    try {
        & $setup -PlanPath $target
        if ($global:answers.Count -ne 0) { throw 'Script did not consume all answers' }
    } finally { Remove-Item Function:\Read-Host -ErrorAction SilentlyContinue }
    return $target
}
$quick = Scripted-Setup -Name 'quick' -Answers @('', '', 'y')
$parsed = Get-Content -LiteralPath $quick -Raw -Encoding UTF8 | ConvertFrom-Json
if ($parsed.device_label -ne 'PROVISIONAL-DEVICE-ID-UNKNOWN' -or $parsed.axes.Count -ne 7) {
    throw 'Quick metadata/axis count failed'
}
foreach ($axis in $parsed.axes) {
    if ($axis.negative -ne $null -or [string]::IsNullOrWhiteSpace($axis.positive) -or $axis.positive -match 'ONE SAFE direction') {
        throw 'Quick mode falsely establishes direction'
    }
}
$preview = @(& $exe field-test preview $quick)
if ($LASTEXITCODE -ne 0 -or $preview[0] -notmatch 'configured: true') {
    throw 'Quick mode cannot pass offline preview'
}
$mapped = Scripted-Setup -Name 'mapped' -Answers @('', 'little,thumb,index', 'y')
$mappedPlan = Get-Content -LiteralPath $mapped -Raw -Encoding UTF8 | ConvertFrom-Json
if ($mappedPlan.axes[0].name -notmatch 'little' -or
    $mappedPlan.axes[1].name -notmatch 'thumb' -or
    $mappedPlan.axes[4].name -notmatch 'index') {
    throw 'Optional finger mapping did not propagate into all seven cues'
}
$answers = @('n', 'device-A', 'operator-A', 'seated', 'unknown')
for ($i=0; $i -lt 7; $i++) { $answers += @("axis-$i", "move-$i", 'n') }
$answers += @('axis0 and axis4', 'y')
$full = Scripted-Setup -Name 'detailed' -Answers $answers
$preview = @(& $exe field-test preview $full)
if ($LASTEXITCODE -ne 0 -or $preview[0] -notmatch 'configured: true') {
    throw 'Detailed mode cannot pass offline preview'
}
$cancel = Join-Path $qa 'cancel.json'
Copy-Item -LiteralPath $template -Destination $cancel
$before = (Get-FileHash -LiteralPath $cancel -Algorithm SHA256).Hash
$global:answers = New-Object 'System.Collections.Generic.Queue[string]'
$global:answers.Enqueue(':cancel')
function global:Read-Host {param([string]$Prompt) return $global:answers.Dequeue() }
try {
    & $setup -PlanPath $cancel
    throw 'Cancel unexpectedly returned normally'
} catch {
    if ($_.Exception.Message -notmatch 'cancelled') { throw }
} finally { Remove-Item Function:\Read-Host -ErrorAction SilentlyContinue }
if ((Get-FileHash -LiteralPath $cancel -Algorithm SHA256).Hash -ne $before) {
    throw 'Cancel modified the original plan'
}
Write-Output 'PASS: quick mode, detailed mode, cancellation and file-only preview'
Write-Output 'No serial port opened or modified'
