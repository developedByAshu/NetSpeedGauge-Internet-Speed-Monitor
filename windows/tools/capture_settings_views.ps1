# Opens Settings without saving anything and saves a picture of each page, using real click messages.
# Usage: pwsh windows/tools/capture_settings_views.ps1 -OutputFolder <folder> [-Theme dark|light]
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

$WM_LBUTTONUP = 0x0202
$WM_MOUSEWHEEL = 0x020A
$PAGES = 'general', 'appearance', 'units', 'network', 'advanced', 'about'

function Save-View($handle, $name) {
    Start-Sleep -Milliseconds 350
    $r = New-Object NetSpeedGaugeTools.Win+RECT
    [NetSpeedGaugeTools.Win]::GetWindowRect($handle, [ref]$r) | Out-Null
    $bitmap = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    [NetSpeedGaugeTools.Win]::PrintWindow($handle, $dc, 2) | Out-Null
    $graphics.ReleaseHdc($dc)
    $bitmap.Save((Join-Path $OutputFolder "settings-$Theme-$name.png"))
    $graphics.Dispose()
    $bitmap.Dispose()
}

New-Item -ItemType Directory -Force $OutputFolder | Out-Null
$tool = Join-Path $PSScriptRoot '..\..\target\release\examples\settings_preview.exe'
$process = Start-Process $tool -ArgumentList $Theme -PassThru
try {
    Start-Sleep -Seconds 1
    $handle = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugeSettings', 'NetSpeedGauge Settings')
    if ($handle -eq [IntPtr]::Zero) { throw 'The Settings window did not open.' }
    foreach ($position in 0..($PAGES.Count - 1)) {
        $y = 62 + $position * 40 + 18
        [NetSpeedGaugeTools.Win]::PostMessage($handle, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor 90)) | Out-Null
        Save-View $handle $PAGES[$position]
    }
    [NetSpeedGaugeTools.Win]::PostMessage($handle, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr]((120 -shl 16) -bor 90)) | Out-Null
    Start-Sleep -Milliseconds 200
    $wheelDown = [IntPtr](([int](-360 -band 0xFFFF)) -shl 16)
    [NetSpeedGaugeTools.Win]::PostMessage($handle, $WM_MOUSEWHEEL, $wheelDown, [IntPtr]::Zero) | Out-Null
    Save-View $handle 'appearance-scrolled'
    'Preview process RAM with Settings open: {0:N1} MB' -f ((Get-Process -Id $process.Id).WorkingSet64 / 1MB)
}
finally {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
