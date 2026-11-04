# Drives the real app with real click messages: changes settings, checks they save and show live,
# uses the right-click menu, then puts every setting it changed back.
# Usage: pwsh windows/tools/test_real_app_flow.ps1 -OutputFolder <folder>
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
$app = Join-Path $PSScriptRoot '..\..\target\release\NetSpeedGauge.exe'

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

# Start from the default hover and click choices, whatever the user has saved. Their file comes back at the end.
Click $settings 90 240
Click $settings 487 225
Click $settings 589 279

# Start the Appearance page from its defaults, so the positions below hold. Their file comes back at the end.
Click $settings 616 428
Check 'reset opens the icons section' ((Saved 'show_icons') -eq '1')
Check 'reset collapses the background section' ((Saved 'show_background') -eq '0')
Check 'reset collapses the colour by speed section' ((Saved 'color_coding') -eq '0')

function Scroll-Settings($wheel) { [NetSpeedGaugeTools.Win]::PostMessage($settings, 0x020A, $wheel, [IntPtr]::Zero) | Out-Null; Start-Sleep -Milliseconds 300 }
$wheelDown = [IntPtr](([int](-480 -band 0xFFFF)) -shl 16)
$wheelUp = [IntPtr]((480) -shl 16)

# Text: download on top swaps the rows, then back. It stays available whatever the sections below do.
Click $settings 90 120
Save-Window $settings 'settings-appearance-real'
$swapBefore = Saved 'swap_rows'
Click $settings 400 483
Check 'download on top changes' ((Saved 'swap_rows') -ne $swapBefore)
Save-Window $settings 'settings-swapped'
Save-Taskbar 'swapped'
Click $settings 400 483
Check 'download on top restored' ((Saved 'swap_rows') -eq $swapBefore)

# These positions are with the page scrolled to its end and the two lower sections collapsed.
Scroll-Settings $wheelDown
$iconStyleRow = 344
$thicknessRow = 398
$uploadRow = 452
$downloadRow = 506

# Icon style: letters, triangles, then back to arrows.
Click $settings 622 $iconStyleRow
Check 'icon style saves as letters' ((Saved 'arrow_style') -eq '2')
Save-Taskbar 'letters'
Click $settings 572 $iconStyleRow
Check 'triangles save' ((Saved 'arrow_style') -eq '1')
Save-Taskbar 'triangles'
Click $settings 525 $iconStyleRow
Check 'arrows set' ((Saved 'arrow_style') -eq '0')

# Icon thickness: down to the thinnest, up to the thickest, past both limits, then back.
$thicknessBefore = [int](Saved 'icon_thickness')
1..3 | ForEach-Object { Click $settings 525 $thicknessRow }
Check 'thickness stops at 1' ((Saved 'icon_thickness') -eq '1')
Save-Taskbar 'thickness-1'
Check-Clear-Of-Tray 'thickness 1'
foreach ($level in 2..4) {
    Click $settings 631 $thicknessRow
    Check "thickness saves as $level" ((Saved 'icon_thickness') -eq "$level")
    Save-Taskbar "thickness-$level"
    Check-Clear-Of-Tray "thickness $level"
}
Click $settings 631 $thicknessRow
Check 'thickness stops at 4' ((Saved 'icon_thickness') -eq '4')
Save-Window $settings 'settings-thickness'
for ($level = 4; $level -gt $thicknessBefore; $level--) { Click $settings 525 $thicknessRow }
Check 'thickness restored' ((Saved 'icon_thickness') -eq "$thicknessBefore")

# Icons heading: off collapses the rows and removes the icons, on brings the same choices back.
Click $settings 622 $iconStyleRow
Click $settings 400 301
Check 'icons toggle saves as off' ((Saved 'show_icons') -eq '0')
Check 'icons off keeps the chosen style' ((Saved 'arrow_style') -eq '2')
Save-Window $settings 'settings-no-icons'
Save-Taskbar 'no-icons'
Check-Clear-Of-Tray 'no icons'
# The shorter page scrolls back, so the heading has moved.
Click $settings 400 514
Check 'icons toggle saves as on' ((Saved 'show_icons') -eq '1')
Save-Window $settings 'settings-icons-back'
Scroll-Settings $wheelDown
Click $settings 525 $iconStyleRow
Check 'arrows back' ((Saved 'arrow_style') -eq '0')

# Chosen icon colours: pink upload and green download must reach the usage window and hover card.
Click $settings 636 $uploadRow
Click $settings 536 $downloadRow
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
Scroll-Settings $wheelUp
Click $settings 572 129
Check 'theme saves as light' ((Saved 'window_theme') -eq '1')
Save-Window $settings 'settings-light'
Click $settings 470 129
Check 'system theme restored' ((Saved 'window_theme') -eq '0')

# Background heading: on shows its rows and a box behind the numbers during real traffic, off keeps the colour.
Scroll-Settings $wheelDown
Click $settings 400 559
Check 'background toggle saves as on' ((Saved 'show_background') -eq '1')
Save-Window $settings 'settings-background-open'
Click $settings 586 603
Check 'background colour saves' ((Saved 'background_color') -eq [string]0x60A5FA)
Scroll-Settings $wheelUp
Save-Window $settings 'settings-background'
$download = Start-Process curl.exe -ArgumentList '-s', '-o', 'NUL', '-m', '8', 'https://ash-speed.hetzner.com/100MB.bin' -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 4
Save-Taskbar 'background'
$download.WaitForExit(9000) | Out-Null
Scroll-Settings $wheelDown
Click $settings 400 505
Check 'background toggle saves as off' ((Saved 'show_background') -eq '0')
Check 'background off keeps the colour' ((Saved 'background_color') -eq [string]0x60A5FA)

# Colour by speed heading: on shows its rows, off collapses them.
Click $settings 400 601
Check 'colour by speed toggle saves as on' ((Saved 'color_coding') -eq '1')
Save-Window $settings 'settings-speed-colours-open'
Click $settings 400 601
Check 'colour by speed toggle saves as off' ((Saved 'color_coding') -eq '0')

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

# Clicking and hovering the numbers: each can do nothing, show the usage card or open the usage window.
function Post-To-Readout($message) { [NetSpeedGaugeTools.Win]::PostMessage($readout, $message, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null; Start-Sleep -Milliseconds 700 }
function Usage-Is-Open { (Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage') -ne [IntPtr]::Zero }
function Card-Is-Open { (Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge') -ne [IntPtr]::Zero }
function Close-Usage { [NetSpeedGaugeTools.Win]::PostMessage((Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage'), $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null; Start-Sleep -Milliseconds 500 }
function Post-To-Window($handle, $message, $wparam) { [NetSpeedGaugeTools.Win]::PostMessage($handle, $message, [IntPtr]$wparam, [IntPtr]::Zero) | Out-Null; Start-Sleep -Milliseconds 700 }
$WM_MOUSEHOVER = 0x02A1
$WM_MOUSELEAVE = 0x02A3
$WM_ACTIVATE = 0x0006
$hoverRow = 225
$clickRow = 279
$nothingX = 404
$cardX = 487
$windowX = 589

Click $settings 90 240
Click $settings $nothingX $clickRow
Check 'click choice saves as nothing' ((Saved 'click_action') -eq '0')
Save-Window $settings 'settings-advanced'
Post-To-Readout $WM_LBUTTONUP
Check 'click does nothing when set to nothing' (-not (Usage-Is-Open) -and -not (Card-Is-Open))

Click $settings $cardX $clickRow
Check 'click choice saves as card' ((Saved 'click_action') -eq '1')
Post-To-Readout $WM_LBUTTONUP
Check 'click shows the card' ((Card-Is-Open) -and -not (Usage-Is-Open))
Post-To-Readout $WM_MOUSELEAVE
Check 'moving away leaves a clicked card open' (Card-Is-Open)
Post-To-Readout $WM_LBUTTONUP
Check 'a second click closes the card' (-not (Card-Is-Open))
Post-To-Readout $WM_LBUTTONUP
Post-To-Window (Find-Window 'NetSpeedGaugePopup' 'NetSpeedGauge') $WM_ACTIVATE 0
Check 'clicking elsewhere closes a clicked card' (-not (Card-Is-Open))

Click $settings $windowX $clickRow
Check 'click choice saves as window' ((Saved 'click_action') -eq '2')
Post-To-Readout $WM_LBUTTONUP
Check 'click opens the usage window' (Usage-Is-Open)
Post-To-Readout $WM_MOUSELEAVE
Check 'moving away leaves a clicked window open' (Usage-Is-Open)
Post-To-Window (Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage') $WM_ACTIVATE (1 -shl 16)
Check 'minimizing does not count as clicking elsewhere' (Usage-Is-Open)
Post-To-Readout $WM_LBUTTONUP
Check 'a second click closes the window' (-not (Usage-Is-Open))
Post-To-Readout $WM_LBUTTONUP
Post-To-Window (Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage') $WM_ACTIVATE 0
Check 'clicking elsewhere closes a clicked window' (-not (Usage-Is-Open))

Click $settings $nothingX $hoverRow
Check 'hover choice saves as nothing' ((Saved 'hover_action') -eq '0')
Post-To-Readout $WM_MOUSEHOVER
Check 'hover does nothing when set to nothing' (-not (Usage-Is-Open) -and -not (Card-Is-Open))

Click $settings $windowX $hoverRow
Check 'hover choice saves as window' ((Saved 'hover_action') -eq '2')
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_MOUSEHOVER, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 100
Check 'hover opens the usage window' (Usage-Is-Open)
# This real mouse is not on the numbers or the window, so the window must close by itself. Skipped if it is.
$usageArea = New-Object NetSpeedGaugeTools.Win+RECT
[NetSpeedGaugeTools.Win]::GetWindowRect((Find-Window 'NetSpeedGaugeUsage' 'NetSpeedGauge Usage'), [ref]$usageArea) | Out-Null
$mouse = [Windows.Forms.Cursor]::Position
if ($mouse.X -ge $usageArea.L -and $mouse.X -lt $usageArea.R -and $mouse.Y -ge $usageArea.T -and $mouse.Y -lt 1080) {
    'SKIP  moving away closes a hovered window (the mouse is over it)'
    Close-Usage
} else {
    Start-Sleep -Milliseconds 900
    Check 'moving away closes a hovered window' (-not (Usage-Is-Open))
}
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_MOUSEHOVER, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 100
[NetSpeedGaugeTools.Win]::PostMessage($readout, $WM_LBUTTONUP, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 900
Check 'a click pins a hovered window' (Usage-Is-Open)
Post-To-Readout $WM_MOUSEHOVER
Check 'hovering leaves a pinned window alone' (Usage-Is-Open)
Close-Usage

Click $settings $cardX $hoverRow
Check 'hover choice saves as card' ((Saved 'hover_action') -eq '1')
Post-To-Readout $WM_MOUSEHOVER
Check 'hover shows the card' (Card-Is-Open)
Post-To-Readout $WM_MOUSELEAVE
Check 'moving away closes a hover card' (-not (Card-Is-Open))
Check 'old on or off names are gone from the file' (-not (Select-String -Path $settingsFile -Pattern 'click_opens_usage|hover_card' -Quiet))

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
