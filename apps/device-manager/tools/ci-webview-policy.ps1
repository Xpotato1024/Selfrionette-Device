param([ValidateSet('install','remove')][string]$Mode)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:SELFRIONETTE_CI_WEBVIEW_POLICY -ne '1') { throw 'Only the ephemeral GitHub-hosted test runner may use this policy' }
$root = 'HKLM:\SOFTWARE\Policies\Microsoft\Edge\WebView2'
$name = 'selfrionette-device-manager.exe'
$subkeys = @('AdditionalBrowserArguments', 'UserDataFolder')
if ($Mode -eq 'remove') {
    foreach ($sub in $subkeys) {
        $key = Join-Path $root $sub
        if (Test-Path $key) { Remove-ItemProperty -Path $key -Name $name -ErrorAction SilentlyContinue }
    }
    exit 0
}
foreach ($sub in $subkeys) {
    $key = Join-Path $root $sub
    if ((Test-Path $key) -and ((Get-Item $key).GetValueNames() -contains $name)) { throw 'Refusing to overwrite an existing application policy' }
}
$argsKey = Join-Path $root 'AdditionalBrowserArguments'
$dataKey = Join-Path $root 'UserDataFolder'
New-Item -Path $argsKey -Force | Out-Null
New-Item -Path $dataKey -Force | Out-Null
New-ItemProperty -Path $argsKey -Name $name -PropertyType String -Value $env:SRN_POLICY_ARGUMENTS | Out-Null
New-ItemProperty -Path $dataKey -Name $name -PropertyType String -Value $env:SRN_POLICY_PROFILE | Out-Null
