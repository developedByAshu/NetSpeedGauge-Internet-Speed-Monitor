//! The user's "Start with Windows" choice, kept in their own Windows startup list.

use crate::taskbar_dock::to_wide;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "NetSpeedGauge";
const MAX_PATH_UNITS: usize = 32_768;

/// Returns the startup command for a program path, quoted so spaces in the path are safe.
pub fn startup_command(program_path: &str) -> String {
    format!("\"{program_path}\"")
}

/// Returns the full path of the running program.
fn program_path() -> Option<String> {
    let mut buffer = vec![0u16; MAX_PATH_UNITS];
    // SAFETY: the buffer length passed matches the buffer.
    let length = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buffer.as_mut_ptr(), buffer.len() as u32) };
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

/// Returns true when NetSpeedGauge is in the user's startup list.
pub fn is_enabled() -> bool {
    let key = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    let mut size = 0u32;
    // SAFETY: the strings are null-terminated, and a null data pointer only asks for the size.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    status == 0
}

/// Adds or removes NetSpeedGauge from the user's startup list. Returns true on success.
pub fn set_enabled(enabled: bool) -> bool {
    let key = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    if !enabled {
        // SAFETY: the strings are null-terminated and outlive the call.
        return unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } == 0;
    }
    let Some(path) = program_path() else {
        return false;
    };
    let command = to_wide(&startup_command(&path));
    let byte_count = (command.len() * std::mem::size_of::<u16>()) as u32;
    // SAFETY: `command` holds `byte_count` bytes of null-terminated text and outlives the call.
    let status = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, command.as_ptr().cast(), byte_count)
    };
    status == 0
}
