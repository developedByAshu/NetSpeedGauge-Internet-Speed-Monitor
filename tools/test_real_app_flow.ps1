# Drives the real app with real click messages: changes settings, checks they save and show live,
# uses the right-click menu, then puts every setting it changed back.
# Usage: pwsh tools/test_real_app_flow.ps1 -OutputFolder <folder>
param([Parameter(Mandatory)][string]$OutputFolder)

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -Namespace NetSpeedGaugeTools -Name Win -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr a, string c, string t);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@

$WM_LBUTTONUP = 0x0202
$WM_RBUTTONUP = 0x0205
$WM_CLOSE = 0x0010
$WM_APP_OPEN_SETTINGS = 0x8003
$settingsFile = Join-Path $env:APPDATA 'NetSpeedGauge\settings.ini'
$app = Join-Path $PSScriptRoot '..\target\release\NetSpeedGauge.exe'

function Find-Window($class, $title) { [NetSpeedGaugeTools.Win]::FindWindow($class, $title) }
function Click($handle, $x, $y) { [NetSpeedGaugeTools.Win]::PostMessage($handle, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor $x)) | Out-Null; Start-Sleep -Milliseconds 500 }
function Saved($key) { (Select-String -Path $settingsFile -Pattern "^$key=(.*)$").Matches.Groups[1].Value }
function Check($label, $passed) { if ($passed) { "PASS  $label" } else { "FAIL  $label" } }

function Save-Taskbar($name) {
    $bitmap = New-Object Drawing.Bitmap 260, 48
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen(1400, 1032, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $OutputFolder "flow-$name.png"))
    $graphics.Dispose(); $bitmap.Dispose()
}

# Reports whether the readout sits clear of the tray, with the numbers, and fails if it overlaps.
function Check-Clear-Of-Tray($label) {
    $tray = [NetSpeedGaugeTools.Win]::FindWindowEx((Find-Window 'Shell_TrayWnd' $null), [IntPtr]::Zero, 'TrayNotifyWnd', $null)
    $trayArea = New-Object NetSpeedGaugeTools.Win+RECT
    $readoutArea = New-Object NetSpeedGaugeTools.Win+RECT
    [NetSpeedGaugeTools.Win]::GetWindowRect($tray, [ref]$trayArea) | Out-Null
    [NetSpeedGaugeTools.Win]::GetWindowRect($readout, [ref]$readoutArea) | Out-Null
    Check "$label clear of tray (readout right $($readoutArea.R), tray left $($trayArea.L))" ($readoutArea.R -le $trayArea.L)
}

function Save-Window($handle, $name) {
    $r = New-Object NetSpeedGaugeTools.Win+RECT
    [NetSpeedGaugeTools.Win]::GetWindowRect($handle, [ref]$r) | Out-Null
    $bitmap = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    [NetSpeedGaugeTools.Win]::PrintWindow($handle, $dc, 2) | Out-Null
    $graphics.ReleaseHdc($dc)
    $bitmap.Save((Join-Path $OutputFolder "flow-$name.png"))
    $graphics.Dispose(); $bitmap.Dispose()
}

New-Item -ItemType Directory -Force $OutputFolder | Out-Null
$settingsBackup = Join-Path $OutputFolder 'settings-before-test.ini'
Copy-Item $settingsFile $settingsBackup -Force
$readout = Find-Window 'NetSpeedGaugeReadout' 'NetSpeedGauge'
Check 'readout is running' ($readout -ne [IntPtr]::Zero)

[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_APP_OPEN_SETTINGS, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 1
$settings = Find-Window 'NetSpeedGaugeSettings' 'NetSpeedGauge Settings'
Check 'Settings opens' ($settings -ne [IntPtr]::Zero)
Save-Window $settings 'settings-real'

# Icon style: letters, then back to arrows.
Click $settings 90 120
Save-Window $settings 'settings-appearance-real'
Click $settings 563 321
Check 'icon style saves as letters' ((Saved 'arrow_style') -eq '2')
Save-Taskbar 'letters'
Click $settings 505 321
Check 'triangles save' ((Saved 'arrow_style') -eq '1')
Save-Taskbar 'triangles'
Click $settings 467 321
Check 'arrows set' ((Saved 'arrow_style') -eq '0')

# Icon thickness: down to the thinnest, up to the thickest, past both limits, then back.
$thicknessBefore = [int](Saved 'icon_thickness')
1..3 | ForEach-Object { Click $settings 525 375 }
Check 'thickness stops at 1' ((Saved 'icon_thickness') -eq '1')
Save-Taskbar 'thickness-1'
Check-Clear-Of-Tray 'thickness 1'
foreach ($level in 2..4) {
    Click $settings 631 375
    Check "thickness saves as $level" ((Saved 'icon_thickness') -eq "$level")
    Save-Taskbar "thickness-$level"
    Check-Clear-Of-Tray "thickness $level"
}
Click $settings 631 375
Check 'thickness stops at 4' ((Saved 'icon_thickness') -eq '4')
Save-Window $settings 'settings-thickness'
for ($level = 4; $level -gt $thicknessBefore; $level--) { Click $settings 525 375 }
Check 'thickness restored' ((Saved 'icon_thickness') -eq "$thicknessBefore")

# No icons: thickness and the icon colours grey out and ignore clicks, then work again.
Click $settings 617 321
Check 'no icons saves' ((Saved 'arrow_style') -eq '3')
$upColourBefore = Saved 'up_color'
Click $settings 631 375
Check 'thickness ignores clicks with no icons' ((Saved 'icon_thickness') -eq "$thicknessBefore")
Click $settings 636 429
Check 'upload colour ignores clicks with no icons' ((Saved 'up_color') -eq $upColourBefore)
Save-Window $settings 'settings-no-icons'
Save-Taskbar 'no-icons'
Click $settings 467 321
Check 'arrows back' ((Saved 'arrow_style') -eq '0')
Click $settings 636 429
Check 'upload colour works again' ((Saved 'up_color') -ne $upColourBefore)
Click $settings 636 429
Save-Window $settings 'settings-icons-back'

# Download on top: swaps the rows, then back.
$swapBefore = Saved 'swap_rows'
Click $settings 400 537
Check 'download on top changes' ((Saved 'swap_rows') -ne $swapBefore)
Save-Window $settings 'settings-swapped'
Save-Taskbar 'swapped'
Click $settings 400 537
Check 'download on top restored' ((Saved 'swap_rows') -eq $swapBefore)

# Chosen icon colours: pink upload and green download must reach the usage window and hover card.
Click $settings 636 429
Click $settings 536 483
Check 'upload colour saves as pink' ((Saved 'up_color') -eq [string]0xF472B6)
Check 'download colour saves as green' ((Saved 'down_color') -eq [string]0x4ADE80)
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 800
$colourUsage = Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage'
Save-Window $colourUsage 'usage-chosen-colours'
$sheet = New-Object Drawing.Bitmap (Join-Path $OutputFolder 'flow-usage-chosen-colours.png')
$found = @{}
for ($x = 0; $x -lt $sheet.Width; $x += 1) { for ($y = 0; $y -lt 260; $y += 1) { $found[$sheet.GetPixel($x, $y).ToArgb() -band 0xFFFFFF] = 1 } }
$sheet.Dispose()
Check 'usage window shows the pink upload colour' $found.ContainsKey(0xF472B6)
Check 'usage window shows the green download colour' $found.ContainsKey(0x4ADE80)
[NetSpeedGaugeTools.Win]::PostMessage($colourUsage, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 400
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x02A1, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 700
$hoverCard = Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge'
Check 'hover card opens' ($hoverCard -ne [IntPtr]::Zero)
if ($hoverCard -ne [IntPtr]::Zero) { Save-Window $hoverCard 'hover-chosen-colours' }
[NetSpeedGaugeTools.Win]::PostMessage($readout, 0x02A3, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 400

# Theme: light, then back to the system theme.
Click $settings 572 129
Check 'theme saves as light' ((Saved 'window_theme') -eq '1')
Save-Window $settings 'settings-light'
Click $settings 470 129
Check 'system theme restored' ((Saved 'window_theme') -eq '0')

# Background: a blue box behind the numbers, seen during real traffic, then none.
$wheelDown = [IntPtr](([int](-480 -band 0xFFFF)) -shl 16)
$wheelUp = [IntPtr]((480) -shl 16)
[NetSpeedGaugeTools.Win]::PostMessage($settings, 0x020A, $wheelDown, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 300
Click $settings 586 549
Check 'background colour saves' ((Saved 'background_color') -ne '-1')
[NetSpeedGaugeTools.Win]::PostMessage($settings, 0x020A, $wheelUp, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 300
Save-Window $settings 'settings-background'
$download = Start-Process curl.exe -ArgumentList '-s', '-o', 'NUL', '-m', '8', 'https://ash-speed.hetzner.com/100MB.bin' -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 4
Save-Taskbar 'background'
$download.WaitForExit(9000) | Out-Null
[NetSpeedGaugeTools.Win]::PostMessage($settings, 0x020A, $wheelDown, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 300
Click $settings 386 549
Check 'background removed' ((Saved 'background_color') -eq '-1')

# Unit: megabits, then back to megabytes.
Click $settings 90 160
Click $settings 496 225
Check 'unit saves as megabits' ((Saved 'unit') -eq '1')
Save-Taskbar 'megabits'
Click $settings 436 225
Check 'megabytes restored' ((Saved 'unit') -eq '0')

# Another stepper at its limit: decimal places stop at 2 and the plus button goes muted.
1..3 | ForEach-Object { Click $settings 631 333 }
Check 'decimals stop at 2' ((Saved 'decimals') -eq '2')
Save-Window $settings 'settings-decimals-limit'

# Clicking the numbers: off does nothing, on opens the usage window.
Click $settings 90 240
if ((Saved 'click_opens_usage') -ne '0') { Click $settings 400 225 }
Check 'click choice saves as off' ((Saved 'click_opens_usage') -eq '0')
Save-Window $settings 'settings-advanced'
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 700
Check 'click on the numbers does nothing when off' ((Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage') -eq [IntPtr]::Zero)
Click $settings 400 225
Check 'click choice saves as on' ((Saved 'click_opens_usage') -eq '1')
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 700
$clickedUsage = Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage'
Check 'click on the numbers opens Usage when on' ($clickedUsage -ne [IntPtr]::Zero)
[NetSpeedGaugeTools.Win]::PostMessage($clickedUsage, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 500

# About page.
Click $settings 90 280
Save-Window $settings 'settings-about'

[NetSpeedGaugeTools.Win]::PostMessage($settings, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 2
Check 'Settings closes' ((Find-Window 'NetSpeedGaugeSettings' 'NetSpeedGauge Settings') -eq [IntPtr]::Zero)
$process = Get-Process NetSpeedGauge
'RAM after closing Settings: {0:N1} MB' -f ($process.WorkingSet64 / 1MB)

# Right-click menu: Usage row.
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_RBUTTONUP, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 700
$popup = Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge'
Check 'right-click menu opens' ($popup -ne [IntPtr]::Zero)
Click $popup 80 55
Start-Sleep -Milliseconds 700
$usage = Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage'
Check 'menu Usage opens the usage window' ($usage -ne [IntPtr]::Zero)
Check 'menu closes after choosing' ((Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge') -eq [IntPtr]::Zero)
Save-Window $usage 'usage-real'

# Gear icon in the usage window opens Settings.
Click $usage 404 32
Start-Sleep -Milliseconds 700
$settings = Find-Window 'NetSpeedGaugeSettings' 'NetSpeedGauge Settings'
Check 'gear icon opens Settings' ($settings -ne [IntPtr]::Zero)
[NetSpeedGaugeTools.Win]::PostMessage($settings, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
[NetSpeedGaugeTools.Win]::PostMessage($usage, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 1

# Right-click menu: Exit row.
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_RBUTTONUP, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 700
$popup = Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge'
Click $popup 80 125
Start-Sleep -Seconds 2
Check 'menu Exit closes the app' (-not (Get-Process NetSpeedGauge -ErrorAction SilentlyContinue))
Copy-Item $settingsBackup $settingsFile -Force
Start-Process $app
Start-Sleep -Seconds 2
Check 'app starts again' ([bool](Get-Process NetSpeedGauge -ErrorAction SilentlyContinue))
Save-Taskbar 'restored'
