//! The shared calendar arithmetic, plus today's date from Windows.

pub use netspeedgauge_shared::calendar::*;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

/// Returns today's day number in the computer's local time.
pub fn today() -> i32 {
    // SAFETY: an all-zero SYSTEMTIME is valid, and GetLocalTime only fills it in.
    let now = unsafe {
        let mut now = std::mem::zeroed();
        GetLocalTime(&mut now);
        now
    };
    day_from_date(i32::from(now.wYear), u32::from(now.wMonth), u32::from(now.wDay))
}
