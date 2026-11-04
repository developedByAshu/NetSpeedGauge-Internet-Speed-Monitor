# Opens the usage window on sample data and saves a picture of each view, using real click messages.
# Usage: pwsh windows/tools/capture_usage_views.ps1 -OutputFolder <folder> [-Theme dark|light]
param(
    [Parameter(Mandatory)][string]$OutputFolder,
    [ValidateSet('dark', 'light')][string]$Theme = 'dark'
)

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace NetSpeedGaugeTools -Name Win -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@

$WM_MOUSEMOVE = 0x0200
$WM_LBUTTONUP = 0x0202

function Save-View($handle, $name) {
    Start-Sleep -Milliseconds 350
    $r = New-Object NetSpeedGaugeTools.Win+RECT
    [NetSpeedGaugeTools.Win]::GetWindowRect($handle, [ref]$r) | Out-Null
    $bitmap = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    [NetSpeedGaugeTools.Win]::PrintWindow($handle, $dc, 2) | Out-Null
    $graphics.ReleaseHdc($dc)
    $bitmap.Save((Join-Path $OutputFolder "$Theme-$name.png"))
    $graphics.Dispose()
    $bitmap.Dispose()
}

function Send-Mouse($handle, $message, $x, $y) {
    [NetSpeedGaugeTools.Win]::PostMessage($handle, $message, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor $x)) | Out-Null
}

New-Item -ItemType Directory -Force $OutputFolder | Out-Null
$tool = Join-Path $PSScriptRoot '..\..\target\release\examples\usage_preview.exe'
$process = Start-Process $tool -ArgumentList $Theme -PassThru
try {
    Start-Sleep -Seconds 1
    $handle = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugeUsage', 'NetSpeedGauge Usage')
    if ($handle -eq [IntPtr]::Zero) { throw 'The usage window did not open.' }

    Save-View $handle 'day'
    Send-Mouse $handle $WM_LBUTTONUP 107 32; Save-View $handle 'week'
    Send-Mouse $handle $WM_LBUTTONUP 165 32; Save-View $handle 'month'
    Send-Mouse $handle $WM_LBUTTONUP 223 32; Save-View $handle 'year'
    Send-Mouse $handle $WM_LBUTTONUP 49 32
    Send-Mouse $handle $WM_LBUTTONUP 200 380; Save-View $handle 'day-selected'
    Send-Mouse $handle $WM_LBUTTONUP 320 32
    Send-Mouse $handle $WM_MOUSEMOVE 300 90; Save-View $handle 'adapter-menu'
    (Get-Process -Id $process.Id).WorkingSet64 / 1MB | ForEach-Object { 'Preview process RAM with window open: {0:N1} MB' -f $_ }
}
finally {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
