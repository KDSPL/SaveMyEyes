// Windows overlay implementation using windows-rs
// Creates a layered, transparent, click-through overlay window on each monitor

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, CS_HREDRAW,
    CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WINDOWPOS, WM_WINDOWPOSCHANGING, WNDCLASSW,
    WS_DISABLED, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

// Thread-safe wrapper for HWND
struct HwndWrapper(isize);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

static OVERLAY_WINDOWS: Mutex<Vec<HwndWrapper>> = Mutex::new(Vec::new());
static CURRENT_OPACITY: Mutex<f32> = Mutex::new(0.3);
static ALLOW_CAPTURE: Mutex<bool> = Mutex::new(false);
static CLASS_REGISTERED: Mutex<bool> = Mutex::new(false);
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);

const CLASS_NAME: &str = "SaveMyEyesOverlay\0";

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_WINDOWPOSCHANGING {
        let pos = &mut *(lparam.0 as *mut WINDOWPOS);
        pos.hwndInsertAfter = HWND_TOPMOST;
        pos.flags = pos.flags & !SWP_NOZORDER;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn register_class() -> bool {
    let mut registered = CLASS_REGISTERED.lock().unwrap();
    if *registered {
        return true;
    }

    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let class_name: Vec<u16> = CLASS_NAME.encode_utf16().collect();

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hbrBackground: CreateSolidBrush(COLORREF(0)),
            ..Default::default()
        };

        let result = RegisterClassW(&wc);
        if result != 0 {
            *registered = true;
            true
        } else {
            false
        }
    }
}

/// Callback for EnumDisplayMonitors to create overlay on each monitor
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprect: *mut RECT,
    _lparam: LPARAM,
) -> windows::core::BOOL {
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    if GetMonitorInfoW(hmonitor, &mut mi).as_bool() {
        let rect = mi.rcMonitor;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let class_name: Vec<u16> = CLASS_NAME.encode_utf16().collect();

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WS_POPUP | WS_VISIBLE | WS_DISABLED,
            rect.left,
            rect.top,
            width,
            height,
            None,
            None,
            Some(hinstance.into()),
            None,
        );

        if let Ok(hwnd) = hwnd {
            let opacity = *CURRENT_OPACITY.lock().unwrap();
            let alpha = (opacity * 255.0) as u8;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);

            let allow_capture = *ALLOW_CAPTURE.lock().unwrap();
            let affinity = if allow_capture {
                WDA_NONE
            } else {
                WDA_EXCLUDEFROMCAPTURE
            };
            let _ = SetWindowDisplayAffinity(hwnd, affinity);

            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

            OVERLAY_WINDOWS
                .lock()
                .unwrap()
                .push(HwndWrapper(hwnd.0 as isize));
        }
    }

    windows::core::BOOL::from(true)
}

/// Show overlay with given opacity on all monitors
pub fn show_overlay(opacity: f32, allow_capture: bool) {
    *CURRENT_OPACITY.lock().unwrap() = opacity.clamp(0.0, 0.9);
    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;

    hide_overlay();

    if !register_class() {
        return;
    }

    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(0));
    }

    // Start watchdog thread to detect externally destroyed overlay windows
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(2));

            let windows = OVERLAY_WINDOWS.lock().unwrap();
            if windows.is_empty() {
                drop(windows);
                WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
                break;
            }

            let mut needs_recreate = false;
            for wrapper in windows.iter() {
                unsafe {
                    let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
                    if !IsWindow(Some(hwnd)).as_bool() {
                        needs_recreate = true;
                        break;
                    }
                }
            }
            drop(windows);

            if needs_recreate {
                let opacity = *CURRENT_OPACITY.lock().unwrap();
                let allow_capture = *ALLOW_CAPTURE.lock().unwrap();
                hide_overlay();
                if register_class() {
                    *CURRENT_OPACITY.lock().unwrap() = opacity;
                    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;
                    unsafe {
                        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(0));
                    }
                }
            }
        });
    }
}

/// Hide overlay windows
pub fn hide_overlay() {
    let mut windows = OVERLAY_WINDOWS.lock().unwrap();
    for wrapper in windows.drain(..) {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Update overlay alpha on all windows
pub fn set_opacity(opacity: f32) {
    let opacity = opacity.clamp(0.0, 0.9);
    *CURRENT_OPACITY.lock().unwrap() = opacity;
    let alpha = (opacity * 255.0) as u8;

    let windows = OVERLAY_WINDOWS.lock().unwrap();
    for wrapper in windows.iter() {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
    }
}

/// Check if overlay is visible
pub fn is_visible() -> bool {
    !OVERLAY_WINDOWS.lock().unwrap().is_empty()
}
