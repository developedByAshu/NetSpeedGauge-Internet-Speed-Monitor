//! The local date and the time the Mac started.

use netspeedgauge_shared::calendar::day_from_date;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current time in seconds since 1970.
pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Returns today's day number in the Mac's local time.
pub fn today() -> i32 {
    let now = unix_now() as libc::time_t;
    // SAFETY: an all-zero tm is valid, and localtime_r only fills it in.
    let local = unsafe {
        let mut local: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut local);
        local
    };
    day_from_date(local.tm_year + 1900, (local.tm_mon + 1) as u32, local.tm_mday as u32)
}

/// Returns when the Mac last started, in seconds since 1970.
pub fn boot_time() -> i64 {
    let mut request = [libc::CTL_KERN, libc::KERN_BOOTTIME];
    let mut started = libc::timeval { tv_sec: 0, tv_usec: 0 };
    let mut size = std::mem::size_of::<libc::timeval>();
    // SAFETY: `request`, `started` and `size` are valid for the call, and `size` matches `started`.
    let status = unsafe {
        libc::sysctl(request.as_mut_ptr(), 2, std::ptr::from_mut(&mut started).cast(), &mut size, std::ptr::null_mut(), 0)
    };
    if status == 0 { started.tv_sec } else { 0 }
}
