param(
    [string]$OutputDirectory = ''
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repo 'target\field-test-kit'
}
$destination = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $destination) {
    throw "Kit output already exists: $destination"
}
$binary = Join-Path $repo 'target\debug\selfrionettectl.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw 'Run cargo +stable build --locked -p selfrionettectl before packaging'
}
$source = (& git -C $repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $source -notmatch '^[a-f0-9]{40}$') {
    throw 'Unable to resolve source commit'
}
New-Item -ItemType Directory -Path $destination -ErrorAction Stop | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $destination 'selfrionettectl.exe')
foreach ($name in @('field-test.cmd', 'field-test.ps1', 'setup-plan.ps1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $destination $name)
}
Copy-Item -LiteralPath (Join-Path $repo 'docs\operations\field-test-30-60min.md') -Destination (Join-Path $destination 'README.md')
$kitExe = Join-Path $destination 'selfrionettectl.exe'
$plan = Join-Path $destination 'test-plan.json'
& $kitExe field-test template $plan
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $plan -PathType Leaf)) {
    throw 'Failed to generate field-test plan template'
}
$preview = & $kitExe field-test preview $plan
if ($LASTEXITCODE -ne 0 -or -not ($preview -match '589 seconds') -or -not ($preview -match 'configured: false')) {
    throw 'Offline preview verification failed'
}
$hash = (Get-FileHash -LiteralPath $kitExe -Algorithm SHA256).Hash
$build = [ordered]@{
    source_commit = $source
    exe_sha256 = $hash
    platform = 'Windows x64 MSVC'
    kind = 'software-tested passive field observation'
    hardware_access_performed = $false
    hardware_verified = $false
    control_ready = $false
}
$json = ($build | ConvertTo-Json -Depth 5) + [Environment]::NewLine
[System.IO.File]::WriteAllText((Join-Path $destination 'BUILD.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
foreach ($name in @('BUILD.json','field-test.cmd','field-test.ps1','setup-plan.ps1','README.md','selfrionettectl.exe','test-plan.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $destination $name) -PathType Leaf)) {
        throw "Missing packaged file: $name"
    }
}
Write-Output "Portable field-test kit created: $destination"
Write-Output "Commit: $source"
Write-Output "SHA256: $hash"
Write-Output 'No port opened; no hardware access'
