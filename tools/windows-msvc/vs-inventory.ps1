function ConvertFrom-VswhereInventory {
    param([AllowNull()][AllowEmptyString()][string]$Json,
        [scriptblock]$DirectoryExists = { param($Path) Test-Path -LiteralPath $Path -PathType Container })
    $items = New-Object 'System.Collections.Generic.List[object]'
    if (-not [string]::IsNullOrWhiteSpace($Json)) {
        if ($Json.Trim() -notmatch '^(\[|\{|null$)') { throw 'Malformed vswhere JSON root; installation blocked' }
        # Assignment preserves an empty JSON array without a pipeline wrapper.
        $parsed = ConvertFrom-Json -InputObject $Json -ErrorAction Stop
        foreach ($entry in $parsed) {
            if ($null -eq $entry) { continue }
            if ($entry.GetType().FullName -ne 'System.Management.Automation.PSCustomObject') { throw 'Malformed vswhere record; installation blocked' }
            $id = if ($entry.PSObject.Properties['instanceId']) { $entry.instanceId } else { '' }
            $path = if ($entry.PSObject.Properties['installationPath']) { $entry.installationPath } else { '' }
            $version = if ($entry.PSObject.Properties['installationVersion']) { $entry.installationVersion } else { '' }
            $complete = $entry.PSObject.Properties['isComplete'] -and $entry.isComplete -is [bool] -and $entry.isComplete
            $launchable = $entry.PSObject.Properties['isLaunchable'] -and $entry.isLaunchable -is [bool] -and $entry.isLaunchable
            $valid = $id -is [string] -and -not [string]::IsNullOrWhiteSpace($id) -and
                $path -is [string] -and $path -match '^[a-zA-Z]:[\\/]' -and
                $version -is [string] -and $version -match '^17\.\d+(\.\d+){0,2}$' -and
                $complete -and $launchable
            if ($valid) { $valid = [bool](& $DirectoryExists $path) }
            $items.Add([pscustomobject]@{ Id=$id; Path=$path; Version=$version; Reusable=[bool]$valid })
        }
    }
    $state = 'Absent'
    if ($items.Count -gt 0) {
        $state = 'Existing'
        foreach ($item in $items) { if (-not $item.Reusable) { $state = 'Incomplete' } }
    }
    # One result object; caller never counts ConvertFrom-Json pipeline containers.
    [pscustomobject]@{ State=$state; Count=$items.Count; Instances=$items.ToArray() }
}
