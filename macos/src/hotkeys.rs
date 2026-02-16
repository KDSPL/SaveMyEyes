// macOS global hotkeys using NSEvent global monitor + CGEventTap fallback.
//
// Primary: NSEvent.addGlobalMonitorForEventsMatchingMask
//   - Works without Accessibility permissions for monitoring
//   - Cannot consume events, but acceptable for our shortcuts
//
// Fallback: CGEventTap (handles events when our app is active)
//
// Hotkeys:
//   Cmd+Shift+D       -> Toggle dimmer
//   Cmd+Shift+>  (.)  -> Increase dimming
//   Cmd+Shift+<  (,)  -> Decrease dimming

use std::sync::atomic::{AtomicBool, Ordering};
use std::ptr::NonNull;

static REGISTERED: AtomicBool = AtomicBool::new(false);

/// Key codes (macOS virtual key codes)
const KEY_D: u16 = 0x02;
const KEY_PERIOD: u16 = 0x2F;
const KEY_COMMA: u16 = 0x2B;

/// Register global hotkeys via NSEvent global monitor.
/// Must be called from the main thread.
pub fn register_all() {
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    // NSEvent global monitor (runs on main thread, most reliable)
    install_ns_event_monitor();

    // CGEventTap as backup (handles events when our app is active)
    std::thread::spawn(|| {
        install_event_tap();
    });
}

#[allow(dead_code)]
pub fn unregister_all() {
    REGISTERED.store(false, Ordering::SeqCst);
}

/// Hotkey actions dispatched to the app module
#[derive(Debug, Clone, Copy)]
pub enum HotkeyAction {
    Toggle,
    Increase,
    Decrease,
}

// ---- NSEvent global monitor ------------------------------------------------

fn install_ns_event_monitor() {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};

    let handler = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
        let event: &NSEvent = unsafe { event.as_ref() };
        let flags = event.modifierFlags();
        let has_cmd = flags.contains(NSEventModifierFlags::Command);
        let has_shift = flags.contains(NSEventModifierFlags::Shift);

        if !(has_cmd && has_shift) {
            return;
        }

        let keycode = event.keyCode();

        match keycode {
            KEY_D => {
                eprintln!("SaveMyEyes: [NSEvent] Cmd+Shift+D detected");
                crate::app::dispatch_hotkey(HotkeyAction::Toggle);
            }
            KEY_PERIOD => {
                eprintln!("SaveMyEyes: [NSEvent] Cmd+Shift+> detected");
                crate::app::dispatch_hotkey(HotkeyAction::Increase);
            }
            KEY_COMMA => {
                eprintln!("SaveMyEyes: [NSEvent] Cmd+Shift+< detected");
                crate::app::dispatch_hotkey(HotkeyAction::Decrease);
            }
            _ => {}
        }
    });

    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::KeyDown,
        &handler,
    );

    if monitor.is_some() {
        eprintln!("SaveMyEyes: NSEvent global monitor installed successfully.");
    } else {
        eprintln!("SaveMyEyes: Failed to install NSEvent global monitor.");
    }

    // Keep the monitor alive for the lifetime of the app by leaking it
    std::mem::forget(monitor);
}

// ---- CGEventTap fallback ----------------------------------------------------

type CGEventRef = *mut std::ffi::c_void;
type CFMachPortRef = *mut std::ffi::c_void;
type CFRunLoopSourceRef = *mut std::ffi::c_void;
type CGEventTapProxy = *mut std::ffi::c_void;

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 1 << 17;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: unsafe extern "C" fn(
            CGEventTapProxy,
            u32,
            CGEventRef,
            *mut std::ffi::c_void,
        ) -> CGEventRef,
        user_info: *mut std::ffi::c_void,
    ) -> CFMachPortRef;

    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
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

unsafe extern "C" fn event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    _user_info: *mut std::ffi::c_void,
) -> CGEventRef {
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
        eprintln!("SaveMyEyes: CGEventTap disabled by timeout");
        return event;
    }

    if event_type != K_CG_EVENT_KEY_DOWN {
        return event;
    }

    let flags = CGEventGetFlags(event);
    let has_cmd = flags & K_CG_EVENT_FLAG_MASK_COMMAND != 0;
    let has_shift = flags & K_CG_EVENT_FLAG_MASK_SHIFT != 0;

    if !(has_cmd && has_shift) {
        return event;
    }

    let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;

    match keycode {
        KEY_D => {
            eprintln!("SaveMyEyes: [CGEventTap] Cmd+Shift+D detected");
            crate::app::dispatch_hotkey(HotkeyAction::Toggle);
        }
        KEY_PERIOD => {
            eprintln!("SaveMyEyes: [CGEventTap] Cmd+Shift+> detected");
            crate::app::dispatch_hotkey(HotkeyAction::Increase);
        }
        KEY_COMMA => {
            eprintln!("SaveMyEyes: [CGEventTap] Cmd+Shift+< detected");
            crate::app::dispatch_hotkey(HotkeyAction::Decrease);
        }
        _ => {}
    }

    event // listen-only, always pass through
}

fn install_event_tap() {
    request_accessibility_permission();

    let event_mask: u64 = 1 << K_CG_EVENT_KEY_DOWN;
    let max_attempts = 60;
    let mut attempt = 0;

    loop {
        attempt += 1;
        let tap = unsafe {
            CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                event_mask,
                event_tap_callback,
                std::ptr::null_mut(),
            )
        };

        if !tap.is_null() {
            eprintln!(
                "SaveMyEyes: CGEventTap created (listen-only, attempt {}).",
                attempt
            );
            unsafe {
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CFRunLoopRun();
            }
            eprintln!("SaveMyEyes: CGEventTap run loop exited, retrying...");
            continue;
        }

        if attempt >= max_attempts {
            eprintln!(
                "SaveMyEyes: CGEventTap unavailable after {} attempts. \
                 NSEvent monitor is still active.",
                max_attempts
            );
            return;
        }

        if attempt == 1 || attempt % 10 == 0 {
            eprintln!(
                "SaveMyEyes: Waiting for Accessibility permission... (attempt {}/{})",
                attempt, max_attempts
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

// ---- Accessibility permission -----------------------------------------------

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: *const std::ffi::c_void;
    fn CFDictionaryCreate(
        allocator: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
}

extern "C" {
    static kAXTrustedCheckOptionPrompt: *const std::ffi::c_void;
}

fn request_accessibility_permission() {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        let trusted = AXIsProcessTrustedWithOptions(options);
        if trusted {
            eprintln!("SaveMyEyes: Accessibility permission granted.");
        } else {
            eprintln!(
                "SaveMyEyes: Accessibility not yet granted. CGEventTap will retry."
            );
        }
    }
}

/// Check if accessibility permission is granted (without prompting).
pub fn is_accessibility_granted() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Prompt the user for accessibility permission if not already granted.
/// If permission is stale (binary changed, toggle shows ON but macOS
/// doesn't actually trust us), reset the TCC entry first so the user
/// sees a clean prompt instead of a confusingly-already-enabled toggle.
pub fn request_accessibility_if_needed() {
    if !is_accessibility_granted() {
        eprintln!("SaveMyEyes: Accessibility not granted — clearing stale TCC entry…");
        let _ = std::process::Command::new("tccutil")
            .args(["reset", "Accessibility", "com.kdspl.savemyeyes"])
            .output();
        eprintln!("SaveMyEyes: Prompting for accessibility permission…");
        request_accessibility_permission();
    } else {
        eprintln!("SaveMyEyes: Accessibility already granted.");
    }
}
