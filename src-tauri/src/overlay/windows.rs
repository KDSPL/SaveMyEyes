// Windows overlay implementation using windows-rs
// Creates a layered, transparent, click-through overlay window

#![cfg(target_os = "windows")]

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
    CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, SW_HIDE, SW_SHOWNA,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_WINDOWPOSCHANGING, WINDOWPOS,
    WS_DISABLED, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE, WNDCLASSW,
};
use std::sync::atomic::{AtomicBool, Ordering};

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
    // When Windows tries to change our Z-order (e.g. another app activates),
    // force the overlay to stay TOPMOST. This replaces the aggressive polling
    // approach and doesn't interfere with screenshot tools.
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
            hbrBackground: CreateSolidBrush(COLORREF(0)), // Black brush
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

        // Create layered, transparent, click-through overlay window
        // WS_DISABLED ensures the window never receives keyboard/mouse focus
        // WS_EX_TRANSPARENT makes it click-through for mouse
        // WS_EX_NOACTIVATE prevents activation
        // WS_EX_TOOLWINDOW hides from taskbar/alt-tab
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

            // Set display affinity (exclude from screen capture if configured)
            // WDA_EXCLUDEFROMCAPTURE hides the window from screen capture APIs
            // This allows PrintScreen and capture apps to work without showing the overlay
            let allow_capture = *ALLOW_CAPTURE.lock().unwrap();
            let affinity = if allow_capture { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE };
            match SetWindowDisplayAffinity(hwnd, affinity) {
                Ok(_) => println!("[Windows] SetWindowDisplayAffinity set to {:?}", if allow_capture { "WDA_NONE" } else { "WDA_EXCLUDEFROMCAPTURE" }),
                Err(e) => println!("[Windows] SetWindowDisplayAffinity failed: {:?}", e),
            }

            // Make sure it's topmost
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

            // Store handle
            OVERLAY_WINDOWS.lock().unwrap().push(HwndWrapper(hwnd.0 as isize));
        }
    }

    windows::core::BOOL::from(true)
}

/// Show overlay with given opacity on all monitors
pub fn show_overlay(opacity: f32, allow_capture: bool) {
    // Update settings
    *CURRENT_OPACITY.lock().unwrap() = opacity.clamp(0.0, 0.9);
    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;
    
    println!("[Windows] show_overlay: opacity={}, allow_capture={}", opacity, allow_capture);

    // Hide existing overlays first
    hide_overlay();

    // Register window class if needed
    if !register_class() {
        println!("[Windows] Failed to register window class");
        return;
    }

    // Create overlay on each monitor
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(0),
        );
    }

    println!(
        "[Windows] show_overlay called with opacity: {}, created {} windows",
        opacity,
        OVERLAY_WINDOWS.lock().unwrap().len()
    );

    // Start a watchdog thread to detect externally destroyed overlay windows.
    // Topmost enforcement is handled by WM_WINDOWPOSCHANGING in window_proc,
    // so this thread only checks for destroyed windows and recreates them.
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            println!("[Windows] Overlay watchdog thread started");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));

                let windows = OVERLAY_WINDOWS.lock().unwrap();
                if windows.is_empty() {
                    drop(windows);
                    WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
                    println!("[Windows] Overlay watchdog thread stopped (no windows)");
                    break;
                }

                // Only check if windows still exist — no SetWindowPos calls
                let mut needs_recreate = false;
                for wrapper in windows.iter() {
                    unsafe {
                        let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
                        if !IsWindow(Some(hwnd)).as_bool() {
                            println!("[Windows] Watchdog: overlay window destroyed externally, will recreate");
                            needs_recreate = true;
                            break;
                        }
                    }
                }
                drop(windows);

                if needs_recreate {
                    // Recreate all overlay windows with current settings
                    let opacity = *CURRENT_OPACITY.lock().unwrap();
                    let allow_capture = *ALLOW_CAPTURE.lock().unwrap();
                    println!("[Windows] Watchdog: recreating overlays (opacity={}, allow_capture={})", opacity, allow_capture);
                    // hide_overlay clears the vec; show_overlay re-populates it
                    hide_overlay();
                    // Re-register and recreate
                    if register_class() {
                        *CURRENT_OPACITY.lock().unwrap() = opacity;
                        *ALLOW_CAPTURE.lock().unwrap() = allow_capture;
                        unsafe {
                            let _ = EnumDisplayMonitors(
                                None,
                                None,
                                Some(monitor_enum_proc),
                                LPARAM(0),
                            );
                        }
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
    println!("[Windows] hide_overlay called");
}

/// Temporarily hide overlay windows without destroying them (for screenshot)
/// Returns true if windows were hidden
pub fn hide_overlay_temp() -> bool {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    if windows.is_empty() {
        println!("[Windows] hide_overlay_temp: no windows to hide");
        return false;
    }
    for wrapper in windows.iter() {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    println!("[Windows] hide_overlay_temp: hidden {} windows", windows.len());
    true
}

/// Restore temporarily hidden overlay windows
pub fn restore_overlay_temp() {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    for wrapper in windows.iter() {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_SHOWNA); // Show without activating
            // Re-apply topmost to ensure it's on top
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    println!("[Windows] restore_overlay_temp: restored {} windows", windows.len());
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
    println!("[Windows] set_opacity called with: {}", opacity);
}

/// Get current overlay opacity
pub fn get_opacity() -> f32 {
    *CURRENT_OPACITY.lock().unwrap()
}

/// Check if overlay is visible
pub fn is_visible() -> bool {
    !OVERLAY_WINDOWS.lock().unwrap().is_empty()
}

// NOTE: Topmost monitor removed - it was interfering with ShareX's UI
// The overlay is already TOPMOST when created, and WDA_EXCLUDEFROMCAPTURE
// ensures it's hidden from actual screenshots

/// Toggle overlay visibility
pub fn toggle(opacity: f32, allow_capture: bool) {
    if is_visible() {
        hide_overlay();
    } else {
        show_overlay(opacity, allow_capture);
    }
}
