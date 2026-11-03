# Opens the real right-click menu and hover card with real messages and saves a picture of each.
# Usage: pwsh tools/capture_popups.ps1 -OutputFolder <folder>
param([Parameter(Mandatory)][string]$OutputFolder)

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace NetSpeedGaugeTools -Name Win -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@

function Save-Popup($name) {
    Start-Sleep -Milliseconds 500
    $handle = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugePopup', 'NetSpeedGauge')
    if ($handle -eq [IntPtr]::Zero) { "$name did not open"; return }
    $r = New-Object NetSpeedGaugeTools.Win+RECT
    [NetSpeedGaugeTools.Win]::GetWindowRect($handle, [ref]$r) | Out-Null
    $bitmap = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    [NetSpeedGaugeTools.Win]::PrintWindow($handle, $dc, 2) | Out-Null
    $graphics.ReleaseHdc($dc)
    $bitmap.Save((Join-Path $OutputFolder "$name.png"))
    $graphics.Dispose()
    $bitmap.Dispose()
    "$name opened at ({0},{1}) size {2}x{3}" -f $r.L, $r.T, ($r.R - $r.L), ($r.B - $r.T)
    [NetSpeedGaugeTools.Win]::PostMessage($handle, 0x0200, [IntPtr]::Zero, [IntPtr]((110 -shl 16) -bor 80)) | Out-Null
    Start-Sleep -Milliseconds 300
}

New-Item -ItemType Directory -Force $OutputFolder | Out-Null
$readout = [NetSpeedGaugeTools.Win]::FindWindow('NetSpeedGaugeReadout', 'NetSpeedGauge')
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x02A1, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Save-Popup 'popup-hover-card'
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x02A3, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 400
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x0205, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Save-Popup 'popup-menu'
