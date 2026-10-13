param([switch]$PreflightOnly)
$ErrorActionPreference='Stop'
$taskInstaller='D:\Xpotato-apps\codex_room\device-calibration-prep\vs_buildtools.exe'
$taskInstallDir='D:\Xpotato-apps\toolchains\vs2022-msvc'
$taskVSWhere='C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
. "$PSScriptRoot\vs-inventory.ps1"
$taskRaw=(& $taskVSWhere -all -products '*' -version '[17.0,18.0)' -format json | Out-String)
if($LASTEXITCODE -ne 0){throw 'vswhere failed; installation blocked'}
$taskInventory=ConvertFrom-VswhereInventory -Json $taskRaw
if($taskInventory.State -eq 'Incomplete'){throw ('Incomplete VS registration; preserve and inspect existing instance, no new installation started: '+(($taskInventory.Instances | ForEach-Object {$_.Path}) -join ', '))}
if($taskInventory.State -eq 'Existing'){
    if($taskInventory.Count -ne 1){throw 'Multiple VS instances registered; explicit selection required, no installation started'}
    $taskRegisteredPath=$taskInventory.Instances[0].Path
    $taskReady=$null
    foreach($taskDir in (Get-ChildItem -LiteralPath (Join-Path $taskRegisteredPath 'VC\Tools\MSVC') -Directory -ErrorAction SilentlyContinue)){
        $taskVersion=$null
        if(-not [version]::TryParse($taskDir.Name,[ref]$taskVersion) -or $taskVersion.Major -ne 14 -or $taskVersion.Minor -lt 30){continue}
        $taskMissing=@('bin\Hostx64\x64\cl.exe','bin\Hostx64\x64\link.exe','bin\Hostx64\x86\cl.exe','bin\Hostx64\x86\link.exe','lib\x64\libcmt.lib','include\vcruntime.h') | Where-Object {-not (Test-Path -LiteralPath (Join-Path $taskDir.FullName $_) -PathType Leaf)}
        if($null -eq $taskMissing){$taskReady=$taskDir.FullName}
    }
    if(-not $taskReady){throw ('Registered VS lacks usable v143 tools; reuse this installation path, no new product started: '+$taskRegisteredPath)}
    Write-Output ('Reuse existing v143 toolchain: '+$taskReady)
    return
}
if(Test-Path -LiteralPath $taskInstallDir){throw 'Unregistered destination already exists; preserve and inspect it, no installation started'}
if(Get-Process -Name vs_buildtools,vs_installer,setup -ErrorAction SilentlyContinue){throw 'Another installer is active; no duplicate installation started'}
$signature=Get-AuthenticodeSignature -LiteralPath $taskInstaller
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=Microsoft Corporation'){throw 'Official Microsoft signature verification failed'}
if((Get-Item -LiteralPath $taskInstaller).VersionInfo.ProductVersion -ne 'Visual Studio 2022'){throw 'Unexpected product version'}
$taskArgs=@('--quiet','--wait','--norestart','--nocache','--installPath',$taskInstallDir,'--add','Microsoft.VisualStudio.Component.VC.Tools.x86.x64')
if($PreflightOnly){[pscustomobject]@{Inventory=$taskInventory.State;InstanceCount=$taskInventory.Count;Signature=$signature.Status;InstallPath=$taskInstallDir;Arguments=($taskArgs -join ' ');InstallationStarted=$false} | Format-List; return}
$taskAdmin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if($taskAdmin){$taskProcess=Start-Process -FilePath $taskInstaller -ArgumentList $taskArgs -WindowStyle Hidden -PassThru -Wait}
else{$taskProcess=Start-Process -FilePath $taskInstaller -ArgumentList $taskArgs -Verb RunAs -WindowStyle Hidden -PassThru -Wait}
Write-Output ('Build Tools installer exit code: '+$taskProcess.ExitCode)
if($taskProcess.ExitCode -notin @(0,3010)){throw ('Installer failed: '+$taskProcess.ExitCode)}
if($taskProcess.ExitCode -eq 3010){Write-Output 'Reboot requested; no reboot performed.'}
