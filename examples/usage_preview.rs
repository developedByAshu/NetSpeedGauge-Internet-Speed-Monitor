//! Opens the usage window on sample data, for checking its design. Real usage is never touched.
//! Usage: cargo run --example usage_preview [dark|light]

#![windows_subsystem = "windows"]

use netspeedgauge::calendar::today;
use netspeedgauge::usage_store::UsageStore;
use netspeedgauge::usage_ui;
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowW, GetMessageW, TranslateMessage, MSG,
};

const WIFI: u64 = 1;
const LAN: u64 = 2;
const DAYS_OF_SAMPLE_DATA: i32 = 500;

/// Builds believable usage: heavier weekends, occasional large downloads and quiet days.
fn sample_store() -> UsageStore {
    let mut store = UsageStore::default();
    store.add_adapter(WIFI, "Wi-Fi".to_string());
    store.add_adapter(LAN, "Ethernet".to_string());
    let last_day = today();
    let mut seed = 12_345u64;
    let mut next = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as f64 / f64::from(u32::MAX >> 1)
    };
    for offset in (0..DAYS_OF_SAMPLE_DATA).rev() {
        let day = last_day - offset;
        let weekend = if (day + 3).rem_euclid(7) >= 5 { 1.8 } else { 1.0 };
        let burst = if next() > 0.93 { 4.0 } else { 1.0 };
        let quiet = if next() < 0.06 { 0.0 } else { 1.0 };
        let down = (next() * 3.0e9 + 2.0e8) * weekend * burst * quiet;
        let up = down * (next() * 0.18 + 0.03);
        store.record(WIFI, day, up as u64, down as u64);
        if next() > 0.8 {
            store.record(LAN, day, (up * 0.4) as u64, (down * 0.6) as u64);
        }
    }
    store
}

fn main() {
    let theme = std::env::args().nth(1).unwrap_or_else(|| "dark".to_string());
    let class = netspeedgauge::taskbar_dock::to_wide("Shell_TrayWnd");
    // SAFETY: Win32 setup and message loop on the main thread.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let anchor = FindWindowW(class.as_ptr(), std::ptr::null());
        usage_ui::open(Rc::new(RefCell::new(sample_store())), anchor, false, usage_ui::OpenedBy::Menu);
        usage_ui::preview_theme(theme != "light");
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
