// Global hotkeys using Win32 RegisterHotKey API

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    VK_DOWN, VK_END, VK_UP,
};

/// Hotkey IDs (must be unique within the application)
pub const HOTKEY_TOGGLE: i32 = 1;
pub const HOTKEY_INCREASE: i32 = 2;
pub const HOTKEY_DECREASE: i32 = 3;

/// Register all global hotkeys. Returns true if all succeed.
pub fn register_all(hwnd: HWND) -> bool {
    let mods = HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
    let mut ok = true;

    unsafe {
        // Ctrl+Alt+End → Toggle dimmer
        if RegisterHotKey(Some(hwnd), HOTKEY_TOGGLE, mods, VK_END.0 as u32).is_err() {
            ok = false;
        }
        // Ctrl+Alt+Up → Increase opacity
        if RegisterHotKey(Some(hwnd), HOTKEY_INCREASE, mods, VK_UP.0 as u32).is_err() {
            ok = false;
        }
        // Ctrl+Alt+Down → Decrease opacity
        if RegisterHotKey(Some(hwnd), HOTKEY_DECREASE, mods, VK_DOWN.0 as u32).is_err() {
            ok = false;
        }
    }

    ok
}

/// Unregister all global hotkeys
pub fn unregister_all(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_TOGGLE);
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_INCREASE);
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_DECREASE);
    }
}
