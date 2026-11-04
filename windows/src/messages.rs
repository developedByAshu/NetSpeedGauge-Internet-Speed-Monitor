//! Messages other windows send to the readout window.

use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

pub const WM_APP_OPEN_USAGE: u32 = WM_APP + 2;
pub const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 3;
pub const WM_APP_EXIT: u32 = WM_APP + 4;
pub const WM_APP_SETTINGS_CHANGED: u32 = WM_APP + 5;
pub const WM_APP_POPUP_CHANGED: u32 = WM_APP + 6;
