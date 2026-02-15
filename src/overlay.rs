// Screen dimmer using layered overlay windows with WDA_EXCLUDEFROMCAPTURE.
//
// Creates transparent, click-through, topmost overlay windows on each monitor.
// SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) tells the DWM to exclude
// these windows from screenshot and screen recording capture.
//
// Z-order strategy (debounced re-assertion):
//   • Overlay is created with WS_EX_TOPMOST (enters the topmost z-band)
//   • SetWinEventHook detects when another process's window takes foreground
//   • Instead of immediately re-asserting (which caused flickering), we DEBOUNCE:
//     we record the timestamp of the last event and wait 500ms after the LAST
//     event before re-asserting. This lets the window manager settle first.
//   • SWP_NOSENDCHANGING prevents notifying other apps of our re-topping.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, CS_HREDRAW,
    CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING,
    SWP_NOSIZE, SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WNDCLASSW, WS_DISABLED, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

// Thread-safe wrappers
struct HwndWrapper(isize);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

struct HookWrapper(isize);
unsafe impl Send for HookWrapper {}
unsafe impl Sync for HookWrapper {}

static OVERLAY_WINDOWS: Mutex<Vec<HwndWrapper>> = Mutex::new(Vec::new());
static CURRENT_OPACITY: Mutex<f32> = Mutex::new(0.3);
static ALLOW_CAPTURE: Mutex<bool> = Mutex::new(false);
static CLASS_REGISTERED: Mutex<bool> = Mutex::new(false);
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);
static EVENT_HOOK: Mutex<Option<HookWrapper>> = Mutex::new(None);

/// Timestamp (millis since epoch) of the last foreground event.
/// 0 means no pending re-assertion.
static REASSERT_REQUESTED_AT: AtomicU64 = AtomicU64::new(0);

/// How long to wait after the LAST foreground event before re-asserting (ms).
/// This debounce window lets the window manager settle, preventing z-order
/// ping-pong that causes flickering.
const DEBOUNCE_MS: u64 = 500;

const CLASS_NAME: &str = "SaveMyEyesOverlay\0";

// WinEvent constants
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Minimal window proc — no WM_WINDOWPOSCHANGING override.
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

/// Re-assert topmost on all overlay windows.
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

/// WinEvent callback — fired when another process's window takes the foreground.
/// Instead of immediately re-asserting (which causes flickering), we just
/// record the timestamp. A background thread checks this and waits for the
/// debounce period to elapse before actually re-asserting.
unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _event_time: u32,
) {
    // Record "re-assertion needed" with current timestamp.
    // Each new event resets the debounce timer.
    REASSERT_REQUESTED_AT.store(now_ms(), Ordering::SeqCst);
}

fn install_event_hook() {
    let mut hook_guard = EVENT_HOOK.lock().unwrap();
    if hook_guard.is_some() {
        return;
    }
    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
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
            let opacity = *CURRENT_OPACITY.lock().unwrap();
            let alpha = (opacity * 255.0) as u8;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);

            // Capture exclusion — ShareX, OBS, Snipping Tool, etc. won't see the dimming
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

/// Show overlay with given opacity on all monitors.
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

    // Install event hook for foreground changes
    install_event_hook();

    // Start the debounce + watchdog thread.
    // This single thread handles:
    //   1. Debounced z-order re-assertion (waits 500ms after last foreground event)
    //   2. Watchdog for externally destroyed windows (checks every 5s)
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            let mut watchdog_counter: u32 = 0;

            loop {
                // Poll every 200ms (fast enough for debounce, low CPU usage)
                std::thread::sleep(std::time::Duration::from_millis(200));

                let windows = OVERLAY_WINDOWS.lock().unwrap();
                if windows.is_empty() {
                    drop(windows);
                    WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
                    break;
                }
                drop(windows);

                // ── Debounced re-assertion ──
                let requested_at = REASSERT_REQUESTED_AT.load(Ordering::SeqCst);
                if requested_at > 0 {
                    let elapsed = now_ms().saturating_sub(requested_at);
                    if elapsed >= DEBOUNCE_MS {
                        // Enough time has passed since the last foreground event.
                        // The window manager has settled — safe to re-assert now.
                        REASSERT_REQUESTED_AT.store(0, Ordering::SeqCst);
                        reassert_topmost();
                    }
                    // else: still within debounce window, wait longer
                }

                // ── Watchdog (every 5s = 25 × 200ms) ──
                watchdog_counter += 1;
                if watchdog_counter >= 25 {
                    watchdog_counter = 0;

                    let windows = OVERLAY_WINDOWS.lock().unwrap();
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
                        // Tear down and rebuild
                        {
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
                            install_event_hook();
                        }
                    }
                }
            }
        });
    }
}

/// Hide overlay windows and clean up hooks
pub fn hide_overlay() {
    uninstall_event_hook();
    REASSERT_REQUESTED_AT.store(0, Ordering::SeqCst);

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
/// Also re-asserts topmost (this is an explicit user action).
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

    // Re-assert on user-initiated change
    reassert_topmost();
}

/// Check if overlay is visible
pub fn is_visible() -> bool {
    !OVERLAY_WINDOWS.lock().unwrap().is_empty()
}
