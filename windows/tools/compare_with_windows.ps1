# Compares NetSpeedGauge's counting logic with Windows' own counters during a real download.
# Usage: pwsh windows/tools/compare_with_windows.ps1 [-Seconds 30] [-Adapter Wi-Fi]
param(
    [int]$Seconds = 30,
    [string]$Adapter = 'Wi-Fi',
    [string]$DownloadUrl = 'https://ash-speed.hetzner.com/100MB.bin'
)

$tool = Join-Path $PSScriptRoot '..\..\target\release\examples\list_adapters.exe'
if (-not (Test-Path $tool)) { throw 'Build first: cargo build --release --example list_adapters' }

$before = Get-NetAdapterStatistics -Name $Adapter
$download = Start-Process curl.exe -ArgumentList '-s', '-o', 'NUL', '-m', $Seconds, $DownloadUrl -PassThru -WindowStyle Hidden
$total = & $tool $Seconds | Select-Object -Last 1
$after = Get-NetAdapterStatistics -Name $Adapter
$download.WaitForExit(5000) | Out-Null

$total -match 'up=(\d+) down=(\d+)' | Out-Null
'NetSpeedGauge  down {0:N1} MB, up {1:N1} MB' -f ([long]$Matches[2] / 1e6), ([long]$Matches[1] / 1e6)
'Windows   down {0:N1} MB, up {1:N1} MB' -f (($after.ReceivedBytes - $before.ReceivedBytes) / 1e6), (($after.SentBytes - $before.SentBytes) / 1e6)
