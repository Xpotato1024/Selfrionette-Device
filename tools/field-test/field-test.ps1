$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$root = $PSScriptRoot
$exe = Join-Path $root 'selfrionettectl.exe'
$plan = Join-Path $root 'test-plan.json'
$manifest = Join-Path $root 'BUILD.json'
foreach ($path in @($exe, $plan, $manifest)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing kit file: $path" }
}
$build = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8 | ConvertFrom-Json
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $build.exe_sha256) { throw 'Executable hash mismatch' }
$selected = Get-Content -LiteralPath $plan -Raw -Encoding UTF8 | ConvertFrom-Json
Write-Host "Device label: $($selected.device_label); operator: $($selected.operator); posture: $($selected.posture)"
Write-Host 'Close Device Manager / Serial Monitor / Sim. Unload the sensors before opening.'
Write-Host 'Available ports (enumeration only):'
& $exe list
if ($LASTEXITCODE -ne 0) { throw 'Port enumeration failed' }
$port = Read-Host 'Type the confirmed port (for example COM5)'
if ($port -notmatch '^COM[0-9]{1,3}$') { throw 'Invalid COM port' }
$confirmation = Read-Host "Opening may reset the device. No commands will be sent. Type OPEN $port to continue"
if ($confirmation -cne "OPEN $port") { throw 'Not confirmed; no port opened' }
$out = Join-Path $root ('capture-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
& $exe field-test capture $plan --port $port --out $out --yes-open
$code = $LASTEXITCODE
if (Test-Path -LiteralPath $out -PathType Container) {
    Copy-Item -LiteralPath $manifest -Destination (Join-Path $out 'BUILD.json') -ErrorAction Stop
}
Write-Host "Exit: $code; records: $out"
Write-Host 'Keep aborted records too. Read review.md and fill notes.txt. No calibration was installed.'
exit $code
