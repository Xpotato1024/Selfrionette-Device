$ErrorActionPreference='Stop'
. "$PSScriptRoot\vs-inventory.ps1"
$taskPassed=0
function Assert-Inventory($Name,$Json,$State,$Count) {
    $result=ConvertFrom-VswhereInventory -Json $Json -DirectoryExists {param($p) $p -eq 'D:\fixture\VS'}
    if($result.State -ne $State -or $result.Count -ne $Count -or $result.Instances.Count -ne $Count){throw ('FAIL '+$Name)}
    $script:taskPassed++; Write-Output ('PASS '+$Name)
}
foreach($case in @(@('empty-output',''),@('whitespace','   '),@('empty-array','[]'),@('null','null'),@('null-array','[null]'))) {
    Assert-Inventory $case[0] $case[1] 'Absent' 0
}
$valid=[pscustomobject]@{instanceId='fixture-1';installationPath='D:\fixture\VS';installationVersion='17.14.1.0';isComplete=$true;isLaunchable=$true}
Assert-Inventory 'valid-record' (ConvertTo-Json -InputObject $valid -Compress) 'Existing' 1
Assert-Inventory 'valid-array' (ConvertTo-Json -InputObject @($valid) -Compress) 'Existing' 1
Assert-Inventory 'multiple-records' (ConvertTo-Json -InputObject @($valid,$valid) -Compress) 'Existing' 2
foreach($kind in @('incomplete','not-launchable','missing-path','missing-directory','relative-path','missing-id','invalid-version','string-bool')) {
    $record=ConvertFrom-Json (ConvertTo-Json -InputObject $valid)
    switch($kind) {
        'incomplete' {$record.isComplete=$false}
        'not-launchable' {$record.isLaunchable=$false}
        'missing-path' {$record.installationPath=''}
        'missing-directory' {$record.installationPath='D:\fixture\missing'}
        'relative-path' {$record.installationPath='D:fixture'}
        'missing-id' {$record.instanceId=''}
        'invalid-version' {$record.installationVersion='18.0'}
        'string-bool' {$record.isComplete='true'}
    }
    Assert-Inventory $kind (ConvertTo-Json -InputObject $record -Compress) 'Incomplete' 1
}
foreach($json in @('bad-json','""','[""]','[1]','[[]]')) {
    $rejected=$false
    try {[void](ConvertFrom-VswhereInventory -Json $json)} catch {$rejected=$true}
    if(-not $rejected){throw 'Malformed registry data allowed installation'}
    $taskPassed++
}
Write-Output ('Inventory regression tests PASS: '+$taskPassed)
