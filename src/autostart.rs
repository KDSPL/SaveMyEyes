// Registry-based autostart for Windows
// Uses HKCU\Software\Microsoft\Windows\CurrentVersion\Run

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "SaveMyEyes";

fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_run_key(access: u32) -> Option<HKEY> {
    let key_path = wide_string(RUN_KEY);
    let mut hkey = HKEY::default();
    unsafe {
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            Some(0),
            windows::Win32::System::Registry::REG_SAM_FLAGS(access),
            &mut hkey,
        );
        if result.is_ok() {
            Some(hkey)
        } else {
            None
        }
    }
}

/// Enable autostart by setting registry value to current executable path
pub fn enable() -> bool {
    if let Some(hkey) = open_run_key(KEY_WRITE.0) {
        let exe_path = std::env::current_exe().unwrap_or_default();
        let exe_str = format!("\"{}\"", exe_path.display());
        let value_name = wide_string(VALUE_NAME);
        let data = wide_string(&exe_str);
        let data_bytes =
            unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };

        unsafe {
            let result = RegSetValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                Some(0),
                REG_SZ,
                Some(data_bytes),
            );
            let _ = RegCloseKey(hkey);
            result.is_ok()
        }
    } else {
        false
    }
}

/// Disable autostart by removing the registry value
pub fn disable() -> bool {
    if let Some(hkey) = open_run_key(KEY_WRITE.0) {
        let value_name = wide_string(VALUE_NAME);
        unsafe {
            let result = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
            let _ = RegCloseKey(hkey);
            result.is_ok()
        }
    } else {
        false
    }
}

/// Check if autostart is currently enabled
pub fn is_enabled() -> bool {
    if let Some(hkey) = open_run_key(KEY_READ.0) {
        let value_name = wide_string(VALUE_NAME);
        unsafe {
            let result =
                RegQueryValueExW(hkey, PCWSTR(value_name.as_ptr()), None, None, None, None);
            let _ = RegCloseKey(hkey);
            result.is_ok()
        }
    } else {
        false
    }
}
