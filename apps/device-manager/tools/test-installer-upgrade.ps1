# Ephemeral CI only: exercise the real installer UI against historical registry layouts.
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted'){throw 'Upgrade regression is restricted to a disposable GitHub-hosted runner'}
$app=Split-Path $PSScriptRoot
$build=Get-Content (Join-Path $app 'dist/latest-distribution.json') -Raw | ConvertFrom-Json
$config=Get-Content (Join-Path $app 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$target=Join-Path $env:RUNNER_TEMP 'selfrionette-install-test'
$exe=Join-Path $target 'selfrionette-device-manager.exe'
if($env:SELFRIONETTE_INSTALLED_EXE -ne $exe){throw 'Unexpected test installation'}
$setup=@(Get-ChildItem $build.directory -Filter '*-setup.exe');if($setup.Count -ne 1){throw 'Expected one setup'}
$arp='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Selfrionette Device Manager'
$stable='Software\dev.selfrionette.manager\Installer'
$reportDir=Join-Path $app 'src-tauri/target/upgrade-qa';New-Item $reportDir -ItemType Directory -Force|Out-Null
$cases=[Collections.Generic.List[object]]::new()
$marker=Join-Path ([Environment]::GetFolderPath('ApplicationData')) ('dev.selfrionette.manager\upgrade-test-'+[Guid]::NewGuid().ToString('N')+'.txt')
New-Item (Split-Path $marker) -ItemType Directory -Force|Out-Null
[IO.File]::WriteAllText($marker,'retain-user-record')
$unowned=Join-Path $target 'unowned-test-file.txt';[IO.File]::WriteAllText($unowned,'retain-unowned-file')
function Verify-Target {
  $entry=Get-ItemProperty $arp
  if($entry.InstallLocation.Trim('"') -ne $target){throw 'The test install path changed'}
  if($entry.UninstallString -ne ('"'+(Join-Path $target 'uninstall.exe')+'"')){throw 'The registered uninstaller path changed'}
  if((Get-FileHash $exe).Hash -ne (Get-FileHash $build.executable).Hash){throw 'Installed executable changed unexpectedly'}
  if([IO.File]::ReadAllText($marker) -ne 'retain-user-record' -or [IO.File]::ReadAllText($unowned) -ne 'retain-unowned-file'){throw 'Update deleted user data'}
  return $entry
}
try {
  foreach($baseline in @(@{version='0.1.0-preview.1';publisher='selfrionette'},@{version='0.1.0-preview.2';publisher='Xpotato.net'})){
    $null=Verify-Target
    $legacy='HKCU:\Software\'+$baseline.publisher+'\Selfrionette Device Manager'
    if(Test-Path $legacy){throw 'Historical test key must be absent before fixture creation'}
    New-Item $legacy -Force|Out-Null;Set-Item $legacy -Value $target
    $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($stable,$true)
    try {if($key){$key.DeleteValue('', $false)}}finally{if($key){$key.Dispose()}}
    Set-ItemProperty $arp -Name DisplayVersion -Value $baseline.version
    Set-ItemProperty $arp -Name Publisher -Value $baseline.publisher
    $before=(Get-ItemProperty $arp|Select-Object DisplayVersion,Publisher,InstallLocation,UninstallString)|ConvertTo-Json -Compress
    & (Join-Path $PSScriptRoot 'installer-ui.ps1') -Installer $setup[0].FullName -Report (Join-Path $reportDir ($baseline.version+'-cancel.json')) -CancelBeforeUninstall
    $after=(Get-ItemProperty $arp|Select-Object DisplayVersion,Publisher,InstallLocation,UninstallString)|ConvertTo-Json -Compress
    if($before -ne $after){throw 'Cancelled upgrade modified the registration'}
    $null=Verify-Target
    $log=Join-Path $reportDir ($baseline.version+'-upgrade.json')
    & (Join-Path $PSScriptRoot 'installer-ui.ps1') -Installer $setup[0].FullName -Report $log
    $ui=Get-Content $log -Raw|ConvertFrom-Json
    if(-not $ui.child_uninstaller_observed){throw 'Test did not exercise the pre-install uninstaller'}
    $entry=Verify-Target
    if($entry.Publisher -ne 'Xpotato.net' -or $entry.DisplayVersion -ne $config.version){throw 'Updated product metadata mismatch'}
    $cases.Add(@{baseline_registry_version=$baseline.version;baseline_publisher=$baseline.publisher;result='PASS';real_installer_ui=$true;legacy_registration_fixture=$true;data_preserved=$true;preinstall_uninstaller=$true;cancel_unchanged=$true})
    Remove-Item $legacy # This exact key was created above and belongs to the test.
  }
  $repair=Join-Path $reportDir 'same-version-repair.json'
  & (Join-Path $PSScriptRoot 'installer-ui.ps1') -Installer $setup[0].FullName -Report $repair
  $null=Verify-Target
  $cases.Add(@{case='same-version-repair';result='PASS';data_preserved=$true})
  $report=@{source_commit=$build.commit;result='PASS';cases=$cases;scope='Actual current setup/uninstaller with historical registry fixtures, not archived historical executables'}
  [IO.File]::WriteAllText((Join-Path $build.directory 'UPGRADE-VALIDATION.json'),($report|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
  $sums=Get-ChildItem $build.directory -File|Where-Object Extension -in @('.exe','.zip','.json','.md')|Sort-Object Name|ForEach-Object {(Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()+'  '+$_.Name}
  [IO.File]::WriteAllText((Join-Path $build.directory 'SHA256SUMS.txt'),($sums -join "`n")+"`n",[Text.UTF8Encoding]::new($false))
  Write-Output 'PASS: both historical registry layouts, cancellation, real preinstall uninstall, same-version repair, record preservation'
} finally {
  if((Test-Path $marker) -and [IO.File]::ReadAllText($marker) -eq 'retain-user-record'){Remove-Item $marker}
  if((Test-Path $unowned) -and [IO.File]::ReadAllText($unowned) -eq 'retain-unowned-file'){Remove-Item $unowned}
}
