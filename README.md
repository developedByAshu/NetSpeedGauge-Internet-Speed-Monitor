<h1 align="center">NetSpeedGauge (Internet Speed Meter)</h1>

<p align="center">
  <img src="docs/images/banner.png" alt="NetSpeedGauge shows internet upload and download speed on the Windows taskbar, using under 4 MB of RAM" width="860">
</p>

<p align="center">
  #1 lightweight speed monitor for Windows and Mac. See your upload and download speed, and how much data you used today, this week, this month and this year.
</p>

<p align="center">
  <a href="https://github.com/developedByAshu/NetSpeedGauge-Internet-Speed-Monitor/releases/latest"><img src="https://img.shields.io/badge/Download-2563EB?style=for-the-badge" alt="Download"></a>
</p>

<p align="center">
  <img src="https://img.shields.io/github/v/release/developedByAshu/NetSpeedGauge-Internet-Speed-Monitor?label=version" alt="Latest version">
  <img src="https://img.shields.io/badge/Windows-10_%7C_11-2563EB" alt="Windows 10 and 11">
  <img src="https://img.shields.io/badge/Mac-Apple_silicon-2563EB" alt="Mac with Apple silicon">
  <img src="https://img.shields.io/badge/licence-GPL--3.0-16A34A" alt="GPL-3.0 licence">
</p>

<table align="center">
  <tr>
    <td align="center" width="170"><h3>Under 4 MB</h3>RAM on Windows</td>
    <td align="center" width="170"><h3>14 MB</h3>RAM on Mac</td>
    <td align="center" width="170"><h3>408 KB</h3>Windows program, no install</td>
    <td align="center" width="170"><h3>0</h3>internet connections</td>
  </tr>
</table>

## Why I built it

I tried the taskbar speed monitors out there. One missed most of my downloads because it followed a single adapter. Another showed the right speed but used about 79 MB of RAM. Some had no usage history at all.

So this one counts every adapter, keeps 2 years of history, and stays out of the way.

## Screenshots

<table>
  <tr>
    <td width="50%" align="center"><img src="docs/images/settings-appearance.png" alt="Settings for theme, font, colours and icons"><br><sub>Make it look the way you want</sub></td>
    <td width="50%" align="center"><img src="docs/images/settings-details.png" alt="Privacy page saying your data stays on your PC"><br><sub>Your data stays with you</sub></td>
  </tr>
  <tr>
    <td width="50%" align="center"><img src="docs/images/tile-usage.png" alt="Internet data usage by day, week, month and year on Windows"><br><sub>Usage by day, week, month and year (sample data)</sub></td>
    <td width="50%" align="center"><img src="docs/images/tile-taskbar.png" alt="Upload and download speed on the Windows taskbar, and the hover card with today's and this month's data usage"><br><sub>On the taskbar. Click for today and this month</sub></td>
  </tr>
</table>

## What it does

<table>
  <tr>
    <td valign="top" width="33%">
      <b>Speed</b>
      <ul>
        <li>Live upload and download speed</li>
        <li>Wi-Fi, LAN, Wi-Fi Direct and Bluetooth together, or only the adapters you pick</li>
        <li>VPN traffic counts once</li>
        <li>Mbps, MBps, Mibps or MiBps</li>
      </ul>
    </td>
    <td valign="top" width="33%">
      <b>Data usage</b>
      <ul>
        <li>Day, week, month and year charts</li>
        <li>Kept for up to 2 years</li>
        <li>Split by adapter</li>
        <li>Keeps counting across restarts</li>
        <li>Closed the app? It catches up when you reopen it, if the computer has not restarted since</li>
      </ul>
    </td>
    <td valign="top" width="33%">
      <b>Look</b>
      <ul>
        <li>Light, dark or same as your system</li>
        <li>Icons, thickness, colours, font, size and background</li>
        <li>Colours can change with speed</li>
        <li>Your colours follow into Usage and the hover card</li>
        <li>Choose what hover and click open</li>
      </ul>
    </td>
  </tr>
</table>

## Install

### Windows

1. Download `NetSpeedGauge-x64.exe` from [Releases](https://github.com/developedByAshu/NetSpeedGauge-Internet-Speed-Monitor/releases/latest). On a Windows on ARM PC, take `NetSpeedGauge-arm64.exe`.
2. Put it in a folder you will keep, then run it.
3. To open it at sign in, turn on **Start with Windows** in Settings.

To update, choose **Exit**, then replace the old file with the new one. Your settings and usage stay as they are.

Windows may show a SmartScreen warning because the program is not code signed yet. Choose **More info**, then **Run anyway**. `SHA256SUMS.txt` in each release lets you check the file.

To remove it, turn off **Start with Windows**, choose **Exit**, then delete the file and `%APPDATA%\NetSpeedGauge`.

### Mac (Apple silicon only)

1. Download `NetSpeedGauge-mac-arm64.zip` from [Releases](https://github.com/developedByAshu/NetSpeedGauge-Internet-Speed-Monitor/releases/latest) and unzip it.
2. Move `NetSpeedGauge` to Applications, then open it.
3. macOS blocks it the first time because the app is not signed by Apple. Open **System Settings**, **Privacy & Security**, choose **Open Anyway**, then open it again.
4. To open it at login, turn on **Start at login** in Settings.

To update, choose **Quit**, then replace the old app with the new one. Your settings and usage stay as they are.

To remove it, turn off **Start at login**, choose **Quit**, then delete the app and `~/Library/Application Support/NetSpeedGauge`.

## Works on

Tested on Windows 11 and on macOS 27 with Apple silicon. It should also work on Windows 10, Windows on ARM and macOS 12 or later, but I have not tested those. There is no version for Intel Macs. If something breaks, [open an issue](https://github.com/developedByAshu/NetSpeedGauge-Internet-Speed-Monitor/issues).

On Windows it shows on the main taskbar only.

## Privacy

- Nothing leaves your computer. No account, tracking, ads or server.
- No network library in the program. It only reads your adapters' byte counts from the system.
- Usage and settings stay in `%APPDATA%\NetSpeedGauge` on Windows and `~/Library/Application Support/NetSpeedGauge` on Mac. They are not encrypted, they are just never sent anywhere.
- The code is all here, so you can check it yourself or ask an AI to.

## CPU and RAM

<table>
  <tr><th></th><th>Windows</th><th>Mac</th></tr>
  <tr><td>Tested on</td><td>Ryzen 5800H, 16GB</td><td>M4, 16GB</td></tr>
  <tr><td>CPU, average</td><td>0.17% of one core (0.01% of the processor)</td><td>0.37% of one core (0.04% of the processor)</td></tr>
  <tr><td>CPU, highest 15 seconds</td><td>0.41% of one core</td><td>0.86% of one core</td></tr>
  <tr><td>RAM, average</td><td>2.4 MB (working set)</td><td>14 MB (physical footprint)</td></tr>
  <tr><td>RAM with a window open</td><td>4.2 MB with Usage, 7.2 MB with Settings</td><td>Up to 92 MB, about 32 MB after closing</td></tr>
  <tr><td>RAM after closing the window</td><td>1.5 MB</td><td>About 32 MB</td></tr>
</table>

<sub>10 minutes each, plugged in, updating every second with Slow down when idle on, and light traffic (under 10 MB on each). The two RAM tools measure differently, so the RAM figures are not a direct comparison. Window figures are readings, not averages. Heavy traffic has not been measured. Yours may differ.</sub>

## Common questions

<details>
<summary><b>How do I see internet speed on the Windows 11 taskbar?</b></summary>
<br>Run NetSpeedGauge. The speed appears beside the system tray.
</details>

<details>
<summary><b>How do I see internet speed in the Mac menu bar?</b></summary>
<br>Run NetSpeedGauge. Upload and download appear side by side in the menu bar.
</details>

<details>
<summary><b>How do I check how much data I used this month on Windows or Mac?</b></summary>
<br>Right-click the numbers, choose <b>Usage</b>, then pick <b>Month</b>.
</details>

<details>
<summary><b>Does it show Mbps or MBps?</b></summary>
<br>Either. Change it under <b>Settings</b>, <b>Units</b>.
</details>

<details>
<summary><b>Does it slow down my PC or use my data?</b></summary>
<br>No data, because it never connects to the internet. For CPU and RAM, see the numbers above.
</details>

<details>
<summary><b>Why are other adapters missing from the Usage list?</b></summary>
<br>An adapter appears there after data has moved through it. Settings lists all of them.
</details>

## Build it yourself

On Windows you need Rust and the Visual Studio C++ build tools.

```
cargo test --release -p netspeedgauge-shared -p netspeedgauge-windows
cargo build --release -p netspeedgauge-windows --bin NetSpeedGauge
```

The program ends up in `target\release\NetSpeedGauge.exe`.

On a Mac with Apple silicon you need Rust and the Xcode command line tools. This runs the tests and builds `target/mac/NetSpeedGauge.app`:

```
./mac/bundle.sh
```

## Licence

[GPL-3.0](LICENSE)
