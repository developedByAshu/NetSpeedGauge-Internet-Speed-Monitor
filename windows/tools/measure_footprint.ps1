# Measures the running NetSpeedGauge process: RAM range and average CPU over a time window.
# Usage: pwsh windows/tools/measure_footprint.ps1 [-Seconds 600]
param([int]$Seconds = 600)

$sampleEverySeconds = 30
$process = Get-Process NetSpeedGauge -ErrorAction Stop
# The CPU figure is live, so the starting value is copied now rather than read from the process later.
$startCpu = [double]$process.CPU
$startTime = Get-Date
$ramSamples = @()
while (((Get-Date) - $startTime).TotalSeconds -lt $Seconds) {
    Start-Sleep -Seconds $sampleEverySeconds
    $ramSamples += (Get-Process -Id $process.Id -ErrorAction Stop).WorkingSet64 / 1MB
}
$last = Get-Process -Id $process.Id -ErrorAction Stop
$elapsed = ((Get-Date) - $startTime).TotalSeconds
$cpuSeconds = [double]$last.CPU - $startCpu
$cores = [Environment]::ProcessorCount
$ram = $ramSamples | Measure-Object -Minimum -Maximum -Average

'Window: {0:N0} s on {1} logical cores' -f $elapsed, $cores
'CPU: {0:N3} s used, {1:N4}% of one core, {2:N4}% of the whole processor' -f $cpuSeconds, ($cpuSeconds / $elapsed * 100), ($cpuSeconds / $elapsed * 100 / $cores)
'RAM working set: min {0:N1} MB, average {1:N1} MB, max {2:N1} MB, private {3:N1} MB' -f $ram.Minimum, $ram.Average, $ram.Maximum, ($last.PrivateMemorySize64 / 1MB)
'Threads: {0}, handles: {1}' -f $last.Threads.Count, $last.HandleCount
