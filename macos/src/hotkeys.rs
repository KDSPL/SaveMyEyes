// macOS global hotkeys using CGEventTap.
//
// Registers a Quartz event tap that intercepts key-down events system-wide.
// Modifier flags are checked to match Cmd+Opt+Arrow / Cmd+Opt+End patterns.
//
// Note: requires Accessibility permissions (System Settings > Privacy > Accessibility).

use std::sync::atomic::{AtomicBool, Ordering};

static REGISTERED: AtomicBool = AtomicBool::new(false);

/// Key codes (macOS virtual key codes)
const KEY_UP: u16 = 0x7E;
const KEY_DOWN: u16 = 0x7D;
const KEY_END: u16 = 0x77; // Fn+Right on Mac keyboards

/// Register global hotkeys via CGEventTap.
/// Equivalent shortcuts to Windows Ctrl+Alt mapping → Cmd+Option on macOS.
pub fn register_all() {
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(|| {
        install_event_tap();
    });
}

pub fn unregister_all() {
    REGISTERED.store(false, Ordering::SeqCst);
    // The event tap thread will notice and exit on next event
}

// ── CGEventTap internals ────────────────────────────────────────────────────

// These use raw Core Graphics FFI since objc2 doesn't wrap CGEventTap yet.

#[repr(C)]
#[allow(dead_code)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

type CGEventRef = *mut std::ffi::c_void;
type CFMachPortRef = *mut std::ffi::c_void;
type CFRunLoopSourceRef = *mut std::ffi::c_void;
type CGEventTapProxy = *mut std::ffi::c_void;

// CGEventTap types
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
const K_CG_EVENT_KEY_DOWN: u32 = 10;

// CGEventFlags
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 1 << 19;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,                // CGEventTapLocation
        place: u32,              // CGEventTapPlacement
        options: u32,            // CGEventTapOptions
        events_of_interest: u64, // CGEventMask
        callback: unsafe extern "C" fn(
            CGEventTapProxy,
            u32,       // CGEventType
            CGEventRef,
            *mut std::ffi::c_void,
        ) -> CGEventRef,
        user_info: *mut std::ffi::c_void,
    ) -> CFMachPortRef;

    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
    fn CFRunLoopAddSource(
        rl: *mut std::ffi::c_void,
        source: CFRunLoopSourceRef,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopRun();
}

extern "C" {
    static kCFRunLoopCommonModes: *const std::ffi::c_void;
}

// CGEventField for virtual keycode
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

unsafe extern "C" fn event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    _user_info: *mut std::ffi::c_void,
) -> CGEventRef {
    if event_type != K_CG_EVENT_KEY_DOWN {
        return event;
    }

    if !REGISTERED.load(Ordering::SeqCst) {
        return event;
    }

    let flags = CGEventGetFlags(event);
    let has_cmd = flags & K_CG_EVENT_FLAG_MASK_COMMAND != 0;
    let has_opt = flags & K_CG_EVENT_FLAG_MASK_ALTERNATE != 0;

    if !(has_cmd && has_opt) {
        return event;
    }

    let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;

    match keycode {
        KEY_END => {
            // Toggle dimmer
            crate::app::dispatch_hotkey(HotkeyAction::Toggle);
            std::ptr::null_mut() // consume the event
        }
        KEY_UP => {
            // Increase dimming
            crate::app::dispatch_hotkey(HotkeyAction::Increase);
            std::ptr::null_mut()
        }
        KEY_DOWN => {
            // Decrease dimming
            crate::app::dispatch_hotkey(HotkeyAction::Decrease);
            std::ptr::null_mut()
        }
        _ => event,
    }
}

fn install_event_tap() {
    let event_mask: u64 = 1 << K_CG_EVENT_KEY_DOWN;

    unsafe {
        let tap = CGEventTapCreate(
            0, // kCGHIDEventTap (session)
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_DEFAULT,
            event_mask,
            event_tap_callback,
            std::ptr::null_mut(),
        );

        if tap.is_null() {
            eprintln!("SaveMyEyes: Failed to create event tap. Grant Accessibility permissions.");
            return;
        }

        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

        CFRunLoopRun();
    }
}

/// Hotkey actions dispatched to the app module
#[derive(Debug, Clone, Copy)]
pub enum HotkeyAction {
    Toggle,
    Increase,
    Decrease,
}
