// System tray icon with context menu

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
};

/// Custom message ID for tray icon callbacks
pub const WM_TRAY_ICON: u32 = 0x0401; // WM_APP + 1

/// Menu item IDs
pub const IDM_TOGGLE: u32 = 1001;
pub const IDM_SETTINGS: u32 = 1002;
pub const IDM_QUIT: u32 = 1003;

fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Add the system tray icon
pub fn add_tray_icon(hwnd: HWND) -> bool {
    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        // Load icon from embedded resource (ID 1)
        // Use PCWSTR with the integer ID cast to a pointer
        let icon_id = PCWSTR(1 as *const u16);
        let hicon = LoadIconW(Some(hinstance.into()), icon_id);

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY_ICON,
            ..Default::default()
        };

        if let Ok(icon) = hicon {
            nid.hIcon = icon;
        }

        // Set tooltip
        let tip = wide_str("SaveMyEyes");
        let len = tip.len().min(128);
        nid.szTip[..len].copy_from_slice(&tip[..len]);

        Shell_NotifyIconW(NIM_ADD, &nid).as_bool()
    }
}

/// Remove the system tray icon
pub fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// Show the tray context menu
pub fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap();
        let toggle_text = wide_str("Toggle Dimmer");
        let settings_text = wide_str("Settings");
        let quit_text = wide_str("Quit");

        let _ = AppendMenuW(
            menu,
            MF_STRING,
            IDM_TOGGLE as usize,
            PCWSTR(toggle_text.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            IDM_SETTINGS as usize,
            PCWSTR(settings_text.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            IDM_QUIT as usize,
            PCWSTR(quit_text.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        // Required for TrackPopupMenu to work correctly with tray icons
        let _ = SetForegroundWindow(hwnd);

        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );

        let _ = DestroyMenu(menu);
    }
}
