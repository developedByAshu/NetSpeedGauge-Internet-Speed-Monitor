//! Finds the taskbar and decides where the readout sits on it.

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowRect,
};

// Space kept between the readout and the tray, at 100% scaling.
pub const TRAY_GAP: i32 = 4;

// The desktop background fills the monitor too, but must not count as a fullscreen app.
const DESKTOP_WINDOW_CLASSES: [&str; 2] = ["Progman", "WorkerW"];

/// The taskbar window and the screen areas the readout is placed against.
#[derive(Clone, Copy)]
pub struct TaskbarInfo {
    pub taskbar: HWND,
    pub taskbar_rect: RECT,
    pub tray_rect: RECT,
}

/// Converts text to the null-terminated form Windows expects.
pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Returns a window's screen area, or None if the window has gone.
fn window_rect(window: HWND) -> Option<RECT> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `rect` is a valid out pointer for the duration of the call.
    (unsafe { GetWindowRect(window, &mut rect) } != 0).then_some(rect)
}

/// Finds the primary taskbar and its tray. Returns None while Explorer is restarting.
pub fn find_taskbar() -> Option<TaskbarInfo> {
    let taskbar_class = to_wide("Shell_TrayWnd");
    let tray_class = to_wide("TrayNotifyWnd");
    // SAFETY: both class names are null-terminated and outlive the calls.
    let (taskbar, tray) = unsafe {
        let taskbar = FindWindowW(taskbar_class.as_ptr(), std::ptr::null());
        if taskbar.is_null() {
            return None;
        }
        (taskbar, FindWindowExW(taskbar, std::ptr::null_mut(), tray_class.as_ptr(), std::ptr::null()))
    };
    let taskbar_rect = window_rect(taskbar)?;
    let tray_rect = if tray.is_null() { taskbar_rect } else { window_rect(tray)? };
    Some(TaskbarInfo { taskbar, taskbar_rect, tray_rect })
}

/**
 * Returns the readout's top-left corner: left of the tray on a horizontal taskbar, above the tray
 * on a vertical one (Windows 10 allows those).
 */
pub fn place_readout(taskbar: &RECT, tray: &RECT, width: i32, height: i32, scale: f64) -> (i32, i32) {
    let gap = (f64::from(TRAY_GAP) * scale).round() as i32;
    let taskbar_width = taskbar.right - taskbar.left;
    let taskbar_height = taskbar.bottom - taskbar.top;
    if taskbar_width >= taskbar_height {
        (tray.left - width - gap, taskbar.top + (taskbar_height - height) / 2)
    } else {
        (taskbar.left + (taskbar_width - width) / 2, tray.top - height - gap)
    }
}

/// Converts a screen position to one inside the taskbar, which is how a child window is placed.
pub fn to_taskbar_coords(position: (i32, i32), taskbar: &RECT) -> (i32, i32) {
    (position.0 - taskbar.left, position.1 - taskbar.top)
}

/// Returns true when a collapsed auto-hide taskbar is too thin to hold the readout.
pub fn is_taskbar_collapsed(taskbar: &RECT, readout_width: i32, readout_height: i32) -> bool {
    let width = taskbar.right - taskbar.left;
    let height = taskbar.bottom - taskbar.top;
    if width >= height {
        height < readout_height
    } else {
        width < readout_width
    }
}

/// Returns true when a window fills the whole monitor it is on.
pub fn covers_monitor(window: &RECT, monitor: &RECT) -> bool {
    window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

/**
 * Returns true while the front window fills its monitor, as a fullscreen game or video does.
 * This replaces the Windows shell's own check, which loads about 8 MB of libraries for one answer.
 */
pub fn is_fullscreen_active() -> bool {
    // SAFETY: each pointer passed refers to a local that outlives its call.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return false;
        }
        let mut class_name = [0u16; 32];
        let length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        let class_name = String::from_utf16_lossy(&class_name[..length.max(0) as usize]);
        if DESKTOP_WINDOW_CLASSES.contains(&class_name.as_str()) {
            return false;
        }
        let Some(window_area) = window_rect(window) else {
            return false;
        };
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let handle = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(handle, &mut monitor) != 0 && covers_monitor(&window_area, &monitor.rcMonitor)
    }
}
