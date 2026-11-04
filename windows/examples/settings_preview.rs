//! Opens the Settings window without saving anything, for checking its design.
//! Usage: cargo run --example settings_preview [dark|light]

#![windows_subsystem = "windows"]

use netspeedgauge_windows::settings::Settings;
use netspeedgauge_windows::settings_ui;
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowW, GetMessageW, TranslateMessage, MSG,
};

fn main() {
    let theme = std::env::args().nth(1).unwrap_or_else(|| "dark".to_string());
    let class = netspeedgauge_windows::taskbar_dock::to_wide("Shell_TrayWnd");
    let mut settings = Settings::default();
    settings.is_saved_to_disk = false;
    // SAFETY: Win32 setup and message loop on the main thread.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let anchor = FindWindowW(class.as_ptr(), std::ptr::null());
        settings_ui::open(Rc::new(RefCell::new(settings)), anchor, false);
        settings_ui::preview_theme(theme != "light");
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
