# Proves saved usage has no gaps: traffic moved while NetSpeedGauge was closed is still counted exactly.
# Also opens the real usage window and reports RAM with it open and after closing.
# Usage: pwsh tools/test_usage_catch_up.ps1 [-PictureFolder <folder>]
param([string]$PictureFolder = $env:TEMP)

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace NetSpeedGaugeTools -Name Win -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@

$root = Join-Path $PSScriptRoot '..'
$app = Join-Path $root 'target\release\NetSpeedGauge.exe'
$reader = Join-Path $root 'target\release\examples\usage_totals.exe'
$saveWaitSeconds = 65

function Read-Saved {
    $line = & $reader | Select-Object -First 1
    $line -match 'saved_down=(\d+) saved_up=(\d+) counter_down=(\d+) counter_up=(\d+)' | Out-Null
    [pscustomobject]@{ Down = [long]$Matches[1]; Up = [long]$Matches[2]; CounterDown = [long]$Matches[3]; CounterUp = [long]$Matches[4] }
}

function Show-Ram($label) {
    $process = Get-Process NetSpeedGauge
    '{0}: working set {1:N1} MB, private {2:N1} MB' -f $label, ($process.WorkingSet64 / 1MB), ($process.PrivateMemorySize64 / 1MB)
}

Stop-Process -Name NetSpeedGauge -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
Start-Process $app
Start-Sleep -Seconds $saveWaitSeconds
$first = Read-Saved

Stop-Process -Name NetSpeedGauge -Force
& curl.exe -s -o NUL -m 20 'https://ash-speed.hetzner.com/100MB.bin'
Start-Process $app
Start-Sleep -Seconds $saveWaitSeconds
$second = Read-Saved

$counted = $second.Down - $first.Down
$windows = $second.CounterDown - $first.CounterDown
'Counted by NetSpeedGauge: {0:N1} MB down, {1:N1} MB up' -f ($counted / 1e6), (($second.Up - $first.Up) / 1e6)
'Moved per Windows:   {0:N1} MB down, {1:N1} MB up' -f ($windows / 1e6), (($second.CounterUp - $first.CounterUp) / 1e6)
if ($counted -eq $windows -and $counted -gt 90e6) { 'CATCH-UP PASS: exact, with the app closed during the download' } else { 'CATCH-UP FAIL' }
& $reader | Select-Object -Skip 1

Show-Ram 'Window closed'
$readout = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugeReadout', 'NetSpeedGauge')
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x0202, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 2
$usage = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugeUsage', 'NetSpeedGauge Usage')
if ($usage -eq [IntPtr]::Zero) { 'LEFT CLICK FAIL: usage window did not open' } else { 'LEFT CLICK PASS: usage window opened' }
Show-Ram 'Window open'
$r = New-Object NetSpeedGaugeTools.Win+RECT
[NetSpeedGaugeTools.Win]::GetWindowRect($usage, [ref]$r) | Out-Null
$bitmap = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$dc = $graphics.GetHdc()
[NetSpeedGaugeTools.Win]::PrintWindow($usage, $dc, 2) | Out-Null
$graphics.ReleaseHdc($dc)
$bitmap.Save((Join-Path $PictureFolder 'usage-real.png'))
$graphics.Dispose()
$bitmap.Dispose()
[NetSpeedGaugeTools.Win]::PostMessage($usage, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 3
Show-Ram 'Window closed again'
Start-Sleep -Seconds 20
Show-Ram '20 s later'
