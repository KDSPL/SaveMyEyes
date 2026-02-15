// Screen dimmer implementation using the Windows Magnification API.
//
// Primary method: MagSetFullscreenColorEffect
// This applies a 5×5 color transformation matrix at the DWM compositor level.
// Because it operates below the window manager, there are NO overlay windows
// and therefore ZERO z-order issues — completely flicker-free.
//
// Fallback method: Layered overlay windows (used if Magnification API fails)
// Creates WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST windows.
// This has inherent z-order flickering potential with Chrome and similar apps.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Magnification::{
    MagInitialize, MagSetFullscreenColorEffect, MagUninitialize, MAGCOLOREFFECT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, CS_HREDRAW,
    CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING,
    SWP_NOSIZE, SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WNDCLASSW, WS_DISABLED, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

// ── Shared state ──────────────────────────────────────────────────────

// Thread-safe wrappers for raw pointers
struct HwndWrapper(isize);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

struct HookWrapper(isize);
unsafe impl Send for HookWrapper {}
unsafe impl Sync for HookWrapper {}

/// Which dimming backend is active
#[derive(Clone, Copy, PartialEq)]
enum DimMethod {
    None,
    Magnification,
    Overlay,
}

static DIM_METHOD: Mutex<DimMethod> = Mutex::new(DimMethod::None);
static OVERLAY_WINDOWS: Mutex<Vec<HwndWrapper>> = Mutex::new(Vec::new());
static CURRENT_OPACITY: Mutex<f32> = Mutex::new(0.3);
static ALLOW_CAPTURE: Mutex<bool> = Mutex::new(false);
static CLASS_REGISTERED: Mutex<bool> = Mutex::new(false);
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);
static EVENT_HOOK: Mutex<Option<HookWrapper>> = Mutex::new(None);
static MAG_INITIALIZED: AtomicBool = AtomicBool::new(false);

const CLASS_NAME: &str = "SaveMyEyesOverlay\0";

// WinEvent constants
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MOVESIZEEND: u32 = 0x000B;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

// ── Magnification API (primary, flicker-free) ─────────────────────────

/// Build a 5×5 color matrix that dims the screen.
/// The identity matrix is:
///   [1 0 0 0 0]    R
///   [0 1 0 0 0]    G
///   [0 0 1 0 0]    B
///   [0 0 0 1 0]    A
///   [0 0 0 0 1]    bias
/// We scale R, G, B by `brightness` (1.0 = full bright, 0.1 = very dim).
fn make_dim_matrix(opacity: f32) -> MAGCOLOREFFECT {
    // opacity is 0.0..0.9 where higher = darker
    // brightness = 1.0 - opacity (e.g., opacity 0.5 → brightness 0.5)
    let brightness = (1.0 - opacity).clamp(0.1, 1.0);

    let mut transform = [0.0f32; 25];
    // Row-major 5×5 matrix
    transform[0] = brightness; // R → R
    transform[6] = brightness; // G → G
    transform[12] = brightness; // B → B
    transform[18] = 1.0; // A → A
    transform[24] = 1.0; // bias → bias

    MAGCOLOREFFECT { transform }
}

/// Identity matrix (restores normal colors)
fn identity_matrix() -> MAGCOLOREFFECT {
    let mut transform = [0.0f32; 25];
    transform[0] = 1.0;
    transform[6] = 1.0;
    transform[12] = 1.0;
    transform[18] = 1.0;
    transform[24] = 1.0;
    MAGCOLOREFFECT { transform }
}

/// Try to apply dimming via the Magnification API.
/// Returns true if successful.
fn mag_set_dim(opacity: f32) -> bool {
    // Initialize Magnification API if needed
    if !MAG_INITIALIZED.load(Ordering::SeqCst) {
        let ok = unsafe { MagInitialize().as_bool() };
        if !ok {
            return false;
        }
        MAG_INITIALIZED.store(true, Ordering::SeqCst);
    }

    let effect = make_dim_matrix(opacity);
    unsafe { MagSetFullscreenColorEffect(&effect).as_bool() }
}

/// Remove the Magnification color effect (restore normal colors)
fn mag_clear() {
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        let identity = identity_matrix();
        unsafe {
            let _ = MagSetFullscreenColorEffect(&identity);
            let _ = MagUninitialize();
        }
        MAG_INITIALIZED.store(false, Ordering::SeqCst);
    }
}

// ── Overlay fallback ──────────────────────────────────────────────────

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

fn reassert_topmost() {
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

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _event_time: u32,
) {
    reassert_topmost();
}

fn install_event_hook() {
    let mut hook_guard = EVENT_HOOK.lock().unwrap();
    if hook_guard.is_some() {
        return;
    }
    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MOVESIZEEND,
            None,
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if !hook.is_invalid() {
            *hook_guard = Some(HookWrapper(hook.0 as isize));
        }
    }
}

fn uninstall_event_hook() {
    let mut hook_guard = EVENT_HOOK.lock().unwrap();
    if let Some(wrapper) = hook_guard.take() {
        unsafe {
            let hook = HWINEVENTHOOK(wrapper.0 as *mut std::ffi::c_void);
            let _ = UnhookWinEvent(hook);
        }
    }
}

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

fn show_overlay_fallback() {
    if !register_class() {
        return;
    }

    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(0));
    }

    install_event_hook();

    // Watchdog for externally destroyed windows
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(3));

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
                hide_overlay_windows();
                if register_class() {
                    *CURRENT_OPACITY.lock().unwrap() = opacity;
                    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;
                    unsafe {
                        let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(0));
                    }
                    install_event_hook();
                }
            }
        });
    }
}

fn hide_overlay_windows() {
    uninstall_event_hook();
    let mut windows = OVERLAY_WINDOWS.lock().unwrap();
    for wrapper in windows.drain(..) {
        unsafe {
            let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = DestroyWindow(hwnd);
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────

/// Show the dimmer with given opacity on all monitors.
/// Tries the Magnification API first (flicker-free), falls back to overlay.
pub fn show_overlay(opacity: f32, allow_capture: bool) {
    let opacity = opacity.clamp(0.0, 0.9);
    *CURRENT_OPACITY.lock().unwrap() = opacity;
    *ALLOW_CAPTURE.lock().unwrap() = allow_capture;

    // Clean up any existing dimming
    hide_overlay();

    // Try Magnification API first (compositor-level, flicker-free)
    if mag_set_dim(opacity) {
        *DIM_METHOD.lock().unwrap() = DimMethod::Magnification;
        return;
    }

    // Fall back to overlay windows
    *DIM_METHOD.lock().unwrap() = DimMethod::Overlay;
    show_overlay_fallback();
}

/// Hide the dimmer
pub fn hide_overlay() {
    let method = *DIM_METHOD.lock().unwrap();
    match method {
        DimMethod::Magnification => {
            mag_clear();
        }
        DimMethod::Overlay => {
            hide_overlay_windows();
        }
        DimMethod::None => {}
    }
    *DIM_METHOD.lock().unwrap() = DimMethod::None;
}

/// Update dimmer opacity
pub fn set_opacity(opacity: f32) {
    let opacity = opacity.clamp(0.0, 0.9);
    *CURRENT_OPACITY.lock().unwrap() = opacity;

    let method = *DIM_METHOD.lock().unwrap();
    match method {
        DimMethod::Magnification => {
            let _ = mag_set_dim(opacity);
        }
        DimMethod::Overlay => {
            let alpha = (opacity * 255.0) as u8;
            let windows = OVERLAY_WINDOWS.lock().unwrap();
            for wrapper in windows.iter() {
                unsafe {
                    let hwnd = HWND(wrapper.0 as *mut std::ffi::c_void);
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
            }
        }
        DimMethod::None => {}
    }
}

/// Check if dimmer is active
pub fn is_visible() -> bool {
    *DIM_METHOD.lock().unwrap() != DimMethod::None
}
