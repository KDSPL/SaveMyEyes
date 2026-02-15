// Screen dimmer using layered overlay windows with WDA_EXCLUDEFROMCAPTURE.
//
// This approach creates transparent, click-through, topmost overlay windows
// on each monitor. The key feature is SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)
// which tells the DWM compositor to exclude these windows from screenshot and
// screen recording capture — so ShareX, OBS, Snipping Tool, etc. will capture
// the screen WITHOUT the dimming effect.
//
// Z-order strategy (flicker-free):
//   • Overlay is created with WS_EX_TOPMOST (enters the topmost z-band)
//   • We do NOT aggressively re-assert topmost via timers or event hooks
//   • We re-assert topmost ONLY on explicit user actions (opacity change, toggle)
//   • This eliminates the z-order ping-pong with Chrome/other topmost windows
//     that was causing flickering

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
    CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING,
    SWP_NOSIZE, SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WNDCLASSW, WS_DISABLED, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
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

/// Minimal window proc. No WM_WINDOWPOSCHANGING override — this was the
/// source of z-order flickering. We let Windows manage position naturally.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
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

/// Gently re-assert topmost on all overlay windows.
/// Called only on explicit user actions (opacity change, toggle), NOT on a timer.
/// Uses SWP_NOSENDCHANGING to avoid notifying other apps.
fn reassert_topmost_once() {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    for wrapper in windows.iter() {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                );
            }
        }
    }
}

/// Callback for EnumDisplayMonitors — creates one overlay per monitor
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
            // Set opacity
            let opacity = *CURRENT_OPACITY.lock().unwrap();
            let alpha = (opacity * 255.0) as u8;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);

            // Set capture exclusion — this is the key feature.
            // WDA_EXCLUDEFROMCAPTURE tells DWM to skip this window when
            // screenshot/recording apps capture the screen.
            let allow_capture = *ALLOW_CAPTURE.lock().unwrap();
            let affinity = if allow_capture {
                WDA_NONE
            } else {
                WDA_EXCLUDEFROMCAPTURE
            };
            let _ = SetWindowDisplayAffinity(hwnd, affinity);

            // Initial topmost positioning
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
            );

            OVERLAY_WINDOWS
                .lock()
                .unwrap()
                .push(HwndWrapper(hwnd.0 as isize));
        }
    }

    windows::core::BOOL::from(true)
}

/// Show overlay with given opacity on all monitors.
/// The overlay is excluded from screen capture by default.
pub fn show_overlay(opacity: f32, allow_capture: bool) {
    *CURRENT_OPACITY.lock().unwrap() = opacity.clamp(0.0, 0.9);
    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;

    // Remove any existing overlays
    hide_overlay();

    if !register_class() {
        return;
    }

    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(0));
    }

    // Lightweight watchdog: only detects externally destroyed windows (e.g.,
    // by third-party tools). Does NOT re-assert z-order — that's intentional
    // to avoid flickering.
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(5));

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
                // Clear and rebuild
                {
                    let mut windows = OVERLAY_WINDOWS.lock().unwrap();
                    for wrapper in windows.drain(..) {
                        unsafe {
                            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
                            let _ = ShowWindow(hwnd, SW_HIDE);
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }
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

/// Update overlay alpha on all windows.
/// Also gently re-asserts topmost (since this is an explicit user action,
/// it's safe to do — the user won't see flicker because they caused it).
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
    drop(windows);

    // Re-assert topmost on user-initiated opacity change
    reassert_topmost_once();
}

/// Check if overlay is visible
pub fn is_visible() -> bool {
    !OVERLAY_WINDOWS.lock().unwrap().is_empty()
}
