#Requires -RunAsAdministrator
# Supported VHD compaction; no distribution or user data is deleted.
$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$expectedDisk = Join-Path $workspaceRoot 'local-data/linux-build/ext4.vhdx'
$disk = (Resolve-Path -LiteralPath $expectedDisk).Path
if ($disk -ne [IO.Path]::GetFullPath($expectedDisk)) { throw 'Unexpected virtual disk path.' }
$distribution = @(Get-ChildItem 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Lxss' | Where-Object { $_.GetValue('DistributionName') -eq 'Ochat-Linux-Build' })
if ($distribution.Count -ne 1) { throw 'Expected the dedicated Ochat build distribution.' }
$registeredDisk = Join-Path $distribution[0].GetValue('BasePath') 'ext4.vhdx'
if ((Resolve-Path -LiteralPath $registeredDisk).Path -ne $disk) { throw 'The registered build disk does not match this workspace.' }
$before = (Get-Item -LiteralPath $disk).Length
wsl.exe -d Ochat-Linux-Build --exec sh -c 'if pgrep -x cargo >/dev/null || pgrep -x rustc >/dev/null || pgrep -x ochat >/dev/null; then exit 75; fi'
if ($LASTEXITCODE -ne 0) { throw 'The build distribution is busy. Finish the build or close Ochat before compacting it.' }
wsl.exe -d Ochat-Linux-Build --exec fstrim -v /
if ($LASTEXITCODE -ne 0) { throw 'Linux trim did not complete.' }
wsl.exe --terminate Ochat-Linux-Build
if ($LASTEXITCODE -ne 0) { throw 'The build distribution did not stop.' }
Optimize-VHD -Path $disk -Mode Full -ErrorAction Stop
$after = (Get-Item -LiteralPath $disk).Length
[pscustomobject]@{Disk=$disk;BeforeBytes=$before;AfterBytes=$after;ReclaimedBytes=$before-$after} | ConvertTo-Json
wsl.exe -d Ochat-Linux-Build --exec sh -c 'test -x /usr/local/cargo/bin/cargo && df -h /'
if ($LASTEXITCODE -ne 0) { throw 'Build distribution verification failed.' }
